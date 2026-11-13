//! The producer half of the ingest contract (ADR-0365, ADR-0398 D10), once:
//! what a connector's developer builds on instead of writing the contract a
//! second time. The kit's reference producer is its first user.
//!
//! - [`door`], [`pairing`] and [`pin`] say where the door is and how it is
//!   reached and trusted;
//! - [`http`] makes one request to it;
//! - [`calls`] answers the calls Wild makes on the connector's announced
//!   tools: the call wait, one handler per tool, `refused` as an answer;
//! - [`mcp`] calls the other way: the tribe's view, with the connector's
//!   own device key (ADR-0398 D13).

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// The TLS library [`pin::config`] returns a configuration of.
pub use rustls;
/// The refusal codes a door answers with, so a producer matches on the contract's
/// own type without naming `wild-ingest-contract` itself.
pub use wild_ingest_contract::codes::RefusalCode;

pub mod calls;
pub mod door;
pub mod http;
pub mod mcp;
pub mod pairing;
pub mod pin;

/// Write a file in one step — whole, or not at all — readable by this user
/// alone: what a producer keeps (its key, its progress, the answers it gave,
/// what it read) is the user's data. The staging file is created anew, never
/// opened through a link someone left at its name.
pub fn write_owner_only(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Staging name exempt from the fixed-suffix gate: a producer writing its own files, outside any tribe folder.
    let tmp = path.with_extension("tmp");
    let _ = fs::remove_file(&tmp);
    let mut open = fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    open.open(&tmp)?.write_all(bytes)?;
    fs::rename(&tmp, path)
}

/// `text` with every control character, and every bidi or invisible format
/// character, replaced by `\u{FFFD}`. Text a door sent — a state, an id, an
/// error code — goes through this before it is printed, so nothing from
/// outside can move the cursor, recolour or retitle the terminal a producer
/// runs in, nor reorder or hide what the line shows.
#[must_use]
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || is_format(c) {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect()
}

/// The format characters (Unicode category `Cf`) a terminal line can carry
/// unseen: the bidi embeddings, overrides, isolates and marks, which reorder
/// what is shown, and the zero-width and invisible ones, which hide or join.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A link someone left at the staging name is replaced, not written
    /// through; the file written is this user's alone.
    #[test]
    fn a_write_goes_through_no_link_and_is_for_this_user_alone() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, "untouched").unwrap();
        let path = dir.path().join("progress.json");
        std::os::unix::fs::symlink(&victim, path.with_extension("tmp")).unwrap();

        write_owner_only(&path, b"{}").unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
