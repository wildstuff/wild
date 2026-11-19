//! One request to the door, as a producer in any language could make it:
//! HTTP/1.1 by hand over the door's stream — a Unix socket, plain TCP, or
//! TLS with the pinned certificate — and the reply's status, `Retry-After`,
//! headers and JSON body. A door that shows another certificate than the
//! pinned one is refused in words before anything is sent ([`check_door`]).

use std::io::{self, Read, Write};
use std::time::Duration;

use serde_json::Value;
use wild_ingest_contract::codes::RefusalCode;

use super::door::{self, Door};
use super::pin::{self, Seen};

/// One reply: the status, a `Retry-After`, every header, and the body — as
/// JSON when it is JSON, and as text either way.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub status: u16,
    pub retry_after: Option<Duration>,
    /// Every header, each name lowercased.
    pub headers: Vec<(String, String)>,
    /// The body as JSON; `null` when the reply carries none or it is not JSON.
    pub body: Value,
    /// The body as text, for the replies that say it in words rather than JSON.
    pub text: String,
}

impl Reply {
    /// The header `name` carries, whatever case it came in.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One HTTP/1.1 request: the status, a `Retry-After`, and the JSON body
/// (`null` when there is none). A connection closed without a reply is an
/// error.
pub fn request(
    door: &Door,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> io::Result<(u16, Option<Duration>, Value)> {
    reply_within(door, method, path, headers, body, None)
        .map(|reply| (reply.status, reply.retry_after, reply.body))
}

/// [`request`], with the reply waited for up to `read` instead of the
/// door's default: a call wait is held longer than any other reply takes.
pub fn request_within(
    door: &Door,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    read: Option<Duration>,
) -> io::Result<(u16, Option<Duration>, Value)> {
    reply_within(door, method, path, headers, body, read)
        .map(|reply| (reply.status, reply.retry_after, reply.body))
}

/// [`request_within`], keeping the reply's headers: an MCP session lives in
/// one (`mcp-session-id`), and nothing else the kit asks for does.
pub fn reply_within(
    door: &Door,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    read: Option<Duration>,
) -> io::Result<Reply> {
    let stream = door.connect()?;
    if let Some(limit) = read {
        stream.read_within(limit)?;
    }
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nhost: {}\r\nconnection: close\r\ncontent-length: {}\r\n",
        door.addr(),
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let raw = match door.pin() {
        None => exchange(stream, head.as_bytes(), body)?,
        Some(pin) => {
            let door::Stream::Tcp(stream) = stream else {
                return Err(io::Error::other("a pinned door is a TCP door"));
            };
            let (conn, seen) = pinned(door, pin)?;
            exchange(
                rustls::StreamOwned::new(conn, stream),
                head.as_bytes(),
                body,
            )
            .map_err(|e| refusal(door, pin, &seen).unwrap_or(e))?
        }
    };
    let text = String::from_utf8_lossy(&raw);
    let (head, rest) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| io::Error::other("the connection closed without a reply"))?;
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::other("no status line"))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Ok(Reply {
        status,
        retry_after: retry_after(&headers),
        headers,
        body: serde_json::from_str(rest).unwrap_or(Value::Null),
        text: rest.to_string(),
    })
}

/// The longest a door's `Retry-After` parks the producer: the door itself
/// asks for at most a minute, so a longer wait is a door gone wrong.
const RETRY_AFTER_MAX: Duration = Duration::from_secs(300);

/// The largest reply read from a door: a reply is one JSON answer, so one
/// past this is a door gone wrong, not a reply to hold in memory.
const REPLY_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// The reply's `Retry-After` in seconds, at most [`RETRY_AFTER_MAX`].
fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
    headers
        .iter()
        .find(|(k, _)| k == "retry-after")
        .and_then(|(_, v)| v.parse().ok())
        .map(|secs| Duration::from_secs(secs).min(RETRY_AFTER_MAX))
}

/// Write one request and read the whole reply, on a plain or a TLS stream —
/// at most [`REPLY_MAX_BYTES`] of it.
fn exchange<S: Read + Write>(mut stream: S, head: &[u8], body: &[u8]) -> io::Result<Vec<u8>> {
    stream.write_all(head)?;
    stream.write_all(body)?;
    let mut raw = Vec::new();
    let read = (&mut stream)
        .take(REPLY_MAX_BYTES + 1)
        .read_to_end(&mut raw);
    if raw.len() as u64 > REPLY_MAX_BYTES {
        return Err(io::Error::other(format!(
            "the door's reply is larger than {} MiB, which no answer of the contract is: \
             it was not read to the end",
            REPLY_MAX_BYTES / (1024 * 1024)
        )));
    }
    match read {
        Ok(_) => Ok(raw),
        // A TLS peer may close without close_notify once it answered: the
        // reply asked `connection: close`, so what arrived is the reply.
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof && !raw.is_empty() => Ok(raw),
        Err(e) => Err(e),
    }
}

/// The contract code a door's reply carries, or `None` when it carries
/// none: a reply that is no refusal of the contract's, e.g. a route the
/// door does not have. A code this contract version does not know reads
/// [`RefusalCode::Unknown`].
pub fn code(reply: &Value) -> Option<RefusalCode> {
    reply
        .get("code")
        .filter(|c| c.is_string())
        .and_then(|c| serde_json::from_value(c.clone()).ok())
}

/// Before anything is sent: an `https://` door shows the certificate its
/// pairing file names, or the producer refuses in words and sends nothing — not its
/// pairing code, not a batch.
pub fn check_door(door: &Door) -> io::Result<()> {
    let Some(pin) = door.pin() else {
        return Ok(());
    };
    let mut stream = door.connect_tcp()?;
    let (mut conn, seen) = pinned(door, pin)?;
    while conn.is_handshaking() {
        if let Err(e) = conn.complete_io(&mut stream) {
            return Err(refusal(door, pin, &seen).unwrap_or(e));
        }
    }
    conn.send_close_notify();
    let _ = conn.complete_io(&mut stream);
    Ok(())
}

/// The refusal, in words, when the door showed another certificate.
fn refusal(door: &Door, pin: [u8; 32], seen: &Seen) -> Option<io::Error> {
    let shown = (*seen.lock().ok()?)?;
    Some(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "the door at {} shows a certificate with the fingerprint sha256:{}, not the \
             sha256:{} its pairing file names: this is not the Wild it was paired with, or \
             that Wild's certificate changed. Nothing was sent; ask the operator to pair \
             this connector again",
            door.addr(),
            pin::hex(&shown),
            pin::hex(&pin)
        ),
    ))
}

/// A TLS client that trusts exactly the certificate whose SHA-256 is `pin`
/// — or, for a door given its own client configuration, what that trusts.
fn pinned(door: &Door, pin: [u8; 32]) -> io::Result<(rustls::ClientConnection, Seen)> {
    let seen = Seen::default();
    let config = match door.client_config() {
        Some(config) => config.clone(),
        None => pin::config(pin, seen.clone())?,
    };
    let name = pin::server_name(door.addr())?;
    let conn = rustls::ClientConnection::new(config, name).map_err(io::Error::other)?;
    Ok((conn, seen))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply larger than [`REPLY_MAX_BYTES`] is refused in words, not
    /// read into memory to the end.
    #[test]
    fn a_reply_past_the_cap_is_refused_not_read_to_the_end() {
        let fits = vec![b'x'; REPLY_MAX_BYTES as usize];
        let raw = exchange(io::Cursor::new(fits.clone()), b"", b"").map(|r| r.len());
        assert_eq!(raw.ok(), Some(fits.len()));
        let over = vec![b'x'; REPLY_MAX_BYTES as usize + 1];
        let e = exchange(io::Cursor::new(over), b"", b"").unwrap_err();
        assert!(e.to_string().contains("larger than 16 MiB"), "{e}");
    }

    /// A door's `Retry-After` is honoured up to [`RETRY_AFTER_MAX`]: a reply
    /// cannot park the producer for days.
    #[test]
    fn a_retry_after_is_honoured_up_to_the_cap() {
        let at = |v: &str| retry_after(&[("retry-after".into(), v.into())]);
        assert_eq!(at("30"), Some(Duration::from_secs(30)));
        assert_eq!(at("999999999"), Some(RETRY_AFTER_MAX));
        assert_eq!(at("soon"), None);
        assert_eq!(retry_after(&[]), None);
    }
}
