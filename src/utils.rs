//! Common utilities

use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    hash::Hash,
};

use crate::patch::ParsePatchError;

/// Returns `true` if a byte must be quoted in a diff filename.
///
/// Matches git's behavior with all control characters
/// (0x00-0x1f), DEL (0x7f), double quote, and backslash.
pub(crate) fn byte_needs_quoting(b: u8) -> bool {
    b < 0x20 || b == 0x7f || b == b'"' || b == b'\\'
}

/// Writes one byte in its escaped form to an [`io::Write`] sink.
///
/// Named escapes are used where git defines them (`\a`, `\b`,
/// `\t`, `\n`, `\v`, `\f`, `\r`, `\\`, `\"`). Other bytes that
/// require quoting are emitted as 3-digit octal (`\NNN`).
/// Non-special bytes are written through unchanged.
pub(crate) fn write_escaped_byte<W: std::io::Write>(w: &mut W, b: u8) -> std::io::Result<()> {
    match b {
        b'\x07' => w.write_all(b"\\a"),
        b'\x08' => w.write_all(b"\\b"),
        b'\t' => w.write_all(b"\\t"),
        b'\n' => w.write_all(b"\\n"),
        b'\x0b' => w.write_all(b"\\v"),
        b'\x0c' => w.write_all(b"\\f"),
        b'\r' => w.write_all(b"\\r"),
        b'"' => w.write_all(b"\\\""),
        b'\\' => w.write_all(b"\\\\"),
        b if b < 0x20 || b == 0x7f => {
            let buf = [
                b'\\',
                b'0' + (b >> 6),
                b'0' + ((b >> 3) & 7),
                b'0' + (b & 7),
            ];
            w.write_all(&buf)
        }
        _ => w.write_all(&[b]),
    }
}

/// Writes one byte in its escaped form to a [`fmt::Write`] sink.
///
/// Same logic as [`write_escaped_byte`] but for [`fmt::Write`].
pub(crate) fn fmt_escaped_byte(f: &mut impl std::fmt::Write, b: u8) -> std::fmt::Result {
    match b {
        b'\x07' => f.write_str("\\a"),
        b'\x08' => f.write_str("\\b"),
        b'\t' => f.write_str("\\t"),
        b'\n' => f.write_str("\\n"),
        b'\x0b' => f.write_str("\\v"),
        b'\x0c' => f.write_str("\\f"),
        b'\r' => f.write_str("\\r"),
        b'"' => f.write_str("\\\""),
        b'\\' => f.write_str("\\\\"),
        b if b < 0x20 || b == 0x7f => write!(f, "\\{:03o}", b),
        _ => f.write_char(b as char),
    }
}

/// Classifies lines, converting lines into unique `u64`s for quicker comparison
pub struct Classifier<'a, T: ?Sized> {
    next_id: u64,
    unique_ids: HashMap<&'a T, u64>,
}

impl<'a, T: ?Sized + Eq + Hash> Classifier<'a, T> {
    fn classify(&mut self, record: &'a T) -> u64 {
        match self.unique_ids.entry(record) {
            Entry::Occupied(o) => *o.get(),
            Entry::Vacant(v) => {
                let id = self.next_id;
                self.next_id += 1;
                *v.insert(id)
            }
        }
    }
}

impl<'a, T: ?Sized + Text> Classifier<'a, T> {
    pub fn classify_lines(&mut self, text: &'a T) -> (Vec<&'a T>, Vec<u64>) {
        LineIter::new(text)
            .map(|line| (line, self.classify(line)))
            .unzip()
    }
}

impl<T: Eq + Hash + ?Sized> Default for Classifier<'_, T> {
    fn default() -> Self {
        Self {
            next_id: 0,
            unique_ids: HashMap::default(),
        }
    }
}

/// Iterator over the lines of a string, including the `\n` character.
pub struct LineIter<'a, T: ?Sized>(&'a T);

impl<'a, T: ?Sized> LineIter<'a, T> {
    pub fn new(text: &'a T) -> Self {
        Self(text)
    }
}

impl<'a, T: Text + ?Sized> Iterator for LineIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }

        let end = if let Some(idx) = self.0.find("\n") {
            idx + 1
        } else {
            self.0.len()
        };

        let (line, remaining) = self.0.split_at(end);
        self.0 = remaining;
        Some(line)
    }
}

/// A helper trait for processing text like `str` and `[u8]`
/// Useful for abstracting over those types for parsing as well as breaking input into lines
pub trait Text: Eq + Hash {
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    fn starts_with(&self, prefix: &str) -> bool;
    #[allow(unused)]
    fn ends_with(&self, suffix: &str) -> bool;
    fn strip_prefix(&self, prefix: &str) -> Option<&Self>;
    fn strip_suffix(&self, suffix: &str) -> Option<&Self>;
    fn split_at_exclusive(&self, needle: &str) -> Option<(&Self, &Self)>;
    fn find(&self, needle: &str) -> Option<usize>;
    fn split_at(&self, mid: usize) -> (&Self, &Self);
    fn as_str(&self) -> Option<&str>;
    fn as_bytes(&self) -> &[u8];
    #[allow(unused)]
    fn lines(&self) -> LineIter<'_, Self>;

    fn parse<T: std::str::FromStr>(&self) -> Option<T> {
        self.as_str().and_then(|s| s.parse().ok())
    }
}

impl Text for str {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.starts_with(prefix)
    }

    fn ends_with(&self, suffix: &str) -> bool {
        self.ends_with(suffix)
    }

    fn strip_prefix(&self, prefix: &str) -> Option<&Self> {
        self.strip_prefix(prefix)
    }

    fn strip_suffix(&self, suffix: &str) -> Option<&Self> {
        self.strip_suffix(suffix)
    }

    fn split_at_exclusive(&self, needle: &str) -> Option<(&Self, &Self)> {
        self.find(needle)
            .map(|idx| (&self[..idx], &self[idx + needle.len()..]))
    }

    fn find(&self, needle: &str) -> Option<usize> {
        self.find(needle)
    }

    fn split_at(&self, mid: usize) -> (&Self, &Self) {
        self.split_at(mid)
    }

    fn as_str(&self) -> Option<&str> {
        Some(self)
    }

    fn as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn lines(&self) -> LineIter<'_, Self> {
        LineIter::new(self)
    }
}

impl Text for [u8] {
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.starts_with(prefix.as_bytes())
    }

    fn ends_with(&self, suffix: &str) -> bool {
        self.ends_with(suffix.as_bytes())
    }

    fn strip_prefix(&self, prefix: &str) -> Option<&Self> {
        self.strip_prefix(prefix.as_bytes())
    }

    fn strip_suffix(&self, suffix: &str) -> Option<&Self> {
        self.strip_suffix(suffix.as_bytes())
    }

    fn split_at_exclusive(&self, needle: &str) -> Option<(&Self, &Self)> {
        find_bytes(self, needle.as_bytes()).map(|idx| (&self[..idx], &self[idx + needle.len()..]))
    }

    fn find(&self, needle: &str) -> Option<usize> {
        find_bytes(self, needle.as_bytes())
    }

    fn split_at(&self, mid: usize) -> (&Self, &Self) {
        self.split_at(mid)
    }

    fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self).ok()
    }

    fn as_bytes(&self) -> &[u8] {
        self
    }

    fn lines(&self) -> LineIter<'_, Self> {
        LineIter::new(self)
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    match needle.len() {
        0 => Some(0),
        1 => find_byte(haystack, needle[0]),
        len if len > haystack.len() => None,
        needle_len => {
            let mut offset = 0;
            let mut haystack = haystack;

            while let Some(position) = find_byte(haystack, needle[0]) {
                offset += position;

                if let Some(haystack) = haystack.get(position..position + needle_len) {
                    if haystack == needle {
                        return Some(offset);
                    }
                } else {
                    return None;
                }

                haystack = &haystack[position + 1..];
                offset += 1;
            }

            None
        }
    }
}

// XXX Maybe use `memchr`?
fn find_byte(haystack: &[u8], byte: u8) -> Option<usize> {
    haystack.iter().position(|&b| b == byte)
}

/// Decodes escape sequences in a quoted filename.
///
/// See [`byte_needs_quoting`] for the set of characters that
/// require quoting.
pub(crate) fn escaped_filename<T: Text + ToOwned + ?Sized>(
    filename: &T,
) -> Result<Cow<'_, [u8]>, ParsePatchError> {
    if let Some(inner) = filename
        .strip_prefix("\"")
        .and_then(|s| s.strip_suffix("\""))
    {
        decode_escaped(inner)
    } else {
        let bytes = filename.as_bytes();
        if bytes.iter().any(|b| byte_needs_quoting(*b)) {
            return Err(ParsePatchError::new("invalid char in unquoted filename"));
        }
        Ok(bytes.into())
    }
}

fn decode_escaped<T: Text + ToOwned + ?Sized>(
    escaped: &T,
) -> Result<Cow<'_, [u8]>, ParsePatchError> {
    let bytes = escaped.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    let mut last_copy = 0;
    let mut needs_allocation = false;

    while i < bytes.len() {
        if bytes[i] == b'\\' {
            needs_allocation = true;
            result.extend_from_slice(&bytes[last_copy..i]);

            i += 1;
            if i >= bytes.len() {
                return Err(ParsePatchError::new("expected escaped character"));
            }

            let decoded = match bytes[i] {
                b'a' => b'\x07',
                b'b' => b'\x08',
                b'n' => b'\n',
                b't' => b'\t',
                b'v' => b'\x0b',
                b'f' => b'\x0c',
                b'r' => b'\r',
                b'\"' => b'\"',
                b'\\' => b'\\',
                // 3-digit octal: \0xx through \3xx (values 0x00–0xFF)
                c @ b'0'..=b'3' => {
                    if i + 2 >= bytes.len() {
                        return Err(ParsePatchError::new("invalid escaped character"));
                    }
                    let d1 = bytes[i + 1];
                    let d2 = bytes[i + 2];
                    if !(b'0'..=b'7').contains(&d1) || !(b'0'..=b'7').contains(&d2) {
                        return Err(ParsePatchError::new("invalid escaped character"));
                    }
                    i += 2;
                    (c - b'0') << 6 | (d1 - b'0') << 3 | (d2 - b'0')
                }
                _ => return Err(ParsePatchError::new("invalid escaped character")),
            };
            result.push(decoded);
            i += 1;
            last_copy = i;
        } else if byte_needs_quoting(bytes[i]) {
            return Err(ParsePatchError::new("invalid unescaped character"));
        } else {
            i += 1;
        }
    }

    if needs_allocation {
        result.extend_from_slice(&bytes[last_copy..]);
        Ok(Cow::Owned(result))
    } else {
        Ok(Cow::Borrowed(bytes))
    }
}
