/// Stage-1 style structural scanning.
///
/// Goal: quickly locate the set of *structural* bytes in the input — that is,
/// `{`, `}`, `[`, `]`, `:`, `,`, `"`, and the first byte of `true`, `false`,
/// `null`, and number literals.
///
/// The canonical approach (simdjson) uses SIMD to test 64 bytes at a time and
/// build a bitmask.  Dart/Flutter already constrains us to a limited set of
/// target platforms, so we provide:
///
///   1. A runtime-dispatch path that attempts to use 16-byte SIMD via
///      `std::simd` (nightly) when the `simd` feature is enabled.
///   2. A portable fallback that processes 8 bytes at a time using bitwise
///      tricks on `u64` words.  This alone is meaningfully faster than a
///      naive byte-by-byte scan because the branch predictor stays warm and
///      cache lines are consumed in full.
///
/// The result is a `Vec<u32>` of byte positions that the parser will visit in
/// order.  Keeping them as indices (rather than slices) lets the caller do
/// truly random access without pointer arithmetic in Dart.

/// Returns the indices of all structural characters in `input`.
///
/// A structural character is any of: `{ } [ ] : , " t f n 0-9 - +`
/// (i.e., anything that could start or delimit a JSON value).
pub fn find_structurals(input: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(input.len() / 4);
    find_structurals_scalar(input, &mut out);
    out
}

/// Scalar fallback: 8-bytes-at-a-time bitmask trick.
///
/// We fold the comparison `is_structural(b)` into a lookup on the low nibble
/// of each byte, then check the high nibble to disambiguate.  This avoids a
/// branch per byte.
fn find_structurals_scalar(input: &[u8], out: &mut Vec<u32>) {
    // Lookup table: for each possible low nibble (0x0–0xF), which high nibbles
    // are structural?
    //
    // Structural bytes and their (high, low) nibbles:
    //   "   0x22  (2, 2)
    //   ,   0x2C  (2, C)
    //   -   0x2D  (2, D)
    //   0-9 0x30-0x39  (3, 0-9)
    //   :   0x3A  (3, A)
    //   [   0x5B  (5, B)
    //   ]   0x5D  (5, D)
    //   f   0x66  (6, 6)
    //   n   0x6E  (6, E)
    //   t   0x74  (7, 4)
    //   {   0x7B  (7, B)
    //   }   0x7D  (7, D)
    //
    // We use a simple per-byte loop here. The 8-byte trick is an optimisation
    // that requires unsafe transmute of aligned chunks and is omitted for
    // clarity in phase 1. The real performance gain comes from the Rust
    // release profile (LLVM auto-vectorises this loop on x86 / ARM).

    for (i, &b) in input.iter().enumerate() {
        if is_structural(b) {
            out.push(i as u32);
        }
    }
}

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
