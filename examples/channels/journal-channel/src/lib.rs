//! `journal-channel` — the minimal CHANNEL-flavor (transport-axis) Tier-2
//! plugin.
//!
//! The copy-paste starting point for an operator channel: the host's
//! notifier calls this guest's `deliver()` once per fanned-out operator
//! notification, and the guest's "transport" is one bus publish — a JSON
//! journal entry on `wild.{tribe}.user.journal.notification`, observable
//! from `wild watch`'s Bus pane or `nats sub`. No external service, no
//! account, no token: the point is the host-calls-deliver → render →
//! hand-to-transport shape, not the transport.
//!
//! Direction (the part that trips people up): a channel plugin EXPORTS
//! `wild:operator-channel/channel` — the host is the CONSUMER. This is
//! the inverse of a tool-provider plugin, where the host calls tools on
//! demand; here the host pushes notifications TO the guest. The real
//! outbound reference is `plugins/channels/telegram-channel/` in the
//! development repository — same export, but the delivery is an HTTP
//! push over governed egress.

#![allow(clippy::all)]

wit_bindgen::generate!({
    path:  "wit",
    world: "journal-channel",
    generate_all,
});

use std::sync::atomic::{AtomicU64, Ordering};

use exports::wild::operator_channel::channel::{
    ActionSemantics, ChannelCapabilities, ChannelError, ChannelKind, DeliveryReceipt,
    Guest as ChannelGuest, Kind, Notification, PiiClass, Severity,
};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};

/// Stable plugin slug — must match the sidecar (`sidecar.json`); a
/// mismatch is a hard load error (ADR-0045 §5 cross-check). For an
/// operator channel the slug doubles as the channel TOKEN: it is what a
/// `system/channels.yaml` binding row names in `channel:`.
const SLUG: &str = "journal-channel";

/// Journal entries render the body verbatim, so the cap exists only to
/// demonstrate the truncation contract (`rendered-truncated`), not to fit
/// a vendor limit. Comfortably under NATS's default 1 MiB message size.
const MAX_BODY_CHARS: u32 = 16_384;

/// Per-instance journal sequence. Wasm components are single-threaded,
/// but the host POOLS instances and re-warms a trapped one — so this is
/// a per-instance teaching aid (gaps and resets are normal), not a
/// durable counter. Durable state belongs on the host side of a
/// capability, never in guest statics.
static SEQ: AtomicU64 = AtomicU64::new(0);

struct JournalChannel;

// ── The channel surface (host is the consumer) ─────────────────────

impl ChannelGuest for JournalChannel {
    /// Host-polled ONCE, at shim construction (the host caches it) — not
    /// per delivery. Declare what the surface honestly renders.
    fn capabilities() -> ChannelCapabilities {
        ChannelCapabilities {
            // `pull`: the operator inspects the journal (Bus pane,
            // `nats sub`) — this channel never pings an external
            // endpoint. Today the notifier fans out to every accepting
            // channel regardless of kind; the kind is routing metadata.
            kind: ChannelKind::Pull,
            // The journal preserves `body-md` verbatim, markdown intact.
            supports_rich_markdown: true,
            // A journal entry records actions; it cannot make them
            // clickable. Declare it honestly — Elder pre-filters on this.
            supports_inline_actions: false,
            max_body_chars: MAX_BODY_CHARS,
            rate_limit_per_minute: None,
        }
    }

    /// Invoked by the host once per fanned-out notification. Idempotent
    /// by `notification-id` (the host derives it deterministically, so a
    /// redelivery journals the same id — a reader can dedup on it).
    ///
    /// `config` is the operator's `system/channels.yaml` binding for
    /// `(journal-channel, tribe)` — empty when the tribe has no binding.
    /// A push channel with a recipient (chat id, address) must SKIP on
    /// empty config rather than guess; a journal has no recipient, so an
    /// unbound tribe journals all the same. We record only the KEYS: a
    /// journal must never copy operator config values (they can name
    /// secret aliases).
    fn deliver(
        n: Notification,
        config: Vec<(String, String)>,
    ) -> Result<DeliveryReceipt, ChannelError> {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let (body, truncated) = truncate_chars(&n.body_md, MAX_BODY_CHARS as usize);

        let entry = serde_json::json!({
            "channel": SLUG,
            "seq": seq,
            "delivered_at_ms": now_ms(),
            "notification": {
                "notification_id": n.notification_id,
                "tribe_id": n.tribe_id,
                "severity": severity_str(n.severity),
                "kind": kind_str(n.kind),
                "pii_class": pii_str(n.pii_class),
                "title": n.title,
                "body_md": body,
                "body_truncated": truncated,
                "actions": n.actions.iter().map(|a| serde_json::json!({
                    "action_id": a.action_id,
                    "label": a.label,
                    "semantics": semantics_str(a.semantics),
                })).collect::<Vec<_>>(),
                "deeplink": n.deeplink,
            },
            "config_keys": config.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        });
        let body_bytes = serde_json::to_vec(&entry)
            .map_err(|e| ChannelError::Permanent(format!("journal entry encode: {e}")))?;

        // The journal sink: one fire-and-forget publish, on the existing
        // `wild.{tribe}.user.{channel}.notification` subject shape
        // (`wild_core::subjects::tribe_user_notification`) — never a new
        // subject root.
        use crate::wild::messaging::consumer;
        use crate::wild::messaging::types::BrokerMessage;
        let subject = format!("wild.{}.user.journal.notification", n.tribe_id);
        consumer::publish(&BrokerMessage {
            subject,
            body: body_bytes,
            // Empty is fine: the host stamps the trusted identity
            // headers itself and they always win.
            headers: vec![],
        })
        // The bus being unreachable is transient — a fallback channel
        // may still reach the operator, so `unavailable`, not
        // `permanent`.
        .map_err(|e| ChannelError::Unavailable(format!("journal publish: {e}")))?;

        Ok(DeliveryReceipt {
            delivered_at: now_ms(),
            // Back-correlation handle for an (eventual) inbound reply —
            // for a journal it names the entry.
            channel_message_id: Some(format!("journal-{}-{seq}", n.notification_id)),
            rendered_truncated: truncated,
        })
    }
}

// ── Plugin lifecycle (wild:plugin-meta/plugin-base) ────────────────

impl MetaGuest for JournalChannel {
    /// Self-reported identity; the host cross-checks every field against
    /// the sidecar at load time. Keep `provides`/`requires` a mirror of
    /// wit/world.wit's export/import lists.
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: SLUG.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Provider),
            provides: vec!["wild:operator-channel/channel@0.1.0".into()],
            requires: vec!["wild:messaging/consumer@0.3.0".into()],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    /// No per-profile config to parse — an honest no-op. A real channel
    /// would decode its config bundle here and return a typed
    /// `InitError` so `wild doctor` can surface WHY it was skipped.
    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        Ok(())
    }

    /// Nothing to tear down — no connections, no caches.
    fn shutdown() {}
}

export!(JournalChannel);

// ── Helpers ────────────────────────────────────────────────────────

/// Epoch milliseconds off the WASI wall clock (the daemon-wide
/// convention for `delivered-at`; WIT has no timestamp type).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Truncate to at most `max` CHARS (never mid-code-point). Returns the
/// possibly-shortened string and whether it was cut — the honesty bit
/// the receipt's `rendered-truncated` carries back to the host.
fn truncate_chars(s: &str, max: usize) -> (String, bool) {
    match s.char_indices().nth(max) {
        Some((byte_idx, _)) => (s[..byte_idx].to_string(), true),
        None => (s.to_string(), false),
    }
}

// Enum → wire-string walks. The journal spells the enums out as the
// lowercase WIT names so a reader greps the entry against the contract.

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Blocker => "blocker",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::HelpNeeded => "help-needed",
    }
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::AuditFinding => "audit-finding",
        Kind::HelpRequest => "help-request",
        Kind::SystemNotice => "system-notice",
    }
}

fn pii_str(p: PiiClass) -> &'static str {
    match p {
        PiiClass::Public => "public",
        PiiClass::TribeScoped => "tribe-scoped",
        PiiClass::Secret => "secret",
    }
}

fn semantics_str(s: ActionSemantics) -> &'static str {
    match s {
        ActionSemantics::Fix => "fix",
        ActionSemantics::Ack => "ack",
        ActionSemantics::Reject => "reject",
        ActionSemantics::AnswerOption => "answer-option",
        ActionSemantics::Deeplink => "deeplink",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_sidecar_identity() {
        let m = JournalChannel::manifest();
        assert_eq!(m.slug, "journal-channel");
        assert_eq!(m.version, "0.1.0");
        assert!(matches!(m.kind, Some(PluginKind::Provider)));
        assert_eq!(m.provides, vec!["wild:operator-channel/channel@0.1.0"]);
        assert_eq!(m.requires, vec!["wild:messaging/consumer@0.3.0"]);
        assert!(m.config_keys.is_empty(), "a declared key must be read");
    }

    #[test]
    fn capabilities_are_honest_about_the_surface() {
        let c = JournalChannel::capabilities();
        assert!(matches!(c.kind, ChannelKind::Pull));
        assert!(c.supports_rich_markdown, "journal keeps markdown verbatim");
        assert!(!c.supports_inline_actions, "a journal entry is not clickable");
        assert_eq!(c.max_body_chars, MAX_BODY_CHARS);
    }

    #[test]
    fn truncate_cuts_on_char_boundaries() {
        let (s, cut) = truncate_chars("abc", 5);
        assert_eq!((s.as_str(), cut), ("abc", false));
        let (s, cut) = truncate_chars("héllo", 2);
        assert_eq!((s.as_str(), cut), ("hé", true));
        let (s, cut) = truncate_chars("", 0);
        assert_eq!((s.as_str(), cut), ("", false));
    }
}
