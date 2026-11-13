//! Trusting a door's certificate by its pin (ADR-0365 D1, #10874): the
//! certificate whose SHA-256 a pairing file or an agent's connection names,
//! and no other. The reference producer and `wild-kit mcp` both take it from
//! here, so the one way the kit trusts a Wild is written once.

use std::sync::{Arc, Mutex};

/// The fingerprint a door showed instead of the pinned one, if any.
pub type Seen = Arc<Mutex<Option<[u8; 32]>>>;

/// `sha256:` and 64 hex digits.
pub fn parse(pin: &str) -> Result<[u8; 32], String> {
    let refused = || format!("the certificate pin is sha256:<64 hex digits>, not `{pin}`");
    let hex = pin.strip_prefix("sha256:").ok_or_else(refused)?;
    // Every byte a hex digit: `from_str_radix` alone would take a `+` sign.
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(refused());
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| refused())?;
    }
    Ok(out)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A TLS client configuration that trusts exactly the certificate whose
/// SHA-256 is `pin`, noting in `seen` the one a door showed instead.
///
/// The type is `rustls`'s: [`super::rustls`] re-exports the crate this one is
/// built against, so a caller names the same version.
pub fn config(pin: [u8; 32], seen: Seen) -> std::io::Result<Arc<rustls::ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(Pinned {
        pin,
        provider: provider.clone(),
        seen,
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// The name TLS sends for the door at `addr` (`host:port`). It is not what is
/// checked — the pin is — but TLS sends one.
pub fn server_name(addr: &str) -> std::io::Result<rustls::pki_types::ServerName<'static>> {
    let host = addr
        .rsplit_once(':')
        .map_or(addr, |(h, _)| h)
        .trim_start_matches('[')
        .trim_end_matches(']');
    rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| std::io::Error::other(format!("the door's host `{host}`: {e}")))
}

/// Trusts the one certificate whose SHA-256 is `pin`; the handshake's
/// signatures are still checked, so only the holder of its key passes.
#[derive(Debug)]
struct Pinned {
    pin: [u8; 32],
    provider: Arc<rustls::crypto::CryptoProvider>,
    seen: Seen,
}

impl rustls::client::danger::ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use sha2::Digest as _;
        let shown: [u8; 32] = sha2::Sha256::digest(end_entity.as_ref()).into();
        if shown == self.pin {
            return Ok(rustls::client::danger::ServerCertVerified::assertion());
        }
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(shown);
        }
        Err(rustls::Error::General(
            "the door's certificate is not the pinned one".into(),
        ))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_64_hex_digits_and_nothing_else() {
        let good = format!("sha256:{}", "ab".repeat(32));
        assert_eq!(parse(&good).unwrap(), [0xab; 32]);
        // `u8::from_str_radix("+a", 16)` is `Ok(10)`: a sign is no hex digit.
        let signed = format!("sha256:+a{}", "ab".repeat(31));
        assert!(parse(&signed).is_err(), "a `+` sign is refused");
        assert!(parse(&format!("sha256:{}", "ab".repeat(31))).is_err());
        assert!(parse(&format!("sha256:{}zz", "ab".repeat(31))).is_err());
        assert!(parse(&"ab".repeat(32)).is_err(), "the prefix is required");
    }

    /// The pin check is the library's one way to trust a server: the pinned
    /// certificate passes, any other is refused and its fingerprint kept for
    /// the operator's message.
    #[test]
    fn only_the_pinned_certificate_is_trusted() {
        use rustls::client::danger::ServerCertVerifier as _;
        use sha2::Digest as _;
        let pinned_der = b"the door's certificate".to_vec();
        let other_der = b"someone else's certificate".to_vec();
        let pin: [u8; 32] = sha2::Sha256::digest(&pinned_der).into();
        let seen: Seen = Arc::default();
        let verifier = Pinned {
            pin,
            provider: Arc::new(rustls::crypto::ring::default_provider()),
            seen: seen.clone(),
        };
        let name = server_name("127.0.0.1:1").unwrap();
        let now = rustls::pki_types::UnixTime::now();
        let check = |der: &Vec<u8>| {
            let cert = rustls::pki_types::CertificateDer::from(der.clone());
            verifier.verify_server_cert(&cert, &[], &name, &[], now)
        };
        assert!(check(&pinned_der).is_ok(), "the pinned certificate passes");
        assert!(seen.lock().unwrap().is_none());
        assert!(check(&other_der).is_err(), "another certificate is refused");
        let other: [u8; 32] = sha2::Sha256::digest(&other_der).into();
        assert_eq!(*seen.lock().unwrap(), Some(other), "what was shown is kept");
    }
}
