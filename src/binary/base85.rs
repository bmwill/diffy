//! Base85 encoding and decoding using the character set defined in [RFC 1924].
//!
//! ## References
//!
//! * [RFC 1924]
//! * [Wikipedia: Ascii85 § RFC 1924 version](https://en.wikipedia.org/wiki/Ascii85#RFC_1924_version)
//!
//! [RFC 1924]: https://datatracker.ietf.org/doc/html/rfc1924

use core::fmt;

/// Base85 character set (RFC 1924).
const ALPHABET: &[u8; 85] = b"0123456789\
    ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz\
    !#$%&()*+-;<=>?@^_`{|}~";

/// Pre-computed lookup table for Base85 decoding.
///
/// Maps ASCII byte value → digit value or `0xFF` for invalid characters.
/// This provides O(1) lookup.
const TABLE: [u8; 256] = {
    let mut table = [0xFFu8; 256];
    let mut i = 0usize;
    while i < 85 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// Error type for Base85 operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base85Error {
    /// Invalid character that is not in RFC 1924 alphabet.
    InvalidCharacter(char),
    /// Invalid input length for the operation.
    InvalidLength,
}

impl fmt::Display for Base85Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base85Error::InvalidCharacter(c) => write!(f, "invalid base85 character: {:?}", c),
            Base85Error::InvalidLength => write!(f, "invalid input length"),
        }
    }
}

impl core::error::Error for Base85Error {}

/// Decodes a Base85 string to the provided output.
///
/// ## Limitations
///
/// The input length must be a multiple of 5.
///
/// This function does not handle padding for partial chunks.
/// When decoding data where the original byte count isn't a multiple of 4,
/// callers must handle truncation at a higher level.
/// For example, via a length indicator in Git binary patch.
pub fn decode_into(input: &[u8], output: &mut impl Extend<u8>) -> Result<(), Base85Error> {
    if input.len() % 5 != 0 {
        return Err(Base85Error::InvalidLength);
    }

    // TODO: Use `as_chunks::<5>()` when MSRV >= 1.88
    for chunk in input.chunks_exact(5) {
        let mut value: u32 = 0;
        for &byte in chunk {
            let digit = TABLE[byte as usize];
            if digit == 0xFF {
                return Err(Base85Error::InvalidCharacter(byte as char));
            }
            value = value * 85 + digit as u32;
        }

        output.extend(value.to_be_bytes());
    }

    Ok(())
}

/// Encodes bytes in Base85 to the provided output.
///
/// ## Limitations
///
/// The input length must be a multiple of 4.
///
/// This function does not handle padding for partial chunks.
/// Callers encoding data where the byte count isn't a multiple of 4
/// must handle padding at a higher level.
/// For example, via a length indicator in Git binary patch format.
#[allow(dead_code)] // will be used for patch formatting
pub fn encode_into(input: &[u8], output: &mut impl Extend<char>) -> Result<(), Base85Error> {
    if input.len() % 4 != 0 {
        return Err(Base85Error::InvalidLength);
    }

    // TODO: Use `as_chunks::<4>()` when MSRV >= 1.88
    for chunk in input.chunks_exact(4) {
        let mut value = u32::from_be_bytes(chunk.try_into().unwrap());

        // Extract 5 base85 digits (least to most significant order)
        let mut digits = [0u8; 5];
        for digit in digits.iter_mut().rev() {
            *digit = ALPHABET[(value % 85) as usize];
            value /= 85;
        }
        output.extend(digits.iter().map(|&b| b as char));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    fn decode(input: &str) -> Result<Vec<u8>, Base85Error> {
        let mut result = Vec::with_capacity((input.len() / 5) * 4);
        decode_into(input.as_bytes(), &mut result)?;
        Ok(result)
    }

    fn encode(input: &[u8]) -> Result<String, Base85Error> {
        let mut result = String::with_capacity((input.len() / 4) * 5);
        encode_into(input, &mut result)?;
        Ok(result)
    }

    const TEST_VECTORS: &[(&[u8], &str)] = &[
        (b"", ""),
        (&[0x00, 0x00, 0x00, 0x00], "00000"),
        (&[0xff, 0xff, 0xff, 0xff], "|NsC0"),
        // Rust ecosystem phrases
        (b"Rust", "Qgw55"),
        (b"Fearless concurrency", "MrC1gY-MwEAY*TCV|8+JWo~16"),
        (b"memory safe!", "ZDnn5a(N(gVP<6^"),
        (b"blazing fast", "Vr*f0X>MmAW?^%5"),
        (
            b"zero-cost abstraction!??",
            "dS!BNEn{zUbRc13b98cHV{~b6ZXrKE",
        ),
    ];

    #[test]
    fn table_covers_all_alphabet_chars() {
        for (i, &c) in ALPHABET.iter().enumerate() {
            assert_eq!(
                TABLE[c as usize], i as u8,
                "mismatch for char '{}' at index {}",
                c as char, i
            );
        }
    }

    #[test]
    fn table_rejects_invalid_chars() {
        let invalid_chars = b" \t\n\r\"'\\[],:";
        for &c in invalid_chars {
            assert_eq!(
                TABLE[c as usize], 0xFF,
                "char '{}' should be invalid",
                c as char
            );
        }
    }

    #[test]
    fn decode_test_vectors() {
        for (bytes, encoded) in TEST_VECTORS {
            let result = decode(encoded).unwrap();
            assert_eq!(&result, *bytes, "decode({:?}) failed", encoded);
        }
    }

    #[test]
    fn encode_test_vectors() {
        for (bytes, encoded) in TEST_VECTORS {
            let result = encode(bytes).unwrap();
            assert_eq!(result, *encoded, "encode({:?}) failed", bytes);
        }
    }

    #[test]
    fn decode_invalid_length() {
        assert!(matches!(decode("0000"), Err(Base85Error::InvalidLength)));
        assert!(matches!(decode("000"), Err(Base85Error::InvalidLength)));
        assert!(matches!(decode("00"), Err(Base85Error::InvalidLength)));
        assert!(matches!(decode("0"), Err(Base85Error::InvalidLength)));
    }

    #[test]
    fn decode_invalid_character() {
        assert!(matches!(
            decode("0000 "),
            Err(Base85Error::InvalidCharacter(' '))
        ));
        assert!(matches!(
            decode("0000\""),
            Err(Base85Error::InvalidCharacter('"'))
        ));
    }

    #[test]
    fn encode_invalid_length() {
        assert!(matches!(encode(&[0]), Err(Base85Error::InvalidLength)));
        assert!(matches!(encode(&[0, 0]), Err(Base85Error::InvalidLength)));
        assert!(matches!(
            encode(&[0, 0, 0]),
            Err(Base85Error::InvalidLength)
        ));
        assert!(matches!(
            encode(&[0, 0, 0, 0, 0]),
            Err(Base85Error::InvalidLength)
        ));
    }

    #[test]
    fn round_trip() {
        for (bytes, _) in TEST_VECTORS {
            let encoded = encode(bytes).unwrap();
            let decoded = decode(&encoded).unwrap();
            assert_eq!(&decoded, *bytes, "round-trip failed for {:?}", bytes);
        }
    }
}
