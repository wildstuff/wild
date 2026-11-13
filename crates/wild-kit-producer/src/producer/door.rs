//! Where the producer's door is, and how it reaches it: a Wild's door on this
//! machine (a Unix socket), on the local network (`https://`, pinned) or the
//! kit's own (`http://`). A door it cannot reach is said in words — outside
//! developers copy this producer, so its words are the model.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long a connection may take before the door counts as not answering.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the door is, and the certificate it must show when it speaks TLS.
///
/// Built by [`socket`] and [`tcp`] (or [`super::pairing::door`]), and by
/// nothing else: they hold the transport rule — plain `http://` only to this
/// computer, `https://` only with a pin — and the request path trusts a door
/// to keep it, so a door's fields cannot be set from outside this module.
///
/// ```compile_fail,E0616
/// let mut door = wild_kit::producer::door::tcp("http://127.0.0.1:7600", None, |_| Ok([0; 32]))
///     .unwrap();
/// door.addr = "192.168.1.20:7533".into(); // plain HTTP to another computer
/// ```
///
/// ```compile_fail,E0616
/// let mut door = wild_kit::producer::door::tcp(
///     "https://192.168.1.20:7533",
///     Some("sha256:0".into()),
///     |_| Ok([0; 32]),
/// )
/// .unwrap();
/// door.pin = None; // the same door, its pin dropped: plain HTTP
/// ```
#[derive(Clone)]
pub struct Door {
    addr: String,
    pin: Option<[u8; 32]>,
    socket: Option<PathBuf>,
    client_config: Option<Arc<rustls::ClientConfig>>,
}

impl Door {
    /// `host:port` of the door; for a socket door, the name its requests carry
    /// as `host`.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// The SHA-256 of the door's certificate: `Some` for an `https://` door,
    /// which is reached over TLS; `None` for a socket door or the kit's own
    /// plain `http://` door on this computer.
    #[must_use]
    pub fn pin(&self) -> Option<[u8; 32]> {
        self.pin
    }

    /// The Unix socket of a door on this machine: no `://` in `--door`.
    #[must_use]
    pub fn socket(&self) -> Option<&Path> {
        self.socket.as_deref()
    }

    /// The TLS client configuration an `https://` door is reached with in
    /// place of the pin check: set only by [`Door::with_client_config`].
    pub(crate) fn client_config(&self) -> Option<&Arc<rustls::ClientConfig>> {
        self.client_config.as_ref()
    }

    /// Reach this `https://` door with `config` instead of the pin check.
    ///
    /// **This replaces the pin check; it does not add to it.** The pin check
    /// is the one way this crate trusts a door: a door built from a pairing
    /// file trusts exactly the certificate the file names. Once `config` is
    /// set, the pin is no longer compared at all — what `config` trusts is
    /// what the door is trusted for, and a door that shows another
    /// certificate is no longer refused in words. The door stays an
    /// `https://` door: TLS is still spoken, never plain HTTP. A plain
    /// `http://` or socket door is reached as before.
    #[must_use]
    pub fn with_client_config(mut self, config: rustls::ClientConfig) -> Door {
        self.client_config = Some(Arc::new(config));
        self
    }
}

/// A connection to the door, whichever way it goes.
pub enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.read(buf),
        }
    }
}

impl Stream {
    /// Read for at most `limit` before a read counts as unanswered: the
    /// door's own default is short, and a call wait is held longer.
    pub fn read_within(&self, limit: Duration) -> io::Result<()> {
        match self {
            Stream::Tcp(s) => s.set_read_timeout(Some(limit)),
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(Some(limit)),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Stream::Unix(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
        }
    }
}

impl Door {
    /// Open a connection to the door, or say in words why not.
    pub fn connect(&self) -> io::Result<Stream> {
        match &self.socket {
            Some(path) => connect_socket(path),
            None => connect_tcp(&self.addr).map(Stream::Tcp),
        }
    }

    /// The TCP stream of an `http://` or `https://` door.
    pub fn connect_tcp(&self) -> io::Result<TcpStream> {
        connect_tcp(&self.addr)
    }
}

fn connect_tcp(addr: &str) -> io::Result<TcpStream> {
    let stream = addr
        .to_socket_addrs()
        .and_then(|mut found| {
            found
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such address"))
        })
        .and_then(|a| TcpStream::connect_timeout(&a, CONNECT_TIMEOUT))
        .map_err(|e| unreachable_tcp(addr, e))?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    Ok(stream)
}

#[cfg(unix)]
fn connect_socket(path: &Path) -> io::Result<Stream> {
    let stream =
        std::os::unix::net::UnixStream::connect(path).map_err(|e| missing_socket(path, e))?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    Ok(Stream::Unix(stream))
}

#[cfg(not(unix))]
fn connect_socket(path: &Path) -> io::Result<Stream> {
    Err(io::Error::other(no_sockets_here(path)))
}

/// Said when a socket door is named on a platform without Unix sockets.
fn no_sockets_here(path: &Path) -> String {
    format!(
        "--door {} names a Unix socket, which this platform does not have; \
         use the door on the local network: start with --pairing <file>",
        path.display()
    )
}

/// A TCP door that could not be reached, in words: what happened, what to
/// try, and the OS error after them for the engineer (#11721).
fn unreachable_tcp(addr: &str, e: io::Error) -> io::Error {
    let what_to_try = if is_loopback(addr) {
        "nothing is listening there; is the kit or Wild running?"
    } else {
        "a virtual machine usually needs the door on the local network \
         (“Local network”, „Lokales Netz“: `wild ingest listen on --network`)"
    };
    io::Error::new(
        e.kind(),
        format!("could not reach Wild at {addr}: {what_to_try} ({e})"),
    )
}

/// A socket with no Wild behind it, in words.
fn missing_socket(path: &Path, e: io::Error) -> io::Error {
    use io::ErrorKind::{ConnectionRefused, NotFound};
    if matches!(e.kind(), NotFound | ConnectionRefused) {
        return io::Error::new(
            e.kind(),
            format!("no Wild door at {}; is Wild running?", path.display()),
        );
    }
    e
}

/// A socket door is a path: it holds a `/`, or exists as it is named. A bare
/// `host:port` is neither, and is a missing scheme.
fn looks_like_a_path(door: &str) -> bool {
    door.contains('/') || Path::new(door).exists()
}

/// Whether `addr` (`host:port`) names this computer itself.
/// Whether a plain `http://` door at `addr` is on this computer: a loopback
/// host, or `10.0.2.2`, the address under which a QEMU VM with user-mode
/// networking reaches its host's loopback. QEMU answers that address itself,
/// inside the VM; it never leaves for the network, and the kit's TLS-less
/// door is reachable from such a VM at no other address.
fn reaches_this_computer(addr: &str) -> bool {
    is_loopback(addr) || host_of(addr) == QEMU_USER_MODE_HOST
}

/// The host as a QEMU VM with user-mode networking sees it.
const QEMU_USER_MODE_HOST: &str = "10.0.2.2";

fn host_of(addr: &str) -> &str {
    addr.rsplit_once(':')
        .map_or(addr, |(h, _)| h)
        .trim_start_matches('[')
        .trim_end_matches(']')
}

fn is_loopback(addr: &str) -> bool {
    let host = host_of(addr);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The door `--door` names: the path of the Unix socket Wild names for a
/// program on this computer, or a refusal in words. A door over TCP comes in
/// the pairing file Wild writes ([`tcp`]), never on the command line, so its
/// code passes through no prompt (ADR-0365 D1 amendment, step 4).
pub fn socket(door: &str) -> Result<Door, String> {
    if door.contains("://") {
        return Err(format!(
            "--door takes the path of Wild's Unix socket, not `{door}`: a door over the \
             network comes in the pairing file Wild writes for this connector — start it \
             with --pairing <file>"
        ));
    }
    if !looks_like_a_path(door) {
        return Err(format!(
            "--door takes the path of Wild's Unix socket, not `{door}`; on the network, \
             start with --pairing <file>"
        ));
    }
    if cfg!(not(unix)) {
        return Err(no_sockets_here(Path::new(door)));
    }
    Ok(Door {
        addr: "door".into(),
        pin: None,
        socket: Some(PathBuf::from(door)),
        client_config: None,
    })
}

/// The door a pairing file names, with the pin of its certificate: an
/// `https://` door pins one, the kit's own `http://` door has none.
pub fn tcp(
    door: &str,
    certificate: Option<String>,
    parse_pin: impl Fn(&str) -> Result<[u8; 32], String>,
) -> Result<Door, String> {
    let at = |addr: &str, pin| Door {
        addr: addr.trim_end_matches('/').to_string(),
        pin,
        socket: None,
        client_config: None,
    };
    match (door.split_once("://"), certificate) {
        // Plain HTTP carries the pairing code and the key in the clear: only
        // to a door on this computer, which is the kit's own.
        (Some(("http", addr)), None) if reaches_this_computer(addr.trim_end_matches('/')) => {
            Ok(at(addr, None))
        }
        (Some(("http", addr)), None) => Err(format!(
            "the pairing file names a plain http:// door at {}, which is not on this computer. \
             Plain http:// is the wild-kit's own door only: at 127.0.0.1, or at 10.0.2.2 from a \
             QEMU VM with user-mode networking. A Wild on the network is https:// with a \
             certificate to pin: ask its operator for a pairing file for that door",
            addr.trim_end_matches('/')
        )),
        (Some(("http", _)), Some(_)) => Err("the pairing file pins a certificate for a plain \
                                             http:// door, which has none to check: pair the \
                                             connector again"
            .into()),
        (Some(("https", addr)), Some(pin)) => Ok(at(addr, Some(parse_pin(&pin)?))),
        (Some(("https", _)), None) => Err("the pairing file names an https:// door but no \
                                           certificate to pin: pair the connector again"
            .into()),
        _ => Err(format!(
            "the pairing file names no http:// or https:// door, but `{door}`"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(_: &str) -> Result<[u8; 32], String> {
        Ok([0; 32])
    }

    #[cfg(unix)]
    #[test]
    fn a_path_is_a_socket_door() {
        let d = socket("/tmp/wk/system/ingest.sock").unwrap();
        assert_eq!(d.socket(), Some(Path::new("/tmp/wk/system/ingest.sock")));
        assert!(d.pin().is_none());
    }

    #[test]
    fn a_bare_host_and_port_is_no_socket() {
        let e = socket("127.0.0.1:7600").err().unwrap();
        assert!(e.contains("--pairing <file>"), "{e}");
    }

    /// The line's road is gone: an address on the command line is sent to the
    /// pairing file, which carries the code past every prompt.
    #[test]
    fn a_network_door_on_the_command_line_is_sent_to_the_pairing_file() {
        let e = socket("https://192.168.1.20:7533").err().unwrap();
        assert!(e.contains("comes in the pairing file"), "{e}");
    }

    #[test]
    fn a_pairing_files_door_pins_its_certificate_or_is_refused_in_words() {
        let d = tcp("https://192.168.1.20:7533/", Some("sha256:0".into()), pin).unwrap();
        assert_eq!(d.addr(), "192.168.1.20:7533");
        assert!(d.pin().is_some() && d.socket().is_none());
        assert!(tcp("http://127.0.0.1:7600", None, pin)
            .unwrap()
            .pin()
            .is_none());
        let e = tcp("https://192.168.1.20:7533", None, pin).err().unwrap();
        assert!(e.contains("no certificate to pin"), "{e}");
        let e = tcp("http://127.0.0.1:7600", Some("sha256:0".into()), pin)
            .err()
            .unwrap();
        assert!(e.contains("plain http:// door"), "{e}");
        let e = tcp("/tmp/ingest.sock", None, pin).err().unwrap();
        assert!(e.contains("no http:// or https:// door"), "{e}");
    }

    /// Plain HTTP sends the pairing code and the key in the clear: a pairing
    /// file may name it for this computer only — its loopback, or the host as
    /// a QEMU user-mode VM reaches it.
    #[test]
    fn a_plain_http_door_is_taken_on_this_computer_only() {
        for here in [
            "http://127.0.0.1:7600",
            "http://localhost:7600/",
            "http://[::1]:7600",
            "http://10.0.2.2:51698/",
        ] {
            assert!(tcp(here, None, pin).is_ok(), "{here}");
        }
        for away in [
            "http://192.168.1.20:7533",
            "http://example.com:80",
            "http://10.0.2.20:51698",
            "http://10.0.2.3:51698",
            "http://110.0.2.2:51698",
        ] {
            let e = tcp(away, None, pin).err().expect(away);
            assert!(e.contains("not on this computer"), "{away}: {e}");
            assert!(!e.contains("Pair the connector again"), "{away}: {e}");
            assert!(e.contains("ask its operator"), "{away}: {e}");
        }
        let e = tcp("ftp://127.0.0.1:1", None, pin).err().unwrap();
        assert!(e.contains("no http:// or https:// door"), "{e}");
    }

    /// The transport rule, over every public way to a door: what comes out is
    /// a socket door, a pinned door, or a plain one on this computer — never a
    /// plain door to another one. A client configuration keeps the pin's TLS.
    #[test]
    fn no_public_constructor_yields_an_unpinned_door_to_another_computer() {
        let hosts = [
            "127.0.0.1:7600",
            "localhost:7600",
            "[::1]:7600",
            "10.0.2.2:51698",
            "10.0.2.20:51698",
            "192.168.1.20:7533",
            "example.com:80",
        ];
        let mut made = Vec::new();
        for host in hosts {
            for scheme in ["http", "https", "ftp", ""] {
                for certificate in [None, Some("sha256:0".to_string())] {
                    let door = format!("{scheme}://{host}/");
                    made.extend(tcp(&door, certificate, pin));
                }
            }
            made.extend(socket(host));
            made.extend(socket(&format!("http://{host}")));
        }
        made.extend(socket("/tmp/wk/system/ingest.sock"));
        let configured: Vec<Door> = made
            .iter()
            .cloned()
            .map(|d| {
                d.with_client_config(
                    crate::producer::pin::config([0; 32], Default::default())
                        .unwrap()
                        .as_ref()
                        .clone(),
                )
            })
            .collect();
        assert!(made
            .iter()
            .any(|d| d.pin().is_none() && d.socket().is_none()));
        for d in made.iter().chain(&configured) {
            assert!(
                d.socket().is_some() || d.pin().is_some() || reaches_this_computer(d.addr()),
                "a plain door to {}",
                d.addr()
            );
        }
    }

    #[test]
    fn an_unreachable_tcp_door_says_what_to_try_then_the_os_error() {
        for kind in [
            io::ErrorKind::TimedOut,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::HostUnreachable,
            io::ErrorKind::NetworkUnreachable,
        ] {
            let os = io::Error::from(kind);
            let e = unreachable_tcp("10.0.2.2:51698", io::Error::from(kind));
            let said = e.to_string();
            assert!(
                said.starts_with("could not reach Wild at 10.0.2.2:51698: "),
                "{said}"
            );
            assert!(said.contains("„Lokales Netz“"), "{said}");
            assert!(said.ends_with(&format!(" ({os})")), "{said}");
            assert_eq!(e.kind(), kind);
        }
        let here = unreachable_tcp(
            "127.0.0.1:1",
            io::Error::from(io::ErrorKind::ConnectionRefused),
        );
        let said = here.to_string();
        assert!(said.contains("nothing is listening there"), "{said}");
        assert!(!said.contains("virtual machine"), "{said}");
    }

    #[test]
    fn a_missing_socket_says_wild_may_not_be_running() {
        let e = missing_socket(
            Path::new("/tmp/none.sock"),
            io::Error::from(io::ErrorKind::NotFound),
        );
        assert_eq!(
            e.to_string(),
            "no Wild door at /tmp/none.sock; is Wild running?"
        );
    }

    #[cfg(unix)]
    #[test]
    fn connecting_to_a_missing_socket_is_worded() {
        let d = socket("/nonexistent/ingest.sock").unwrap();
        let e = d.connect().err().unwrap();
        assert!(e
            .to_string()
            .contains("no Wild door at /nonexistent/ingest.sock"));
    }
}
