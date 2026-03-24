mod decode;
mod parser;
mod simd_scan;
mod tape;

use decode::{Value, skip_value};
use parser::parse;
use tape::Tape;

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn fj_free(handle: *mut Tape) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

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
    if let Some(&w) = (&(*handle).words).get(idx) {
        *out = tape::entry_tag(w);
        FjError::Ok
    } else {
        FjError::OutOfBounds
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

impl<'t> Value<'t> {
    pub(crate) fn tape_idx(&self) -> usize {
        self.idx
    }
}
