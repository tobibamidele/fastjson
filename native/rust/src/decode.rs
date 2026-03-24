/// Typed access helpers that operate on a borrowed `Tape`.
///
/// These are used by both the FFI layer and any future codegen layer.
/// All access is bounds-checked; no unsafe code here.
use crate::tape::{
    TAG_ARRAY, TAG_END_ARRAY, TAG_END_OBJECT, TAG_F64, TAG_FALSE, TAG_I64, TAG_NULL, TAG_OBJECT,
    TAG_STRING, TAG_TRUE, Tape, entry_payload, entry_tag,
};

#[derive(Debug)]
pub enum DecodeError {
    WrongType {
        expected: &'static str,
        got: &'static str,
    },
    OutOfBounds,
    KeyNotFound,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::WrongType { expected, got } => {
                write!(f, "expected {expected}, got {got}")
            }
            DecodeError::OutOfBounds => write!(f, "index out of bounds"),
            DecodeError::KeyNotFound => write!(f, "key not found"),
        }
    }
}

fn tag_name(tag: u8) -> &'static str {
    match tag {
        TAG_NULL => "null",
        TAG_TRUE | TAG_FALSE => "bool",
        TAG_STRING => "string",
        TAG_I64 => "integer",
        TAG_F64 => "float",
        TAG_OBJECT => "object",
        TAG_ARRAY => "array",
        _ => "unknown",
    }
}

/// A cursor into the tape at a particular word index.
pub struct Value<'t> {
    tape: &'t Tape,
    /// Index of the primary tape word for this value.
    pub(crate) idx: usize,
}

impl<'t> Value<'t> {
    pub fn new(tape: &'t Tape, idx: usize) -> Self {
        Value { tape, idx }
    }

    fn word(&self) -> Result<u64, DecodeError> {
        self.tape
            .words
            .get(self.idx)
            .copied()
            .ok_or(DecodeError::OutOfBounds)
    }

    pub fn tag(&self) -> Result<u8, DecodeError> {
        Ok(entry_tag(self.word()?))
    }

    pub fn is_null(&self) -> bool {
        self.tag().map_or(false, |t| t == TAG_NULL)
    }

    pub fn as_bool(&self) -> Result<bool, DecodeError> {
        match self.tag()? {
            TAG_TRUE => Ok(true),
            TAG_FALSE => Ok(false),
            t => Err(DecodeError::WrongType {
                expected: "bool",
                got: tag_name(t),
            }),
        }
    }

    pub fn as_i64(&self) -> Result<i64, DecodeError> {
        match self.tag()? {
            TAG_I64 => {
                let raw = self
                    .tape
                    .words
                    .get(self.idx + 1)
                    .copied()
                    .ok_or(DecodeError::OutOfBounds)?;
                Ok(raw as i64)
            }
            t => Err(DecodeError::WrongType {
                expected: "integer",
                got: tag_name(t),
            }),
        }
    }

    pub fn as_f64(&self) -> Result<f64, DecodeError> {
        match self.tag()? {
            TAG_F64 => {
                let raw = self
                    .tape
                    .words
                    .get(self.idx + 1)
                    .copied()
                    .ok_or(DecodeError::OutOfBounds)?;
                Ok(f64::from_bits(raw))
            }
            TAG_I64 => {
                let raw = self
                    .tape
                    .words
                    .get(self.idx + 1)
                    .copied()
                    .ok_or(DecodeError::OutOfBounds)?;
                Ok((raw as i64) as f64)
            }
            t => Err(DecodeError::WrongType {
                expected: "number",
                got: tag_name(t),
            }),
        }
    }

    pub fn as_str_bytes(&self) -> Result<&[u8], DecodeError> {
        match self.tag()? {
            TAG_STRING => {
                let payload = entry_payload(self.word()?);
                Ok(self.tape.get_string(payload))
            }
            t => Err(DecodeError::WrongType {
                expected: "string",
                got: tag_name(t),
            }),
        }
    }

    pub fn as_str(&self) -> Result<&str, DecodeError> {
        let bytes = self.as_str_bytes()?;
        // SAFETY: the parser guarantees all string content is valid UTF-8 after
        // decode (escape sequences are decoded to well-formed UTF-8).
        Ok(unsafe { std::str::from_utf8_unchecked(bytes) })
    }

    /// Get a field from an object by key.
    ///
    /// Complexity: O(n) where n is the number of keys.  For hot paths with
    /// repeated access to the same key, callers should cache the returned index.
    pub fn get_field(&self, key: &[u8]) -> Result<Value<'t>, DecodeError> {
        if self.tag()? != TAG_OBJECT {
            return Err(DecodeError::WrongType {
                expected: "object",
                got: tag_name(self.tag()?),
            });
        }
        let end_idx = entry_payload(self.word()?) as usize;

        let mut cursor = self.idx + 1;
        while cursor < end_idx {
            let kw = self.tape.words[cursor];
            let ktag = entry_tag(kw);
            if ktag != TAG_STRING {
                break; // malformed tape
            }
            let kpayload = entry_payload(kw);
            let kbytes = self.tape.get_string(kpayload);
            let val_idx = cursor + 1;
            if kbytes == key {
                return Ok(Value::new(self.tape, val_idx));
            }
            // Skip past the value.
            cursor = skip_value(self.tape, val_idx);
        }
        Err(DecodeError::KeyNotFound)
    }

    /// Iterate over array elements.  Returns `Err` if not an array.
    pub fn iter_array(&self) -> Result<ArrayIter<'t>, DecodeError> {
        if self.tag()? != TAG_ARRAY {
            return Err(DecodeError::WrongType {
                expected: "array",
                got: tag_name(self.tag()?),
            });
        }
        let end_idx = entry_payload(self.word()?) as usize;
        Ok(ArrayIter {
            tape: self.tape,
            cursor: self.idx + 1,
            end_idx,
        })
    }
}

/// Advance past a single value in the tape, returning the index of the next
/// word after the value.
pub fn skip_value(tape: &Tape, idx: usize) -> usize {
    if idx >= tape.words.len() {
        return idx;
    }
    let w = tape.words[idx];
    match entry_tag(w) {
        TAG_I64 | TAG_F64 => idx + 2,
        TAG_OBJECT | TAG_ARRAY => {
            let end = entry_payload(w) as usize;
            end + 1 // skip past EndObject/EndArray
        }
        _ => idx + 1,
    }
}

/// Iterator over the elements of a JSON array.
pub struct ArrayIter<'t> {
    tape: &'t Tape,
    cursor: usize,
    end_idx: usize,
}

impl<'t> Iterator for ArrayIter<'t> {
    type Item = Value<'t>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end_idx {
            return None;
        }
        let tag = entry_tag(self.tape.words[self.cursor]);
        if tag == TAG_END_ARRAY || tag == TAG_END_OBJECT {
            return None;
        }
        let v = Value::new(self.tape, self.cursor);
        self.cursor = skip_value(self.tape, self.cursor);
        Some(v)
    }
}
