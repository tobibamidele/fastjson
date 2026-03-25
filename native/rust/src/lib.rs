mod decode;
mod parser;
mod simd_scan;
mod tape;

use decode::{Value, skip_value};
use parser::{parse, parse_simd};
use tape::Tape;

// ─────────────────────────────────────────────────────────────────────────────
// Error codes
// ─────────────────────────────────────────────────────────────────────────────

#[repr(C)]
pub enum FjError {
    Ok = 0,
    ParseError = 1,
    BadHandle = 2,
    WrongType = 3,
    KeyNotFound = 4,
    OutOfBounds = 5,
    NullPointer = 6,
}

// ─────────────────────────────────────────────────────────────────────────────
// Parse / free
// ─────────────────────────────────────────────────────────────────────────────

/// Scalar parser entry point.
///
/// # Safety
/// `ptr` must point to at least `len` readable bytes.  `out` must be a valid
/// non-null writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_parse(ptr: *const u8, len: usize, out: *mut *mut Tape) -> FjError {
    if ptr.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    let input = std::slice::from_raw_parts(ptr, len);
    match parse(input) {
        Ok(t) => {
            *out = Box::into_raw(Box::new(t));
            FjError::Ok
        }
        Err(_) => {
            *out = std::ptr::null_mut();
            FjError::ParseError
        }
    }
}

/// SIMD-accelerated parser entry point.
///
/// Uses AVX2 on x86_64, NEON on aarch64, and falls back to the scalar path on
/// other targets.  Produces an identical `Tape` to `fj_parse`; only the
/// execution path differs.
///
/// # Safety
/// Same contract as `fj_parse`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_parse_simd(ptr: *const u8, len: usize, out: *mut *mut Tape) -> FjError {
    if ptr.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    let input = std::slice::from_raw_parts(ptr, len);
    match parse_simd(input) {
        Ok(t) => {
            *out = Box::into_raw(Box::new(t));
            FjError::Ok
        }
        Err(_) => {
            *out = std::ptr::null_mut();
            FjError::ParseError
        }
    }
}

/// Free a document handle returned by `fj_parse` or `fj_parse_simd`.
///
/// # Safety
/// `handle` must have been returned by one of the parse functions and must not
/// be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_free(handle: *mut Tape) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scalar accessors
// ─────────────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_tape_len(handle: *mut Tape) -> usize {
    if handle.is_null() {
        return 0;
    }
    (*handle).words.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_tag(handle: *mut Tape, idx: usize, out: *mut u8) -> FjError {
    if handle.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    match (&(*handle).words).get(idx) {
        Some(&w) => {
            *out = tape::entry_tag(w);
            FjError::Ok
        }
        None => FjError::OutOfBounds,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_get_i64(handle: *mut Tape, idx: usize, out: *mut i64) -> FjError {
    if handle.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    match Value::new(&*handle, idx).as_i64() {
        Ok(n) => {
            *out = n;
            FjError::Ok
        }
        Err(decode::DecodeError::WrongType { .. }) => FjError::WrongType,
        Err(_) => FjError::OutOfBounds,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_get_f64(handle: *mut Tape, idx: usize, out: *mut f64) -> FjError {
    if handle.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    match Value::new(&*handle, idx).as_f64() {
        Ok(n) => {
            *out = n;
            FjError::Ok
        }
        Err(decode::DecodeError::WrongType { .. }) => FjError::WrongType,
        Err(_) => FjError::OutOfBounds,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_get_bool(handle: *mut Tape, idx: usize, out: *mut u8) -> FjError {
    if handle.is_null() || out.is_null() {
        return FjError::NullPointer;
    }
    match Value::new(&*handle, idx).as_bool() {
        Ok(b) => {
            *out = b as u8;
            FjError::Ok
        }
        Err(decode::DecodeError::WrongType { .. }) => FjError::WrongType,
        Err(_) => FjError::OutOfBounds,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_get_str(
    handle: *mut Tape,
    idx: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> FjError {
    if handle.is_null() || out_ptr.is_null() || out_len.is_null() {
        return FjError::NullPointer;
    }
    match Value::new(&*handle, idx).as_str_bytes() {
        Ok(b) => {
            *out_ptr = b.as_ptr();
            *out_len = b.len();
            FjError::Ok
        }
        Err(decode::DecodeError::WrongType { .. }) => FjError::WrongType,
        Err(_) => FjError::OutOfBounds,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Navigation
// ─────────────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_get_field(
    handle: *mut Tape,
    obj_idx: usize,
    key_ptr: *const u8,
    key_len: usize,
    out_idx: *mut usize,
) -> FjError {
    if handle.is_null() || key_ptr.is_null() || out_idx.is_null() {
        return FjError::NullPointer;
    }
    let key = std::slice::from_raw_parts(key_ptr, key_len);
    match Value::new(&*handle, obj_idx).get_field(key) {
        Ok(v) => {
            *out_idx = v.tape_idx();
            FjError::Ok
        }
        Err(decode::DecodeError::KeyNotFound) => FjError::KeyNotFound,
        Err(decode::DecodeError::WrongType { .. }) => FjError::WrongType,
        Err(_) => FjError::OutOfBounds,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_array_first(
    handle: *mut Tape,
    arr_idx: usize,
    out_first: *mut usize,
) -> FjError {
    if handle.is_null() || out_first.is_null() {
        return FjError::NullPointer;
    }
    let tape = &*handle;
    let w = match tape.words.get(arr_idx) {
        Some(&w) => w,
        None => return FjError::OutOfBounds,
    };
    if tape::entry_tag(w) != tape::TAG_ARRAY {
        return FjError::WrongType;
    }
    let end = tape::entry_payload(w) as usize;
    let first = arr_idx + 1;
    if first >= end {
        return FjError::OutOfBounds;
    }
    *out_first = first;
    FjError::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_next_sibling(
    handle: *mut Tape,
    current_idx: usize,
    container_end_idx: usize,
    out_next: *mut usize,
) -> FjError {
    if handle.is_null() || out_next.is_null() {
        return FjError::NullPointer;
    }
    let next = skip_value(&*handle, current_idx);
    if next >= container_end_idx {
        return FjError::OutOfBounds;
    }
    *out_next = next;
    FjError::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_container_end(
    handle: *mut Tape,
    idx: usize,
    out_end: *mut usize,
) -> FjError {
    if handle.is_null() || out_end.is_null() {
        return FjError::NullPointer;
    }
    let tape = &*handle;
    match tape.words.get(idx) {
        Some(&w) if matches!(tape::entry_tag(w), tape::TAG_OBJECT | tape::TAG_ARRAY) => {
            *out_end = tape::entry_payload(w) as usize;
            FjError::Ok
        }
        Some(_) => FjError::WrongType,
        None => FjError::OutOfBounds,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Crate-private helpers
// ─────────────────────────────────────────────────────────────────────────────

impl<'t> Value<'t> {
    pub(crate) fn tape_idx(&self) -> usize {
        self.idx
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use parser::{parse, parse_simd};

    fn tape_words(input: &[u8], simd: bool) -> Vec<u64> {
        let t = if simd {
            parse_simd(input)
        } else {
            parse(input)
        }
        .expect("parse failed");
        t.words
    }

    /// Both paths must produce exactly the same tape for any valid input.
    fn assert_same_tape(json: &[u8]) {
        let scalar = tape_words(json, false);
        let simd = tape_words(json, true);
        assert_eq!(
            scalar,
            simd,
            "tape mismatch for: {}",
            std::str::from_utf8(json).unwrap_or("<binary>")
        );
    }

    #[test]
    fn empty_object() {
        assert_same_tape(b"{}");
    }
    #[test]
    fn empty_array() {
        assert_same_tape(b"[]");
    }
    #[test]
    fn null_value() {
        assert_same_tape(b"null");
    }
    #[test]
    fn true_value() {
        assert_same_tape(b"true");
    }
    #[test]
    fn false_value() {
        assert_same_tape(b"false");
    }
    #[test]
    fn integer() {
        assert_same_tape(b"42");
    }
    #[test]
    fn negative() {
        assert_same_tape(b"-1");
    }
    #[test]
    fn float() {
        assert_same_tape(b"3.14");
    }
    #[test]
    fn float_exp() {
        assert_same_tape(b"1.5e10");
    }
    #[test]
    fn simple_string() {
        assert_same_tape(br#""hello""#);
    }
    #[test]
    fn escape_string() {
        assert_same_tape(br#""hel\"lo""#);
    }
    #[test]
    fn unicode_escape() {
        assert_same_tape(br#""\u0041""#);
    } // "A"
    #[test]
    fn nested_object() {
        assert_same_tape(br#"{"a":{"b":1}}"#);
    }
    #[test]
    fn nested_array() {
        assert_same_tape(b"[[1,2],[3,4]]");
    }
    #[test]
    fn mixed() {
        assert_same_tape(
            br#"{"id":1,"name":"Alice","scores":[98.5,72.0],"active":true,"meta":null}"#,
        );
    }
    #[test]
    fn large_array() {
        let mut s = String::from("[");
        for i in 0..1000 {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(r#"{{"id":{i},"v":{}}}"#, i as f64 * 1.5));
        }
        s.push(']');
        assert_same_tape(s.as_bytes());
    }
    #[test]
    fn string_longer_than_32_bytes() {
        assert_same_tape(br#""abcdefghijklmnopqrstuvwxyz0123456789ABCDEF""#);
    }
    #[test]
    fn string_with_escape_past_32_boundary() {
        // 32 clean bytes then an escape
        assert_same_tape(br#""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"end""#);
    }

    // Verify the scalar path is still correct.
    #[test]
    fn scalar_integer_value() {
        let t = parse(b"99").unwrap();
        use tape::{TAG_I64, entry_tag};
        assert_eq!(entry_tag(t.words[0]), TAG_I64);
        assert_eq!(t.words[1] as i64, 99);
    }

    // Verify the SIMD path returns the right i64.
    #[test]
    fn simd_integer_value() {
        let t = parse_simd(b"99").unwrap();
        use tape::{TAG_I64, entry_tag};
        assert_eq!(entry_tag(t.words[0]), TAG_I64);
        assert_eq!(t.words[1] as i64, 99);
    }
}

