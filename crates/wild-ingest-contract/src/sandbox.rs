//! The marker of a `wild-kit` sandbox profile (ADR-0365 D12 item 7).
//!
//! `wild-kit` writes the marker into the sandbox it serves and opens only a
//! marked (or empty) directory; the real daemon refuses a marked one. Both
//! read the name from here, so the two sides cannot drift apart.

use std::path::Path;

/// The marker file's name at the sandbox root.
pub const SANDBOX_MARKER: &str = "WILD-KIT-SANDBOX";

/// Whether `root` carries the sandbox marker.
pub fn is_sandbox_profile(root: &Path) -> bool {
    root.join(SANDBOX_MARKER).is_file()
}
