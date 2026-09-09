//! `embed-consumer` — reference `wild:ai/embed` caller (Wasm component).
//!
//! Wakes on a NATS subject (the manifest-declared subscription, e.g.
//! `wild.{tribe}.embed.req`), embeds the message body, and publishes the
//! result. One invocation = one embed round-trip:
//!
//!   1. Decode the input: `String::from_utf8_lossy(&msg.body)` — the
//!      body is the raw text to embed (no envelope; a consumer keeps its
//!      own wire shape, the host doesn't interpret the bytes).
//!   2. Leave `request.model` as `none` and set `caller_context.kind` to a
//!      caller token. The host resolves the adapter through the embed
//!      routing map (`<profile_root>/embed-adapters.yaml`), so an operator can
//!      move this consumer to a different model in one YAML edit. An
//!      explicit `model` is still allowed as an author pin. See
//!      `docs/embed-adapters.md` § "Calling embed from a component".
//!   3. Call `wild:ai/embed.embed(EmbedRequest{ input: vec![text], … })`.
//!      `input` is a BATCH — `embed-response.embeddings` is index-aligned
//!      to it. This example embeds one string (a one-element batch); a
//!      real RAG/dedup caller passes many and reads `embeddings[i]`.
//!   4. On `Ok`  → publish `{model, dims, count, preview}` JSON to
//!      `<subject>.result` (preview = the first few vector components, so
//!      the result stays small — full vectors go to a vector store, out
//!      of scope here).
//!      On `Err` → publish the error string to `<subject>.error`.
//!
//! Deliberately generic: it embeds whatever text arrives. The motivating
//! use is entity-linking (contract text → vector → candidate match),
//! but nothing here is domain-specific.
//!
//! `caller-context`: the `wild:ai/embed` request carries an optional
//! `caller-context` whose `kind` is the routing token. The host injects
//! the tribe at the WIT seam (`caller_org`) for telemetry — the Tier-2
//! ADAPTER contract never sees tribe identity at all.

#![allow(clippy::all)]

wit_bindgen::generate!({
    world: "embed-consumer",
    path: "wit",
    generate_all,
});

use exports::wild::messaging::handler::{BrokerMessage, Guest as HandlerGuest};
use exports::wild::plugin_meta::meta::{Guest as PluginMetaGuest, InitError, PluginManifest};

use crate::wild::ai::embed::{embed, CallerContext, EmbedRequest};

/// Caller token for this consumer. Operators can pin this token in
/// `<profile_root>/embed-adapters.yaml` under `caller_routing:`.
const CALLER_KIND: &str = "embed-consumer-example";

/// How many leading vector components to echo in the result. Keeps the
/// result message small — the full vector belongs in a vector store
/// (`wild:data`, out of scope), not on the result subject.
const PREVIEW_LEN: usize = 4;

struct EmbedConsumer;

impl HandlerGuest for EmbedConsumer {
    /// Wake-up callback — one embed round-trip per invocation.
    ///
    /// Agent-level failures (missing model config, embed error) publish
    /// a structured error to `<subject>.error` and return `Ok(())`:
    /// embed errors are commonly permanent (bad model id, malformed
    /// input), so NACK-redelivering them would loop. The error subject
    /// is the observable signal (it rides the NATS firehose +
    /// `wild watch` Bus pane). The `Err(_)` return is reserved for
    /// unrecoverable plumbing (a publish that itself fails).
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        let result_subject = format!("{}.result", msg.subject);
        let error_subject = format!("{}.error", msg.subject);

        let text = String::from_utf8_lossy(&msg.body).to_string();
        if text.trim().is_empty() {
            return publish(
                &error_subject,
                "empty message body — nothing to embed".as_bytes().to_vec(),
            );
        }

        // The bare call. `input` is a batch; here a one-element batch.
        // `model: None` routes through the host's embed resolver keyed on
        // `caller_context.kind`; an explicit `Some(id)` would be an author
        // pin. `truncate: Some(true)` asks the adapter to clamp over-long
        // input to the model's max instead of erroring.
        let request = EmbedRequest {
            input: vec![text],
            model: None,
            caller_context: Some(CallerContext {
                kind: CALLER_KIND.into(),
                tribe_id: "".into(),
                cycle_id: "".into(),
            }),
            dimensions: None,
            truncate: Some(true),
        };

        match embed(&request) {
            Ok(resp) => {
                // `dims` is authoritative — size storage off it BEFORE
                // iterating `embeddings`. Each inner vector has length
                // `dims`; `embeddings` is index-aligned to `input`.
                let first = resp.embeddings.first();
                let preview: Vec<f32> = first
                    .map(|v| v.iter().take(PREVIEW_LEN).copied().collect())
                    .unwrap_or_default();
                let out = ResultBody {
                    model: resp.model,
                    dims: resp.dims,
                    count: resp.embeddings.len() as u32,
                    preview,
                };
                let body = serde_json::to_vec(&out).map_err(|e| format!("encode result: {e}"))?;
                publish(&result_subject, body)
            }
            Err(e) => publish(
                &error_subject,
                format!("embed `{CALLER_KIND}`: {e}").into_bytes(),
            ),
        }
    }
}

/// The result published to `<subject>.result`. Intentionally a preview,
/// not the full vector — a real consumer writes the vector to a store.
#[derive(serde::Serialize)]
struct ResultBody {
    /// Provider model id that produced the vectors.
    model: String,
    /// Vector dimensionality (length of every inner vector).
    dims: u32,
    /// Number of vectors returned (index-aligned to the input batch).
    count: u32,
    /// First [`PREVIEW_LEN`] components of the first vector.
    preview: Vec<f32>,
}

/// Fire-and-forget publish on the broker.
fn publish(subject: &str, body: Vec<u8>) -> Result<(), String> {
    use crate::wild::messaging::consumer;
    use crate::wild::messaging::types::BrokerMessage as ProducerMessage;
    consumer::publish(&ProducerMessage {
        subject: subject.to_string(),
        body,
        headers: vec![],
    })
}

/// Tier-2 plugin self-report. The host calls `meta::manifest()` at load
/// time and cross-checks slug/version/kind/provides/requires against the
/// sidecar (ADR-0045 §5). `init`/`shutdown` are no-ops: workload-flavor
/// per-instance config arrives via `wasi:config/runtime`, not the
/// `meta::init` config bundle.
impl PluginMetaGuest for EmbedConsumer {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "embed-consumer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            // Workload-flavor: exports `wild:messaging/handler`. `kind` is
            // optional/derived in the unified sidecar schema (ADR-0141 PR0.5),
            // so single-role plugins may report `None` and let the host derive
            // it from `provides[]`.
            kind: None,
            provides: vec!["wild:messaging/handler@0.3.0".into()],
            // Mirror of `wit/world.wit`'s import list; the trust gate
            // reads it to decide which caps are granted under the
            // plugin's tier.
            requires: vec![
                // plugin-meta is the EXPORTED lifecycle contract, not a
                // capability import — excluded (the host filters it anyway).
                "wild:ai/embed@0.4.0".into(),
                "wild:messaging/consumer@0.3.0".into(),
            ],
            // No per-instance config: the embed model is resolved by the
            // host from `caller_context.kind` + `<profile_root>/embed-adapters.yaml`.
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        // No-op: per-instance config flows through `wasi:config/runtime`
        // on each invocation, not through the `meta::init` bundle.
        Ok(())
    }

    fn shutdown() {
        // No-op: each instance is single-message-scoped.
    }
}

export!(EmbedConsumer);
