const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

pub fn encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(LOWER_HEX[(byte >> 4) as usize] as char);
        out.push(LOWER_HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_lowercase_hex() {
        assert_eq!(encode([0x00, 0x0f, 0x10, 0xab, 0xff]), "000f10abff");
    }
}
