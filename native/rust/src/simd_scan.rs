//! Stage-1 structural scanning.
//!
//! Exposes two entry points:
//!   - `find_structurals_scalar`   – portable, always available
//!   - `find_structurals_simd`     – runtime-dispatched; falls back to scalar
//!                                   on CPUs without the required feature set
//!
//! The SIMD path uses AVX2 (x86_64) or NEON (aarch64). On every other target
//! it silently uses the scalar path.
//!
//! Structural bytes recognised:
//!   `"  ,  -  0-9  :  [  ]  f  n  t  {  }`
//!
//! Whitespace helpers used by the parser are also exported here.

// ─────────────────────────────────────────────────────────────────────────────
// Portable helpers
// ─────────────────────────────────────────────────────────────────────────────

#[inline(always)]
pub fn is_structural(b: u8) -> bool {
    matches!(
        b,
        b'"' | b',' | b'-' | b'0'..=b'9' | b':' | b'[' | b']' | b'f' | b'n' | b't' | b'{' | b'}'
    )
}

#[inline(always)]
pub fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

// ─────────────────────────────────────────────────────────────────────────────
// Scalar path
// ─────────────────────────────────────────────────────────────────────────────

/// Scalar structural scan.  LLVM auto-vectorises this at -O3 on most targets.
pub fn find_structurals_scalar(input: &[u8], out: &mut Vec<u32>) {
    for (i, &b) in input.iter().enumerate() {
        if is_structural(b) {
            out.push(i as u32);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AVX2 path (x86_64)
// ─────────────────────────────────────────────────────────────────────────────
//
// Strategy: process 32 bytes per iteration.
//
// We classify each byte against a set of ranges and exact values using the
// AVX2 shuffle-based lookup technique (the same approach simdjson uses):
//
//  1. Split each byte into low nibble (lo4) and high nibble (hi4).
//  2. Use VPSHUFB to look up a "row mask" from lo4.
//  3. Use VPSHUFB to look up a "column mask" from hi4.
//  4. AND the two masks: a non-zero result means structural.
//  5. Pack the 32 comparison results into a 32-bit bitmask with VPMOVMSKB.
//  6. Iterate over set bits with TZCNT to emit indices.
//
// The row/column tables below encode exactly the structural byte set.
//
// Structural bytes (hex):
//   22 " | 2C , | 2D - | 30-39 0-9 | 3A : | 5B [ | 5D ] | 66 f | 6E n | 74 t | 7B { | 7D }
//
// lo4 → row mask (which hi4 nibbles are structural for this lo4):
//   lo4=2: hi4 ∈ {2}          → bit 2         → 0b0000_0100 = 0x04
//   lo4=3: hi4 ∈ {2,3}        → bits 2,3      → 0b0000_1100 = 0x0C  (but 3x only 0-9, so hi4=3)
//   lo4=4: hi4 ∈ {7}          → bit 7         → 0b1000_0000 = 0x80  (t=0x74)
//   lo4=5: hi4 ∈ {3}          → bit 3         → 0b0000_1000 = 0x08  (5=0x35)
//   lo4=6: hi4 ∈ {6}          → bit 6         → 0b0100_0000 = 0x40  (f=0x66)
//   lo4=7: hi4 ∈ {3}          → bit 3         → 0b0000_1000 = 0x08  (7=0x37)
//   lo4=8: hi4 ∈ {3}          → bit 3         → 0b0000_1000 = 0x08  (8=0x38)
//   lo4=9: hi4 ∈ {3}          → bit 3         → 0b0000_1000 = 0x08  (9=0x39)
//   lo4=A: hi4 ∈ {3}          → bit 3         → 0b0000_1000 = 0x08  (:=0x3A)
//   lo4=B: hi4 ∈ {5,7}        → bits 5,7      → 0b1010_0000 = 0xA0  ([=0x5B, {=0x7B)
//   lo4=C: hi4 ∈ {2}          → bit 2         → 0b0000_0100 = 0x04  (,=0x2C)
//   lo4=D: hi4 ∈ {2,5,7}      → bits 2,5,7    → 0b1010_0100 = 0xA4  (-=0x2D, ]=0x5D, }=0x7D)
//   lo4=E: hi4 ∈ {6}          → bit 6         → 0b0100_0000 = 0x40  (n=0x6E)
//   lo4=0,1,F: no match        → 0x00
//
// hi4 → col mask (which lo4 nibbles are structural for this hi4):
//   hi4=2: lo4 ∈ {2,C,D}      → bits 2,12,13  → 0b0011_0000_0000_0100 — but we only need 8 bits
//          We build the col table to have a set bit at position hi4 if that hi4 participates.
//          Simpler: col[hi4] = 0xFF if hi4 is any participating high nibble, else 0.
//          Participating hi4 values: 2,3,5,6,7.
//
// Rather than a pure row/col split, we use the well-known "low nibble table + high nibble
// comparison" approach where:
//   row_table[lo4]  = a bitmask of which high nibbles are valid for this lo4
//   col_table[hi4]  = a single-bit mask at position hi4 (i.e. 1 << hi4)
//
// Then: (row_table[lo4] & col_table[hi4]) != 0  ⟺  byte is structural.

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_structurals_avx2(input: &[u8], out: &mut Vec<u32>) {
    use std::arch::x86_64::*;

    // Row table: for each possible low nibble value (0..=15), which high nibble
    // bit-positions are structural partners?
    //
    // Index = lo4 value, value = bitmask over hi4 (bit k set ⟹ hi4=k is structural
    // when paired with this lo4).
    //
    // Derivation from the structural set above:
    //   lo4  structurals with this lo4       hi4 values   bitmask
    //   0x0  (none)                           -            0x00
    //   0x1  (none)                           -            0x00
    //   0x2  " (0x22)                         2            0x04
    //   0x3  (none extra — 0x30-0x39 lo4=0-9)             0x00
    //   0x4  t (0x74)                         7            0x80
    //   0x5  5,6,7,8,9 (0x35-0x39 overlap — handled via lo4=5..9 all hi4=3)
    //        but 0x35='5' lo4=5 hi4=3        3            0x08
    //   0x6  f (0x66)                         6            0x40
    //   0x7  7 (0x37)                         3            0x08
    //   0x8  8 (0x38)                         3            0x08
    //   0x9  9 (0x39)                         3            0x08
    //   0xA  : (0x3A)                         3            0x08
    //   0xB  [ (0x5B), { (0x7B)              5,7           0xA0
    //   0xC  , (0x2C)                         2            0x04
    //   0xD  - (0x2D), ] (0x5D), } (0x7D)   2,5,7         0xA4
    //   0xE  n (0x6E)                         6            0x40
    //   0xF  (none)                           -            0x00
    //
    // Note: 0-9 digits: 0x30..=0x39 → lo4=0..=9, all with hi4=3.
    //   lo4=0 hi4=3: digit '0' (0x30)   → 0x08
    //   lo4=1 hi4=3: digit '1' (0x31)   → 0x08
    //   lo4=2 hi4=3: digit '2' (0x32)   → 0x08  (but lo4=2 already has hi4=2 for '"')
    //   lo4=3 hi4=3: digit '3' (0x33)   → 0x08
    //   ... and so on
    //
    // Merged table:
    //   lo4=0: 0x08  (digit 0)
    //   lo4=1: 0x08  (digit 1)
    //   lo4=2: 0x08 | 0x04 = 0x0C  (digit 2, ")
    //   lo4=3: 0x08  (digit 3)
    //   lo4=4: 0x08 | 0x80 = 0x88  (digit 4, t)
    //   lo4=5: 0x08  (digit 5)
    //   lo4=6: 0x08 | 0x40 = 0x48  (digit 6, f)
    //   lo4=7: 0x08  (digit 7)
    //   lo4=8: 0x08  (digit 8)
    //   lo4=9: 0x08  (digit 9)
    //   lo4=A: 0x08  (:)
    //   lo4=B: 0xA0  ([, {)
    //   lo4=C: 0x04  (,)
    //   lo4=D: 0xA4  (-, ], })
    //   lo4=E: 0x40  (n)
    //   lo4=F: 0x00
    #[rustfmt::skip]
    let row_table: [u8; 32] = [
        0x08, 0x08, 0x0C, 0x08, 0x88, 0x08, 0x48, 0x08,
        0x08, 0x08, 0x08, 0xA0, 0x04, 0xA4, 0x40, 0x00,
        // Second 16 bytes are a mirror (VPSHUFB wraps mod 16, but we keep
        // indices 0-15 correct; the upper 16 of a 256-bit lane are independent).
        0x08, 0x08, 0x0C, 0x08, 0x88, 0x08, 0x48, 0x08,
        0x08, 0x08, 0x08, 0xA0, 0x04, 0xA4, 0x40, 0x00,
    ];

    // Col table: for each high nibble value (0..=15), the single bit at position hi4.
    // col[hi4] = 1 << hi4  clamped to u8.
    // hi4 values that appear in the structural set: 2,3,5,6,7.
    // All others produce a bit that will never appear in the row table values,
    // so the AND will be 0.
    //
    // 1<<2=0x04, 1<<3=0x08, 1<<5=0x20, 1<<6=0x40, 1<<7=0x80
    // For hi4 > 7 the shift would exceed u8; those bytes can't be structural
    // (all our structurals have hi4 ≤ 7), so we use 0x00 for hi4 ≥ 8.
    #[rustfmt::skip]
    let col_table: [u8; 32] = [
        0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let row_v = unsafe { _mm256_loadu_si256(row_table.as_ptr() as *const __m256i) };
    let col_v = unsafe { _mm256_loadu_si256(col_table.as_ptr() as *const __m256i) };
    let low_mask = _mm256_set1_epi8(0x0F_u8 as i8);

    let len = input.len();
    let mut i = 0usize;

    // Main loop: 32 bytes per iteration.
    while i + 32 <= len {
        let chunk = unsafe { _mm256_loadu_si256(input.as_ptr().add(i) as *const __m256i) };

        // lo4 = chunk & 0x0F
        let lo4 = _mm256_and_si256(chunk, low_mask);
        // hi4 = (chunk >> 4) & 0x0F
        let hi4 = _mm256_and_si256(_mm256_srli_epi16(chunk, 4), low_mask);

        // Lookup row and col masks.
        let row = _mm256_shuffle_epi8(row_v, lo4);
        let col = _mm256_shuffle_epi8(col_v, hi4);

        // AND: non-zero iff structural.
        let is_structural_v = _mm256_andnot_si256(
            _mm256_cmpeq_epi8(_mm256_and_si256(row, col), _mm256_setzero_si256()),
            _mm256_set1_epi8(-1i8), // all ones
        );

        // Build 32-bit bitmask: bit k set iff byte k is structural.
        let mut mask = _mm256_movemask_epi8(is_structural_v) as u32;

        // Emit indices for set bits.
        while mask != 0 {
            let bit = mask.trailing_zeros();
            out.push((i + bit as usize) as u32);
            mask &= mask - 1; // clear lowest set bit
        }

        i += 32;
    }

    // Scalar tail for the remaining < 32 bytes.
    for j in i..len {
        if is_structural(input[j]) {
            out.push(j as u32);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NEON path (aarch64)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn find_structurals_neon(input: &[u8], out: &mut Vec<u32>) {
    use std::arch::aarch64::*;

    // NEON processes 16 bytes per iteration using the same nibble-table trick.
    // vtbl1q_u8 is the NEON equivalent of VPSHUFB.

    // Row table (16 bytes — NEON lane is 128-bit).
    #[rustfmt::skip]
    let row_table: [u8; 16] = [
        0x08, 0x08, 0x0C, 0x08, 0x88, 0x08, 0x48, 0x08,
        0x08, 0x08, 0x08, 0xA0, 0x04, 0xA4, 0x40, 0x00,
    ];
    #[rustfmt::skip]
    let col_table: [u8; 16] = [
        0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let row_v = vld1q_u8(row_table.as_ptr());
    let col_v = vld1q_u8(col_table.as_ptr());
    let low_mask = vdupq_n_u8(0x0F);

    let len = input.len();
    let mut i = 0usize;

    while i + 16 <= len {
        let chunk = vld1q_u8(input.as_ptr().add(i));

        let lo4 = vandq_u8(chunk, low_mask);
        let hi4 = vandq_u8(vshrq_n_u8(chunk, 4), low_mask);

        let row = vqtbl1q_u8(row_v, lo4);
        let col = vqtbl1q_u8(col_v, hi4);

        // non-zero iff structural
        let matched = vtstq_u8(row, col);

        // Emit indices. NEON has no movemask; iterate bytes.
        // We store the 16-byte mask and loop — still ~8x fewer iterations than scalar.
        let mut buf = [0u8; 16];
        vst1q_u8(buf.as_mut_ptr(), matched);
        for k in 0..16usize {
            if buf[k] != 0 {
                out.push((i + k) as u32);
            }
        }

        i += 16;
    }

    for j in i..len {
        if is_structural(input[j]) {
            out.push(j as u32);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// SIMD-accelerated structural scan.  Automatically dispatches to AVX2 (x86_64),
/// NEON (aarch64), or the scalar fallback depending on CPU capabilities detected
/// at runtime.
pub fn find_structurals_simd(input: &[u8], out: &mut Vec<u32>) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: we just verified AVX2 is available.
            unsafe {
                find_structurals_avx2(input, out);
            }
            return;
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // NEON is mandatory on aarch64; no runtime check needed.
        unsafe {
            find_structurals_neon(input, out);
        }
        return;
    }

    // Portable fallback.
    #[allow(unreachable_code)]
    find_structurals_scalar(input, out);
}

// ─────────────────────────────────────────────────────────────────────────────
// SIMD string-end scanner (used by SimdParser)
// ─────────────────────────────────────────────────────────────────────────────
//
// Scans forward from `start` looking for the first unescaped `"` or `\`.
// Returns the index of that byte within `input`, or `None` if not found.
//
// For simple strings (no escapes), the caller bulk-copies bytes directly from
// `input` into the string pool, avoiding the per-byte loop in the scalar path.

/// Find the first `"` or `\` in `input[start..]`.
/// Returns `Some(absolute_index)` or `None` if neither is found before `input.len()`.
pub fn find_string_end_simd(input: &[u8], start: usize) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { find_string_end_avx2(input, start) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { find_string_end_neon(input, start) };
    }

    #[allow(unreachable_code)]
    find_string_end_scalar(input, start)
}

pub fn find_string_end_scalar(input: &[u8], start: usize) -> Option<usize> {
    for i in start..input.len() {
        let b = input[i];
        if b == b'"' || b == b'\\' {
            return Some(i);
        }
    }
    None
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn find_string_end_avx2(input: &[u8], start: usize) -> Option<usize> {
    use std::arch::x86_64::*;
    let quote = _mm256_set1_epi8(b'"' as i8);
    let backslash = _mm256_set1_epi8(b'\\' as i8);

    let len = input.len();
    let mut i = start;

    while i + 32 <= len {
        let chunk = unsafe { _mm256_loadu_si256(input.as_ptr().add(i) as *const __m256i) };
        let eq_quote = _mm256_cmpeq_epi8(chunk, quote);
        let eq_bs = _mm256_cmpeq_epi8(chunk, backslash);
        let combined = _mm256_or_si256(eq_quote, eq_bs);
        let mask = _mm256_movemask_epi8(combined) as u32;
        if mask != 0 {
            return Some(i + mask.trailing_zeros() as usize);
        }
        i += 32;
    }

    // Scalar tail.
    for j in i..len {
        let b = input[j];
        if b == b'"' || b == b'\\' {
            return Some(j);
        }
    }
    None
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn find_string_end_neon(input: &[u8], start: usize) -> Option<usize> {
    use std::arch::aarch64::*;
    let quote = vdupq_n_u8(b'"');
    let backslash = vdupq_n_u8(b'\\');

    let len = input.len();
    let mut i = start;

    while i + 16 <= len {
        let chunk = vld1q_u8(input.as_ptr().add(i));
        let eq_q = vceqq_u8(chunk, quote);
        let eq_bs = vceqq_u8(chunk, backslash);
        let combined = vorrq_u8(eq_q, eq_bs);

        let mut buf = [0u8; 16];
        vst1q_u8(buf.as_mut_ptr(), combined);
        for k in 0..16usize {
            if buf[k] != 0 {
                return Some(i + k);
            }
        }
        i += 16;
    }

    for j in i..len {
        let b = input[j];
        if b == b'"' || b == b'\\' {
            return Some(j);
        }
    }
    None
}
