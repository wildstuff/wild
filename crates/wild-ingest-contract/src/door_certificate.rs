//! The pin of the door's certificate (ADR-0365 D1, #10874).
//!
//! On the local network the door serves TLS with a certificate the Wild made
//! itself, which no certificate authority vouches for. The pairing file
//! carries its [`fingerprint`] as `door_certificate`, and the producer
//! trusts exactly the certificate whose fingerprint that is.

use sha2::{Digest, Sha256};

/// What every pin starts with: the hash it names.
pub const PREFIX: &str = "sha256:";

/// The pin of a certificate: `sha256:` and the 64 lowercase hex digits of the
/// SHA-256 of its DER bytes — the certificate the door shows first in its
/// TLS handshake, not its key, not its PEM text.
pub fn fingerprint(der: &[u8]) -> String {
    format!("{PREFIX}{}", hex::encode(Sha256::digest(der)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_sha256_and_64_lowercase_hex_digits_over_the_der() {
        let pin = fingerprint(b"not a certificate, but bytes");
        let hex = pin.strip_prefix(PREFIX).unwrap();
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "{pin}"
        );
        assert_eq!(
            fingerprint(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
