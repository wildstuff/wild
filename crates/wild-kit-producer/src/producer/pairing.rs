//! The pairing file (ADR-0365 D1 amendment, step 4): the door, the pin of its
//! certificate and the one-time code, which Wild writes for this program
//! alone so they never pass through a prompt. The producer reads it at its
//! first start and keeps the door and its pin with its key, so a later start
//! — the file removed once the announcement is made — finds the door again
//! with the same command. A file in a folder a coding agent works in is
//! refused before it is read: it belongs in `~/.wild-kit/pairings/`.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::{json, Value};
use wild_ingest_contract::pairing_file::{agent_folder, producer_place, PairingFile};

use super::door::{self, Door};

/// The door an earlier pairing file named, kept beside the key.
const KEPT: &str = "door.json";

/// The door `file` names, with its code while the file is there; once it is
/// gone, the door an earlier start kept in `key_dir`, with no code.
///
/// The first read keeps the door and its pin in `key_dir/door.json`, written
/// readable by this user alone: a later start finds the door again without
/// the pairing file. `key_dir` is made if it is not there and, on unix, set
/// to owner-only (0700) if it is — so pass a directory of its own, not a
/// folder others are meant to read.
pub fn door(
    file: &Path,
    key_dir: &Path,
    parse_pin: impl Fn(&str) -> Result<[u8; 32], String>,
) -> Result<(Door, Option<String>), String> {
    // Before a byte is read: a file in an agent's folder is one the agent
    // lists, reads and commits, so it is refused, and where it belongs is said.
    let home = home_dir();
    if let Some((folder, marker)) = agent_folder(file, home.as_deref()) {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(format!(
            "the pairing file {} lies in {}, a folder a coding agent works in (it holds {marker}): \
             there the agent can read it and git can commit it. Move it to {} and start this \
             program with --pairing {}",
            file.display(),
            folder.display(),
            producer_place(""),
            producer_place(&name),
        ));
    }
    let kept = key_dir.join(KEPT);
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let text = fs::read_to_string(&kept).map_err(|_| {
                format!(
                    "the pairing file {} is not there, and this install kept no door from an \
                     earlier one: pair the program again and start it with the new file",
                    file.display()
                )
            })?;
            let v: Value = serde_json::from_str(&text).map_err(|e| {
                format!(
                    "the door kept in {} reads wrong {}; pair the program again",
                    kept.display(),
                    where_wrong(&e)
                )
            })?;
            let at = v["door"].as_str().unwrap_or_default();
            let pin = v["door_certificate"].as_str().map(str::to_string);
            return Ok((door::tcp(at, pin, &parse_pin)?, None));
        }
        Err(e) => {
            return Err(format!(
                "could not read the pairing file {}: {e}",
                file.display()
            ))
        }
    };
    let pairing: PairingFile = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not a pairing file Wild wrote (it reads wrong {}); pair the program again \
             for a new one",
            file.display(),
            where_wrong(&e)
        )
    })?;
    let door = door::tcp(&pairing.door, pairing.door_certificate.clone(), &parse_pin)?;
    let keep = json!({"door": pairing.door, "door_certificate": pairing.door_certificate});
    owner_only_dir(key_dir)
        .and_then(|()| super::write_owner_only(&kept, keep.to_string().as_bytes()))
        .map_err(|e| format!("could not keep the door in {}: {e}", kept.display()))?;
    Ok((door, Some(pairing.pairing_code)))
}

/// Where a JSON file reads wrong — line and column, never the text there:
/// `serde_json` quotes the value it could not take, and in a pairing file
/// that value may be the pairing code.
fn where_wrong(e: &serde_json::Error) -> String {
    format!("at line {}, column {}", e.line(), e.column())
}

/// The directory the key and the kept door live in, readable by this user
/// alone: made so where it is made, and tightened so where it was already
/// there — a mode given at creation leaves an existing directory as it was.
fn owner_only_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    #[cfg(unix)]
    fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

/// The user's home directory: `HOME`, or on Windows `USERPROFILE`.
///
/// A deliberate mirror of `wild_core::user_dirs::home_dir`: this crate links
/// no `wild-core` (the dependency-rule test), and `wild-core` cannot take the
/// contract's dependencies. The `wild-kit-cli` test `home_rule_mirror` compares
/// both on every environment branch, so a drift fails there.
#[doc(hidden)]
pub fn home_dir() -> Option<std::path::PathBuf> {
    home_from(
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
        cfg!(windows),
    )
}

/// The rule behind [`home_dir`], with the environment passed in: an empty
/// value counts as unset, and `USERPROFILE` counts only on Windows.
fn home_from(
    home: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
    windows: bool,
) -> Option<std::path::PathBuf> {
    let set = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty());
    set(home)
        .or_else(|| if windows { set(profile) } else { None })
        .map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_pin(_: &str) -> Result<[u8; 32], String> {
        Err("no pin expected".into())
    }

    fn write_pairing(dir: &Path) -> std::path::PathBuf {
        let file = dir.join("pairing.json");
        let body = json!({"door": "http://127.0.0.1:9", "pairing_code": "code-1"});
        fs::write(&file, body.to_string()).unwrap();
        file
    }

    #[test]
    fn home_prefers_home_ignores_empty_values_and_falls_back_to_userprofile_only_on_windows() {
        let os = |v: &str| Some(std::ffi::OsString::from(v));
        assert_eq!(home_from(os("/h"), os("C:\\u"), true), Some("/h".into()));
        assert_eq!(home_from(os(""), os("C:\\u"), true), Some("C:\\u".into()));
        assert_eq!(home_from(None, os("C:\\u"), true), Some("C:\\u".into()));
        assert_eq!(home_from(None, os("C:\\u"), false), None);
        assert_eq!(home_from(None, os(""), true), None);
        assert_eq!(home_from(os(""), None, false), None);
    }

    #[test]
    fn the_first_read_keeps_the_door_for_this_user_alone_and_a_later_start_finds_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_pairing(dir.path());
        let keys = dir.path().join("keys");
        let (first, code) = door(&file, &keys, no_pin).unwrap();
        assert_eq!(code.as_deref(), Some("code-1"));
        assert_eq!(first.addr(), "127.0.0.1:9");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(keys.join(KEPT)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "door.json is for this user alone");
            let mode = fs::metadata(&keys).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o777,
                0o700,
                "the keys folder is for this user alone"
            );
        }

        fs::remove_file(&file).unwrap();
        let (again, code) = door(&file, &keys, no_pin).unwrap();
        assert_eq!(code, None, "a later start has no code");
        assert_eq!(again.addr(), "127.0.0.1:9");
    }

    /// A keys folder already there, readable by others, is tightened when
    /// the door is kept in it — the mode a folder is made with does not
    /// reach one that exists.
    #[cfg(unix)]
    #[test]
    fn a_keys_folder_already_there_is_made_this_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = write_pairing(dir.path());
        let keys = dir.path().join("keys");
        fs::create_dir(&keys).unwrap();
        fs::set_permissions(&keys, fs::Permissions::from_mode(0o755)).unwrap();
        door(&file, &keys, no_pin).unwrap();
        let mode = fs::metadata(&keys).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "the keys folder is for this user alone"
        );
    }

    /// A pairing file that reads wrong is named with where it reads wrong,
    /// never with what is there: that may be the pairing code.
    #[test]
    fn a_pairing_file_that_reads_wrong_does_not_repeat_its_code() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pairing.json");
        let secret = "s3cret-pairing-code";
        for body in [
            // `serde_json` quotes a string it cannot take for the whole file.
            json!(secret).to_string(),
            format!("{{\"door\": \"http://127.0.0.1:9\", \"pairing_code\": {secret}}}"),
        ] {
            fs::write(&file, &body).unwrap();
            let err = door(&file, dir.path(), no_pin).err().expect(&body);
            assert!(err.contains("is not a pairing file"), "{err}");
            assert!(err.contains("at line 1"), "{err}");
            assert!(!err.contains("s3cret"), "{err}");
        }
    }

    #[test]
    fn a_missing_file_with_no_kept_door_says_to_pair_again() {
        let dir = tempfile::tempdir().unwrap();
        let err = door(&dir.path().join("none.json"), dir.path(), no_pin)
            .err()
            .expect("an error");
        assert!(err.contains("kept no door"), "{err}");
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_taken_for_a_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        // A kept door is there; a directory where the file should be is a
        // read error, not "not there", so the kept door is not used.
        fs::write(
            dir.path().join(KEPT),
            json!({"door": "http://127.0.0.1:9"}).to_string(),
        )
        .unwrap();
        let err = door(dir.path(), dir.path(), no_pin)
            .err()
            .expect("an error");
        assert!(err.contains("could not read the pairing file"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn the_home_is_an_absolute_folder_where_the_environment_names_one() {
        // A login environment always names a home; an empty or invented path
        // would not be absolute.
        let home = home_dir().expect("the test environment names a home");
        assert!(home.is_absolute(), "{}", home.display());
    }
}
