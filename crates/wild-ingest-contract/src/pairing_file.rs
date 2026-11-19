//! The pairing file (ADR-0365 D1 amendment, step 4): the pairing line as a
//! file the program reads, so it reaches the program and never an LLM.
//!
//! Before it, the operator copied the line `--door … --door-certificate …
//! --pairing-code …` into the coding agent's prompt, and the pairing code
//! landed with the LLM provider. Now Wild writes the line's three parts into
//! a file only the operator's own user can read, and names only its path; the
//! operator hands the program that path, and the agent may be told it.
//!
//! On the program's computer the file has one place,
//! [`PRODUCER_PAIRINGS_DIR`] under the home directory, outside every folder a
//! coding agent works in. The operator copies it there, or pastes its text
//! into [`PASTE_COMMAND`], which writes it there.
//!
//! The program reads the file at its first start, keeps the door and its pin
//! with its key, announces itself with the code, and removes the file once
//! its state holds the announcement. A later start finds no file and reads
//! the door from what it kept, so a service is started with the same command
//! every time.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a pairing file lies on the computer its connector runs on, under
/// that user's home directory: one place outside every folder a coding agent
/// works in, so the agent is never handed it and git never commits it.
/// `wild-kit init` makes it, `wild-kit paste-pairing` writes into it, and Wild
/// names it when it pairs a connection on another computer.
pub const PRODUCER_PAIRINGS_DIR: &str = ".wild-kit/pairings";

/// The kit's command that takes a pairing as pasted text, in a terminal of the
/// operator's own, and keeps it as a file in [`PRODUCER_PAIRINGS_DIR`].
pub const PASTE_COMMAND: &str = "wild-kit paste-pairing";

/// Where the pairing file named `file_name` belongs on the connector's
/// computer, as the operator types it: `~/.wild-kit/pairings/<file_name>`.
pub fn producer_place(file_name: &str) -> String {
    format!("~/{PRODUCER_PAIRINGS_DIR}/{file_name}")
}

/// The command that keeps a pasted pairing under `file_name` there:
/// `wild-kit paste-pairing <name>`, the name without `.json`.
pub fn paste_command(file_name: &str) -> String {
    let name = file_name.strip_suffix(".json").unwrap_or(file_name);
    format!("{PASTE_COMMAND} {name}")
}

/// What a pairing file holds: the door, the pin of its certificate, and the
/// one-time code. JSON, one object, written owner-only (mode 0600).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PairingFile {
    /// The door's address: `https://host:port` on the local network or this
    /// computer; `http://host:port` only for a door on this computer — a
    /// loopback host, or `10.0.2.2` from a QEMU user-mode VM.
    pub door: String,
    /// The pin of the door's certificate, `sha256:` and 64 hex digits. Absent for an `http://` door.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_certificate: Option<String>,
    /// The one-time code that opens the program's announcement: sent once, as
    /// the `wild-pairing-code` header, and never shown or logged.
    pub pairing_code: String,
}

/// What marks a folder a coding agent works in: a git work tree, or the
/// skills `wild-kit init` installs for Claude Code, Codex and OpenCode.
const AGENT_MARKERS: [&str; 3] = [".git", ".claude", ".agents"];

/// The folder a coding agent works in that holds `file`, with what marks it —
/// or `None`. There the agent lists, reads and commits what lies around it,
/// so a pairing file there reaches the agent after all. The walk stops below
/// `home`: the home directory carries `~/.claude` for any user of Claude
/// Code, and the kit's own place lies under it.
pub fn agent_folder(file: &Path, home: Option<&Path>) -> Option<(PathBuf, &'static str)> {
    let absolute = if file.is_absolute() {
        file.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(file)
    };
    let home = home.and_then(|h| std::fs::canonicalize(h).ok());
    // Only folders that exist can mark anything; each is compared as the
    // file system resolves it (`/var` is `/private/var` on macOS).
    for folder in absolute.ancestors().skip(1) {
        let Ok(folder) = std::fs::canonicalize(folder) else {
            continue;
        };
        if Some(&folder) == home.as_ref() || folder.parent().is_none() {
            return None;
        }
        if let Some(marker) = AGENT_MARKERS.iter().find(|m| folder.join(m).exists()) {
            return Some((folder, marker));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_place_and_the_paste_command_name_the_same_file() {
        assert_eq!(
            producer_place("buero-laptop.json"),
            "~/.wild-kit/pairings/buero-laptop.json"
        );
        assert_eq!(
            paste_command("buero-laptop.json"),
            "wild-kit paste-pairing buero-laptop"
        );
    }

    #[test]
    fn a_plain_door_has_no_pin_and_an_unknown_field_is_read_past() {
        let file: PairingFile = serde_json::from_str(
            r#"{"door":"http://127.0.0.1:7600","pairing_code":"wdp_a_b","later":1}"#,
        )
        .unwrap();
        assert_eq!(file.door_certificate, None);
        assert_eq!(
            serde_json::to_string(&file).unwrap(),
            r#"{"door":"http://127.0.0.1:7600","pairing_code":"wdp_a_b"}"#
        );
    }

    #[test]
    fn a_pairing_file_in_an_agents_folder_is_found_and_the_kits_place_is_not() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        // Every user of Claude Code has `~/.claude`: it marks nothing.
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let kept = home.join(".wild-kit/pairings/buero-laptop.json");
        assert_eq!(agent_folder(&kept, Some(home)), None);

        for marker in [".git", ".claude", ".agents"] {
            let project = home.join(format!("connector{marker}"));
            std::fs::create_dir_all(project.join(marker)).unwrap();
            std::fs::create_dir_all(project.join("deep/er")).unwrap();
            let file = project.join("deep/er/buero-laptop.json");
            let (folder, found) = agent_folder(&file, Some(home)).expect(marker);
            assert_eq!(found, marker);
            assert_eq!(folder, std::fs::canonicalize(&project).unwrap());
        }
        let loose = home.join("Downloads/buero-laptop.json");
        assert_eq!(agent_folder(&loose, Some(home)), None);
    }
}
