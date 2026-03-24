use crate::{
    simd_scan::is_whitespace,
    tape::{
        TAG_ARRAY, TAG_END_ARRAY, TAG_END_OBJECT, TAG_F64, TAG_FALSE, TAG_I64, TAG_NULL,
        TAG_OBJECT, TAG_TRUE, Tape, make_entry,
    },
};

/// All errors the parser can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    UnexpectedByte(u8, usize),
    InvalidEscape(u8, usize),
    InvalidUnicode(usize),
    NumberOverflow(usize),
    TrailingData(usize),
    Utf8Error(usize),
    NestingTooDeep,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEof => write!(f, "unexpected end of input"),
            ParseError::UnexpectedByte(b, pos) => {
                write!(f, "unexpected byte 0x{b:02X} at position {pos}")
            }
            ParseError::InvalidEscape(b, pos) => {
                write!(f, "invalid escape \\{} at position {pos}", *b as char)
            }
            ParseError::InvalidUnicode(pos) => {
                write!(f, "invalid unicode escape at position {pos}")
            }
            ParseError::NumberOverflow(pos) => {
                write!(f, "number out of range at position {pos}")
            }
            ParseError::TrailingData(pos) => {
                write!(f, "trailing data after top-level value at position {pos}")
            }
            ParseError::Utf8Error(pos) => write!(f, "invalid UTF-8 at position {pos}"),
            ParseError::NestingTooDeep => write!(f, "JSON nesting exceeds limit"),
        }
    }
}

/// Maximum nesting depth we will recurse to (prevents a stack overflow).
const MAX_DEPTH: usize = 512;

/// The core parser. Consumes a byte slice and produces a `Tape`.
pub struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Parser {
            input,
            pos: 0,
            depth: 0,
        }
    }

    pub fn parse(mut self) -> Result<Tape, ParseError> {
        // Estimate tape capacity: most JSON tokens are >= 2 bytes.
        let tape = Tape::with_capacity(self.input.len() / 2, self.input.len());
        let mut tape = tape;

        self.skip_ws();
        self.parse_value(&mut tape)?;
        self.skip_ws();

        if self.pos < self.input.len() {
            return Err(ParseError::TrailingData(self.pos));
        }

        Ok(tape)
    }

    // ================
    // Internal helpers
    // ================

    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    #[inline(always)]
    fn advance(&mut self) -> Result<u8, ParseError> {
        let b = self
            .input
            .get(self.pos)
            .copied()
            .ok_or(ParseError::UnexpectedEof)?;
        self.pos += 1;
        Ok(b)
    }

    #[inline(always)]
    fn expect(&mut self, expected: u8) -> Result<(), ParseError> {
        let b = self.advance()?;
        if b != expected {
            Err(ParseError::UnexpectedByte(b, self.pos - 1))
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if is_whitespace(b) {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    // ===================
    // Value dispatch
    // ===================

    fn parse_value(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        if self.depth > MAX_DEPTH {
            return Err(ParseError::NestingTooDeep);
        }

        let b = self.peek().ok_or(ParseError::UnexpectedEof)?;

        match b {
            b'"' => self.parse_string(tape),
            b'{' => self.parse_object(tape),
            b'[' => self.parse_array(tape),
            b't' => self.parse_true(tape),
            b'f' => self.parse_false(tape),
            b'n' => self.parse_null(tape),
            b'-' | b'0'..=b'9' => self.parse_number(tape),
            other => Err(ParseError::UnexpectedByte(other, self.pos)),
        }
    }

    // ========
    // Literals
    // =========

    fn parse_null(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.expect_literal(b"null")?;
        tape.words.push(make_entry(TAG_NULL, 0));
        Ok(())
    }

    fn parse_true(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.expect_literal(b"true")?;
        tape.words.push(make_entry(TAG_TRUE, 0));
        Ok(())
    }

    fn parse_false(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.expect_literal(b"false")?;
        tape.words.push(make_entry(TAG_FALSE, 0));
        Ok(())
    }

    fn expect_literal(&mut self, lit: &[u8]) -> Result<(), ParseError> {
        let end = self.pos + lit.len();
        if end > self.input.len() {
            return Err(ParseError::UnexpectedEof);
        }
        if &self.input[self.pos..end] != lit {
            return Err(ParseError::UnexpectedByte(self.input[self.pos], self.pos));
        }
        self.pos = end;
        Ok(())
    }

    // =========
    // Numbers
    // =========

    fn parse_number(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        let start = self.pos;
        let mut is_float = false;

        // Optional leading minus.
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }

        // Integer part.
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(ParseError::UnexpectedByte(self.input[self.pos], self.pos)),
        }

        // Fractional part.
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ParseError::UnexpectedByte(
                    self.peek().unwrap_or(0),
                    self.pos,
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }

        // Exponent part.
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ParseError::UnexpectedEof);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }

        let raw = &self.input[start..self.pos];

        if is_float {
            // SAFETY: we validated the bytes above; they are valid ASCII.
            let s = unsafe { std::str::from_utf8_unchecked(raw) };
            let v: f64 = s.parse().map_err(|_| ParseError::NumberOverflow(start))?;
            tape.words.push(make_entry(TAG_F64, 0));
            tape.words.push(v.to_bits());
        } else {
            let s = unsafe { std::str::from_utf8_unchecked(raw) };
            let v: i64 = s.parse().map_err(|_| ParseError::NumberOverflow(start))?;
            tape.words.push(make_entry(TAG_I64, 0));
            tape.words.push(v as u64);
        }

        Ok(())
    }

    // ========
    // Strings
    // ========

    fn parse_string(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.expect(b'"')?;
        let entry = self.parse_string_content(tape)?;
        tape.words.push(entry);
        Ok(())
    }

    /// Parse the content of a string (after the opening `"`), write the
    /// decoded bytes into `tape.strings`, and return the tape entry.
    fn parse_string_content(&mut self, tape: &mut Tape) -> Result<u64, ParseError> {
        let pool_offset = tape.strings.len() as u32;

        loop {
            let b = self.advance()?;
            match b {
                b'"' => break,
                b'\\' => {
                    let esc = self.advance()?;
                    match esc {
                        b'"' => tape.strings.push(b'"'),
                        b'\\' => tape.strings.push(b'\\'),
                        b'/' => tape.strings.push(b'/'),
                        b'b' => tape.strings.push(0x08),
                        b'f' => tape.strings.push(0x0C),
                        b'n' => tape.strings.push(b'\n'),
                        b'r' => tape.strings.push(b'\r'),
                        b't' => tape.strings.push(b'\t'),
                        b'u' => self.parse_unicode_escape(tape)?,
                        other => return Err(ParseError::InvalidEscape(other, self.pos - 1)),
                    }
                }
                // Control characters are forbidden in JSON strings.
                0x00..=0x1F => {
                    return Err(ParseError::UnexpectedByte(b, self.pos - 1));
                }
                _ => tape.strings.push(b),
            }
        }

        let len = (tape.strings.len() as u32) - pool_offset;
        Ok(tape.push_string_already_written(pool_offset, len))
    }

    fn parse_unicode_escape(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        let pos = self.pos;
        let code = self.read_hex4()?;

        // Surrogate pair?
        let codepoint = if (0xD800..=0xDBFF).contains(&code) {
            self.expect(b'\\')?;
            self.expect(b'u')?;
            let low = self.read_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(ParseError::InvalidUnicode(pos));
            }
            0x10000 + ((code as u32 - 0xD800) << 10) + (low as u32 - 0xDC00)
        } else {
            code as u32
        };

        // Encode as UTF-8.
        let ch = char::from_u32(codepoint).ok_or(ParseError::InvalidUnicode(pos))?;
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        tape.strings.extend_from_slice(s.as_bytes());
        Ok(())
    }

    fn read_hex4(&mut self) -> Result<u16, ParseError> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let b = self.advance()?;
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(ParseError::InvalidUnicode(self.pos - 1)),
            };
            v = (v << 4) | digit as u16;
        }
        Ok(v)
    }

    // ========
    // Objects
    // ========

    fn parse_object(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.depth += 1;
        self.expect(b'{')?;

        // Reserve slot for the OBJECT entry; we'll fill in the closing-index
        // payload once we know it.
        let obj_idx = tape.words.len();
        tape.words.push(0); // placeholder

        self.skip_ws();

        if self.peek() == Some(b'}') {
            self.pos += 1;
            let end_idx = tape.words.len();
            tape.words[obj_idx] = make_entry(TAG_OBJECT, end_idx as u64);
            tape.words.push(make_entry(TAG_END_OBJECT, obj_idx as u64));
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.skip_ws();
            // Key must be a string.
            if self.peek() != Some(b'"') {
                return Err(ParseError::UnexpectedByte(
                    self.peek().unwrap_or(0),
                    self.pos,
                ));
            }
            self.parse_string(tape)?;

            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();

            self.parse_value(tape)?;

            self.skip_ws();

            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(other) => return Err(ParseError::UnexpectedByte(other, self.pos)),
                None => return Err(ParseError::UnexpectedEof),
            }
        }

        let end_idx = tape.words.len();
        tape.words[obj_idx] = make_entry(TAG_OBJECT, end_idx as u64);
        tape.words.push(make_entry(TAG_END_OBJECT, obj_idx as u64));
        self.depth -= 1;
        Ok(())
    }

    // =========
    // Arrays
    // =========

    fn parse_array(&mut self, tape: &mut Tape) -> Result<(), ParseError> {
        self.depth += 1;
        self.expect(b'[')?;

        let arr_idx = tape.words.len();
        tape.words.push(0); // placeholder

        self.skip_ws();

        if self.peek() == Some(b']') {
            self.pos += 1;
            let end_idx = tape.words.len();
            tape.words[arr_idx] = make_entry(TAG_ARRAY, end_idx as u64);
            tape.words.push(make_entry(TAG_END_ARRAY, arr_idx as u64));
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.skip_ws();
            self.parse_value(tape)?;
            self.skip_ws();

            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                Some(other) => return Err(ParseError::UnexpectedByte(other, self.pos)),
                None => return Err(ParseError::UnexpectedEof),
            }
        }

        let end_idx = tape.words.len();
        tape.words[arr_idx] = make_entry(TAG_ARRAY, end_idx as u64);
        tape.words.push(make_entry(TAG_END_ARRAY, arr_idx as u64));
        self.depth -= 1;
        Ok(())
    }
}

// Helper on `Tape` that is only used by the parser (and only after the string bytes are already in the pool).
impl Tape {
    pub(crate) fn push_string_already_written(&self, offset: u32, len: u32) -> u64 {
        use crate::tape::{TAG_STRING, encode_str_ref};
        make_entry(TAG_STRING, encode_str_ref(offset, len))
    }
}


// Public entry point

/// Parse `input` into a `Tape`. The tape is heap-allocated and must be freed
/// via `tape_free` when no longer needed.
pub fn parse(input: &[u8]) -> Result<Tape, ParseError> {
    Parser::new(input).parse()
}