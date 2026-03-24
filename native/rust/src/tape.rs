/// Tape-based document representation.
///
/// Each entry is a 64-bit word. The upper 8 bits encode the type and the
/// lower 56 bits encode a payload whose meaning depends on the tag.
///
/// Tag layout:
/// 0x01 Null
/// Ox02 True
/// 0x03 False
/// 0x04 String     - payload = (offset: u32, len: u24) into the string pool
/// 0x05 I64        - payload is meaningless; actual value stored in next word
/// 0x06 F64        - payload is meaningless; actual value stored in next word
/// 0x07 Object     - payload = index of matching EndObject entry
/// 0x08 EndObject
/// 0x09 Array      - payload = index of matching EndArray entry
/// 0x0A EndArray
///
/// String data is stored seperately in a byte pool to avoid repeated heap
/// allocations. String that are keys or values are stored once and referenced
/// by (offest, length) pairs packed into the 56-bit payload.

pub const TAG_NULL: u8 = 0x01;
pub const TAG_TRUE: u8 = 0x02;
pub const TAG_FALSE: u8 = 0x03;
pub const TAG_STRING: u8 = 0x04;
pub const TAG_I64: u8 = 0x05;
pub const TAG_F64: u8 = 0x06;
pub const TAG_OBJECT: u8 = 0x07;
pub const TAG_END_OBJECT: u8 = 0x08;
pub const TAG_ARRAY: u8 = 0x09;
pub const TAG_END_ARRAY: u8 = 0x0A;

/// Maximum number of bytes representable in the 24-bit length field.
pub const MAX_INLINE_STR_LEN: usize = (1 << 24) - 1;

#[inline(always)]
pub fn make_entry(tag: u8, payload: u64) -> u64 {
    ((tag as u64) << 56) | (payload & 0x00FF_FFFF_FFFF_FFFF)
}

#[inline(always)]
pub fn entry_tag(entry: u64) -> u8 {
    (entry >> 56) as u8
}

#[inline(always)]
pub fn entry_payload(payload: u64) -> u64 {
    payload & 0x00FF_FFFF_FFFF_FFFF
}

/// Encode a string reference: offset(u32) and length (u24) packed into 56 bits.
/// offset occupies bits 55-24, len occupies bits 23-0
#[inline(always)]
pub fn encode_str_ref(offset: u32, len: u32) -> u64 {
    debug_assert!(len <= MAX_INLINE_STR_LEN as u32, "string too long for tape");
    ((offset as u64) << 24) | (len as u64 & 0xFF_FF)
}

#[inline(always)]
pub fn decode_str_ref(payload: u64) -> (u32, u32) {
    let offset = (payload >> 24) as u32;
    let len = (payload & 0xFF_FFFF) as u32;
    (offset, len)
}

/// The complete in-memory representation of a parsed document.
///
/// `tape`          - the sequence of encoded words described above.
/// `strings`       - raw UTF-8 bytes; string entries reference slices here.
/// `i64s`          - integer values, indexed by position in the tape.
/// `f64s`          - float values, indexed by position in the tape.
///
/// Integers and floats each occupy **two** consecutive tape words: the first
/// carries the tag (with payload = 0), the second carries the raw bit pattern.
pub struct Tape {
    pub words: Vec<u64>,
    pub strings: Vec<u8>,
}

impl Tape {
    pub fn with_capacity(words: usize, string_bytes: usize) -> Self {
        Tape {
            words: Vec::with_capacity(words),
            strings: Vec::with_capacity(string_bytes),
        }
    }

    /// Push a string into the pool and return a tape entry for it.
    pub fn push_string(&mut self, s: &[u8]) -> u64 {
        let offset = self.strings.len() as u32;
        let len = s.len() as u32;
        self.strings.extend_from_slice(s);
        make_entry(TAG_STRING, encode_str_ref(offset, len))
    }

    /// Retrieve a string from the pool given a tape entry payload.
    pub fn get_string(&self, payload: u64) -> &[u8] {
        let (offset, len) = decode_str_ref(payload);
        &self.strings[offset as usize..(offset + len) as usize]
    }
}
