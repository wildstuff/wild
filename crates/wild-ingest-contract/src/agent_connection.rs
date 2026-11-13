//! An agent's connection (ADR-0365 D12 item 8): how a coding agent on
//! another computer reaches Wild's agent door, as a file the kit's MCP bridge
//! reads, so the agent's key reaches the bridge and never an LLM.
//!
//! Before it, the operator pasted the door's address and the agent's key into
//! the agent's MCP config by hand, or into its prompt, and the key landed with
//! the LLM provider; and the agent's MCP client, which trusts no certificate
//! Wild made itself, could not reach the door at all. Now Wild writes the
//! door, the pin of its certificate and the key into a file only the
//! operator's own user can read, and names only its path. On the agent's
//! computer the file lies in [`AGENT_CONNECTIONS_DIR`], inside the place the
//! kit keeps pairings in, so the rules `wild-kit init` gives the coding
//! agents' file tools keep them from it too. The agent's MCP client starts
//! [`MCP_COMMAND`] as a local server; the bridge pins the certificate and
//! sends the key.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where an agent's connection lies on the agent's computer, under that
/// user's home directory: inside [`crate::pairing_file::PRODUCER_PAIRINGS_DIR`],
/// outside every folder a coding agent works in.
pub const AGENT_CONNECTIONS_DIR: &str = ".wild-kit/pairings/agents";

/// The folder of a pairings place agents' connections lie in, on either
/// computer: [`AGENT_CONNECTIONS_DIR`] on the agent's, the profile's
/// pairings on Wild's.
pub const AGENTS_FOLDER: &str = "agents";

/// The kit's command that takes an agent's connection as pasted text, in a
/// terminal of the operator's own, and keeps it in [`AGENT_CONNECTIONS_DIR`].
pub const PASTE_COMMAND: &str = "wild-kit paste-agent";

/// The kit's command a coding agent's MCP client starts as its local server
/// for Wild: `wild-kit mcp <agent>`.
pub const MCP_COMMAND: &str = "wild-kit mcp";

/// Where the connection of `agent` belongs on the agent's computer, as the
/// operator types it: `~/.wild-kit/pairings/agents/<agent>.json`.
pub fn agent_place(agent: &str) -> String {
    format!("~/{AGENT_CONNECTIONS_DIR}/{agent}.json")
}

/// Whether `name` names an agent's connection: lowercase letters and digits,
/// joined by single dashes, at most 48 — a file name every file system and
/// shell takes unquoted.
pub fn is_agent_name(name: &str) -> bool {
    name.len() <= 48
        && !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// What an agent's connection holds: the agent door, the pin of its
/// certificate, and the agent's key. JSON, one object, written owner-only
/// (mode 0600).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentConnection {
    /// The agent door: `https://host:port/mcp` on the ingest door's listener.
    pub door: String,
    /// The pin of the door's certificate, `sha256:` and 64 hex digits. Absent for an `http://` door.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_certificate: Option<String>,
    /// The agent's key, sent as its bearer on every request and never shown
    /// or logged.
    pub agent_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_name_is_a_plain_file_name() {
        assert!(is_agent_name("opencode-vm") && is_agent_name("agent2"));
        for bad in [
            "",
            "Agent",
            "a--b",
            "-a",
            "a-",
            "../x",
            "a b",
            &"a".repeat(49),
        ] {
            assert!(!is_agent_name(bad), "{bad}");
        }
    }

    #[test]
    fn the_place_lies_inside_the_pairings_place() {
        assert_eq!(
            AGENT_CONNECTIONS_DIR,
            format!(
                "{}/{AGENTS_FOLDER}",
                crate::pairing_file::PRODUCER_PAIRINGS_DIR
            )
        );
        assert_eq!(
            agent_place("opencode-vm"),
            "~/.wild-kit/pairings/agents/opencode-vm.json"
        );
    }
}
