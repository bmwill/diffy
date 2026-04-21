//! Git delta binary diff support.
//!
//! A delta payload contains:
//!
//! 1. Header: variable-length encoded sizes (original_size, modified_size)
//! 2. Instructions: sequence of `ADD` and `COPY` operations
//!
//! Based on Diffx's [Git Delta Binary Diffs](https://diffx.org/spec/binary-diffs.html#git-delta-binary-diffs)

use alloc::vec::Vec;
use core::fmt;

/// Applies delta instructions to an original file, producing the modified file.
pub fn apply(original: &[u8], delta: &[u8]) -> Result<Vec<u8>, DeltaError> {
    let mut cursor = DeltaCursor::new(delta);

    let header_orig_size = cursor.read_size()?;
    let header_mod_size = cursor.read_size()?;

    // Validate original size
    if original.len() as u64 != header_orig_size {
        return Err(DeltaError::OriginalSizeMismatch {
            expected: header_orig_size,
            actual: original.len() as u64,
        });
    }

    let mut result = Vec::with_capacity(header_mod_size.min(super::MAX_PREALLOC) as usize);

    // Process instructions until we've consumed all delta data
    while !cursor.is_empty() {
        let control = cursor.read_byte()?;

        if control & 0x80 != 0 {
            // COPY instruction
            let (src_offset, copy_len) = cursor.read_copy_params(control)?;
            let src_end = src_offset
                .checked_add(copy_len)
                .ok_or(DeltaError::InvalidCopyRange)?;

            if src_end > original.len() {
                return Err(DeltaError::CopyOutOfBounds {
                    offset: src_offset,
                    length: copy_len,
                    original_size: original.len(),
                });
            }

            result.extend_from_slice(&original[src_offset..src_end]);
        } else if control == 0 {
            // `git apply` rejects this as "unexpected delta opcode 0".
            return Err(DeltaError::UnexpectedOpcode);
        } else {
            // ADD instruction
            let add_len = control as usize;
            let data = cursor.read_bytes(add_len)?;
            result.extend_from_slice(data);
        }
    }

    // Validate result size
    if result.len() as u64 != header_mod_size {
        return Err(DeltaError::ModifiedSizeMismatch {
            expected: header_mod_size,
            actual: result.len() as u64,
        });
    }

    Ok(result)
}

/// Cursor for reading delta instructions.
struct DeltaCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> DeltaCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset >= self.data.len()
    }

    fn read_byte(&mut self) -> Result<u8, DeltaError> {
        if self.offset >= self.data.len() {
            return Err(DeltaError::UnexpectedEof);
        }
        let byte = self.data[self.offset];
        self.offset += 1;
        Ok(byte)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], DeltaError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(DeltaError::UnexpectedEof)?;
        if end > self.data.len() {
            return Err(DeltaError::UnexpectedEof);
        }
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    /// Reads a variable-length encoded size from the header.
    ///
    /// Format: each byte uses 7 bits for value, MSB indicates continuation.
    /// Bytes are in little-endian order (LSB first).
    fn read_size(&mut self) -> Result<u64, DeltaError> {
        let mut file_len: u64 = 0;
        let mut shift: u32 = 0;

        loop {
            let byte = self.read_byte()?;

            // Add 7 bits of value at current shift position
            let value = (byte & 0x7F) as u64;
            file_len |= value.checked_shl(shift).ok_or(DeltaError::SizeOverflow)?;

            // MSB clear means this is the last byte
            if byte & 0x80 == 0 {
                break;
            }

            shift += 7;
        }

        Ok(file_len)
    }

    /// Reads `COPY` instruction parameters from the control byte.
    /// Returns `(src_offset, copy_len)`.
    ///
    /// Control byte format is `1oooosss`:
    ///
    /// * Bits 0-3: src_offset bytes
    /// * Bits 4-6: copy_len bytes
    fn read_copy_params(&mut self, control: u8) -> Result<(usize, usize), DeltaError> {
        let mut src_offset: u32 = 0;
        for (mask, shift) in [(0x01, 0), (0x02, 8), (0x04, 16), (0x08, 24)] {
            if control & mask != 0 {
                let byte = self.read_byte()? as u32;
                src_offset |= byte.checked_shl(shift).ok_or(DeltaError::SizeOverflow)?;
            }
        }

        let mut copy_len: u32 = 0;
        for (mask, shift) in [(0x10, 0), (0x20, 8), (0x40, 16)] {
            if control & mask != 0 {
                let byte = self.read_byte()? as u32;
                copy_len |= byte.checked_shl(shift).ok_or(DeltaError::SizeOverflow)?;
            }
        }

        if copy_len == 0 {
            // Size of 0 means 65536
            copy_len = 0x10000;
        }

        Ok((src_offset as usize, copy_len as usize))
    }
}

/// Error type for delta operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// Unexpected end of delta data.
    UnexpectedEof,
    /// Size value overflowed during decoding.
    SizeOverflow,
    /// Original file size doesn't match header.
    OriginalSizeMismatch { expected: u64, actual: u64 },
    /// Modified file size doesn't match header.
    ModifiedSizeMismatch { expected: u64, actual: u64 },
    /// COPY instruction references out-of-bounds data.
    CopyOutOfBounds {
        offset: usize,
        length: usize,
        original_size: usize,
    },
    /// COPY range calculation overflowed.
    InvalidCopyRange,
    /// Unexpected delta opcode (control byte 0x00).
    UnexpectedOpcode,
}

impl fmt::Display for DeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeltaError::UnexpectedEof => write!(f, "unexpected end of delta data"),
            DeltaError::SizeOverflow => write!(f, "size value overflow"),
            DeltaError::OriginalSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "original size mismatch: expected {expected}, got {actual}"
                )
            }
            DeltaError::ModifiedSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "modified size mismatch: expected {expected}, got {actual}"
                )
            }
            DeltaError::CopyOutOfBounds {
                offset,
                length,
                original_size,
            } => {
                write!(
                    f,
                    "copy out of bounds: offset={offset}, length={length}, original_size={original_size}"
                )
            }
            DeltaError::InvalidCopyRange => write!(f, "copy range calculation overflow"),
            DeltaError::UnexpectedOpcode => write!(f, "unexpected delta opcode 0"),
        }
    }
}

impl core::error::Error for DeltaError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn read_size_single_byte() {
        // 0x0A = 10, MSB clear = end
        let data = [0x0A];
        let mut cursor = DeltaCursor::new(&data);
        assert_eq!(cursor.read_size().unwrap(), 10);
    }

    #[test]
    fn read_size_multi_byte() {
        // 0x80 | 0x01 = 1, continue; 0x02 = 2 << 7 = 256; total = 257
        let data = [0x81, 0x02];
        let mut cursor = DeltaCursor::new(&data);
        assert_eq!(cursor.read_size().unwrap(), 1 + (2 << 7));
    }

    #[test]
    fn apply_add_only() {
        // Header: orig_size=0, mod_size=5
        // ADD 5 bytes: "hello"
        let delta = [
            0x00, // orig_size = 0
            0x05, // mod_size = 5
            0x05, // ADD 5 bytes
            b'h', b'e', b'l', b'l', b'o',
        ];
        let result = apply(&[], &delta).unwrap();
        assert_eq!(result, b"hello");
    }

    #[test]
    fn apply_copy_only() {
        // Header: orig_size=5, mod_size=5
        // COPY offset=0, len=5
        let delta = [
            0x05, // orig_size = 5
            0x05, // mod_size = 5
            0x90, // COPY: control=0x90 (0x80 | 0x10), offset=0, size byte present
            0x05, // size = 5
        ];
        let original = b"hello";
        let result = apply(original, &delta).unwrap();
        assert_eq!(result, b"hello");
    }

    #[test]
    fn apply_copy_with_offset() {
        // Header: orig_size=10, mod_size=5
        // COPY offset=5, len=5
        let delta = [
            0x0A, // orig_size = 10
            0x05, // mod_size = 5
            0x91, // COPY: 0x80 | 0x10 | 0x01 (offset1 + size1 present)
            0x05, // offset = 5
            0x05, // size = 5
        ];
        let original = b"helloworld";
        let result = apply(original, &delta).unwrap();
        assert_eq!(result, b"world");
    }

    #[test]
    fn apply_mixed_instructions() {
        // Create "HELLO world" from "hello world"
        // Header: orig_size=11, mod_size=11
        // ADD 5: "HELLO"
        // COPY offset=5, len=6: " world"
        let delta = [
            0x0B, // orig_size = 11
            0x0B, // mod_size = 11
            0x05, // ADD 5 bytes
            b'H', b'E', b'L', b'L', b'O', // "HELLO"
            0x91, // COPY: offset1 + size1 present
            0x05, // offset = 5
            0x06, // size = 6
        ];
        let original = b"hello world";
        let result = apply(original, &delta).unwrap();
        assert_eq!(result, b"HELLO world");
    }

    #[test]
    fn apply_copy_size_zero_means_65536() {
        // When size bytes result in 0, it means 65536
        // Header: orig_size=65536, mod_size=65536
        // COPY offset=0, len=65536 (encoded as 0)
        let original = vec![0xAB; 65536];
        let delta = [
            0x80, 0x80, 0x04, // orig_size = 65536 (varint)
            0x80, 0x80, 0x04, // mod_size = 65536 (varint)
            0x80, // COPY: no offset bytes, no size bytes = offset 0, size 65536
        ];
        let result = apply(&original, &delta).unwrap();
        assert_eq!(result.len(), 65536);
        assert_eq!(result, original);
    }

    #[test]
    fn error_original_size_mismatch() {
        let delta = [
            0x0A, // orig_size = 10
            0x05, // mod_size = 5
        ];
        let original = b"short"; // only 5 bytes
        let err = apply(original, &delta).unwrap_err();
        assert!(matches!(
            err,
            DeltaError::OriginalSizeMismatch {
                expected: 10,
                actual: 5
            }
        ));
    }

    #[test]
    fn error_copy_out_of_bounds() {
        let delta = [
            0x05, // orig_size = 5
            0x05, // mod_size = 5
            0x91, // COPY
            0x0A, // offset = 10 (out of bounds!)
            0x05, // size = 5
        ];
        let original = b"hello";
        let err = apply(original, &delta).unwrap_err();
        assert!(matches!(err, DeltaError::CopyOutOfBounds { .. }));
    }

    #[test]
    fn error_unexpected_eof_in_add() {
        let delta = [
            0x00, // orig_size = 0
            0x05, // mod_size = 5
            0x05, // ADD 5 bytes
            b'h', b'i', // only 2 bytes provided
        ];
        let err = apply(&[], &delta).unwrap_err();
        assert_eq!(err, DeltaError::UnexpectedEof);
    }

    #[test]
    fn error_unexpected_opcode() {
        // Same delta layout as compat fixture `binary_delta_zero_control`:
        // hello -> hellX with zero control byte (0x00) between COPY and ADD.
        let delta = [
            0x05, // orig_size = 5
            0x05, // mod_size = 5
            0x91, // COPY: 0x80 | 0x10 | 0x01 (offset1 + size1 present)
            0x00, // offset=0
            0x04, // len=4 ("hell")
            0x00, // zero control byte (unexpected opcode)
            0x01, b'X', // ADD 1 byte: 'X'
        ];
        let original = b"hello";
        let err = apply(original, &delta).unwrap_err();
        assert_eq!(err, DeltaError::UnexpectedOpcode);
    }
}
