//! Minimal, dependency-free base64 encoder/decoder (RFC 4648 standard alphabet).
//!
//! Replaces the `base64` crate for kaptaind's simple use case: decoding base64
//! audio payloads from the Google TTS provider. Supports padding and rejects
//! invalid characters.

use std::error::Error;
use std::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Errors that can occur during base64 decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// An invalid character was encountered.
    InvalidByte(usize, u8),
    /// Padding length was invalid.
    InvalidPadding,
    /// Input length is not a valid base64 length.
    InvalidLength,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::InvalidByte(pos, byte) => {
                write!(f, "invalid base64 byte {} at position {}", byte, pos)
            }
            DecodeError::InvalidPadding => write!(f, "invalid base64 padding"),
            DecodeError::InvalidLength => write!(f, "invalid base64 length"),
        }
    }
}

impl Error for DecodeError {}

fn decode_table() -> [u8; 256] {
    let mut table = [255u8; 256];
    for (i, &b) in ALPHABET.iter().enumerate() {
        table[b as usize] = i as u8;
    }
    table
}

/// Decode a standard base64-encoded string.
pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let table = decode_table();

    // Strip whitespace (common in PEM-style payloads).
    let mut bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();

    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    if !bytes.len().is_multiple_of(4) {
        return Err(DecodeError::InvalidLength);
    }

    let mut padding = 0;
    while bytes.ends_with(b"=") {
        padding += 1;
        bytes.pop();
        if padding > 2 {
            return Err(DecodeError::InvalidPadding);
        }
    }

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for (pos, &b) in bytes.iter().enumerate() {
        let value = table[b as usize];
        if value == 255 {
            return Err(DecodeError::InvalidByte(pos, b));
        }
        buf = (buf << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }

    // After decoding all complete sextets, any leftover bits must be zero for
    // the padding to be valid.
    if bits > 0 && (buf & ((1 << bits) - 1)) != 0 {
        return Err(DecodeError::InvalidPadding);
    }

    // Verify output length matches padding.
    let expected_len =
        (input.bytes().filter(|b| !b.is_ascii_whitespace()).count() / 4) * 3 - padding;
    if out.len() != expected_len {
        return Err(DecodeError::InvalidPadding);
    }

    Ok(out)
}

/// Encode bytes as a standard base64 string.
pub fn encode(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        let n = u32::from_be_bytes([0, chunk[0], chunk[1], chunk[2]]);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(n & 0x3f) as usize] as char);
    }

    let remainder = chunks.remainder();
    match remainder.len() {
        0 => {}
        1 => {
            let n = (remainder[0] as u32) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((remainder[0] as u32) << 16) | ((remainder[1] as u32) << 8);
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn round_trip_random() {
        let data: Vec<u8> = (0..256).map(|i| (i * 7 + 13) as u8).collect();
        let encoded = encode(&data);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn rejects_invalid_chars() {
        assert!(decode("Zm9v!mFy").is_err());
    }

    #[test]
    fn rejects_bad_padding() {
        assert!(decode("Zg===").is_err());
        assert!(decode("Zg").is_err());
    }

    #[test]
    fn ignores_whitespace() {
        assert_eq!(decode("Z g ==").unwrap(), b"f");
    }

    #[test]
    fn decode_throughput_smoke() {
        // ~1 MiB of base64 data, decoded 50 times. This is a smoke test that
        // also prints timing so we can eyeball throughput.
        let data: Vec<u8> = (0..(256 * 1024)).map(|i| (i % 251) as u8).collect();
        let encoded = encode(&data);
        let encoded_len = encoded.len();

        let start = std::time::Instant::now();
        let iterations = 50;
        for _ in 0..iterations {
            let decoded = decode(&encoded).unwrap();
            assert_eq!(decoded.len(), data.len());
        }
        let elapsed = start.elapsed();
        let decoded_mb = (encoded_len as f64 / 4.0 * 3.0) * iterations as f64 / (1024.0 * 1024.0);
        eprintln!(
            "base64 decode throughput: {:.2} MiB/s ({:?} for {} iterations)",
            decoded_mb / elapsed.as_secs_f64(),
            elapsed,
            iterations
        );
    }
}
