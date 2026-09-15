//! `annotate-worker` — the minimal WORKLOAD-flavor Tier-2 plugin.
//!
//! The copy-paste starting point for a worker: the host wakes it once per
//! NATS message on the sidecar-declared subscription
//! (`wild.{tribe}.worker.{agent-name}.task`), it does a small DETERMINISTIC
//! transformation of the task payload — annotate + uppercase the prompt,
//! count words — and publishes a `WorkerResultEvent` back on the task's
//! `result_subject`. No LLM, no external services: the point is the
//! wake-on-subscription → work → publish-outcome shape, nothing else.
//!
//! The wire shapes are the host's `Envelope<WorkerTask>` (inbound) and
//! `Envelope<WorkerResultEvent>` (outbound), mirrored here as small local
//! serde structs. A plugin built inside the development repository would
//! use `common::worker_runtime` (`decode_worker_task`,
//! `encode_worker_result`, `result_for_task`) instead of hand-rolling
//! them; a self-contained example spells the JSON out so the contract is
//! visible.

#![allow(clippy::all)]

wit_bindgen::generate!({
    path:  "wit",
    world: "annotate-worker",
    generate_all,
});

use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::worker::handler::{BrokerMessage, Guest as HandlerGuest};
use exports::wild::worker::meta::{Guest as WorkerMetaGuest, TriggerSpec};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable plugin slug — must match the sidecar (`sidecar.json`); a
/// mismatch is a hard load error (ADR-0045 §5 cross-check).
const SLUG: &str = "annotate-worker";

struct AnnotateWorker;

// ── Inbound wire shape ─────────────────────────────────────────────
// A lenient subset of the host's `Envelope<WorkerTask>`: only the fields
// this worker reads. serde ignores the rest (`context_keys`,
// `timeout_seconds`, optional trigger contexts, …), which keeps the
// example wire-compatible as the task shape grows additively.

#[derive(Deserialize)]
struct TaskEnvelope {
    /// Unique id of the task message — becomes the result's `parent_id`.
    id: Uuid,
    /// Set on per-tribe subjects; echoed onto the result envelope.
    #[serde(default)]
    tribe_id: Option<String>,
    /// Propagates across the whole user-input → cycle → task chain;
    /// echoed verbatim so the host can route the result to its cycle.
    trace_id: Uuid,
    payload: WorkerTask,
}

#[derive(Deserialize)]
struct WorkerTask {
    task_id: Uuid,
    tribe_id: String,
    cycle_id: Uuid,
    /// Per-instance routing slug — appears in the subject suffix.
    worker_name: String,
    /// Wasm image identity.
    component_type: String,
    /// The text this worker transforms.
    prompt: String,
    /// Where the worker publishes its `WorkerResultEvent`. The
    /// `worker.bus.basic` capability bundle grants publish on exactly
    /// this per-worker result subject.
    result_subject: String,
}

// ── Outbound wire shape ────────────────────────────────────────────
// The host's `Envelope<WorkerResultEvent>`: a child envelope preserving
// the task's trace id / parent id so the result subscriber can route it
// back to the originating cycle.

#[derive(Serialize)]
struct ResultEnvelope {
    v: u32,
    id: Uuid,
    ts: chrono::DateTime<chrono::Utc>,
    tribe_id: Option<String>,
    trace_id: Uuid,
    parent_id: Option<Uuid>,
    payload: WorkerResultEvent,
}

#[derive(Serialize)]
struct WorkerResultEvent {
    task_id: Uuid,
    tribe_id: String,
    cycle_id: Uuid,
    worker_name: String,
    component_type: String,
    /// `"completed"` | `"failed"` (the locked status vocabulary; this
    /// worker never times out and never escalates).
    status: &'static str,
    result_key: Option<String>,
    summary: Option<String>,
    tokens_used: u32,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result_json: Option<serde_json::Value>,
}

// ── The worker primitive ───────────────────────────────────────────

impl HandlerGuest for AnnotateWorker {
    /// Invoked by the host once per delivered task message.
    ///
    /// `Err(..)` NACKs the delivery (the host logs it and redelivers per
    /// its policy) — reserve it for "could not even read the task". A
    /// task that was READ but cannot be done publishes a `failed` result
    /// instead: a failure is an outcome, and the chief only sees
    /// outcomes that arrive on the result subject.
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        let env: TaskEnvelope =
            serde_json::from_slice(&msg.body).map_err(|e| format!("task envelope decode: {e}"))?;

        let text = env.payload.prompt.trim();
        if text.is_empty() {
            return publish_result(
                &env,
                "failed",
                "task prompt is empty — nothing to annotate".to_string(),
                None,
            );
        }

        // The work: a deterministic transformation of the task payload.
        let annotated = format!("[{SLUG}] {}", text.to_uppercase());
        let words = text.split_whitespace().count();
        let summary = format!("annotated {} chars ({words} words)", text.chars().count());
        let result_json = serde_json::json!({
            "original": text,
            "annotated": annotated,
            "chars": text.chars().count(),
            "words": words,
        });

        publish_result(&env, "completed", summary, Some(result_json))
    }
}

impl WorkerMetaGuest for AnnotateWorker {
    /// Optional runtime trigger discovery — lets a dashboard render this
    /// worker's choosable triggers without hard-coding them. The sidecar
    /// still carries the default subscription template.
    fn list_triggers() -> Vec<TriggerSpec> {
        vec![TriggerSpec {
            name: "annotate".into(),
            description:
                "Annotate the task prompt: prefix + uppercase + word/char counts. \
                 Deterministic — no LLM, no external services."
                    .into(),
            default_subject: Some("wild.{tribe}.worker.{agent-name}.task".into()),
        }]
    }
}

// ── Plugin lifecycle (wild:plugin-meta/plugin-base) ────────────────

impl MetaGuest for AnnotateWorker {
    /// Self-reported identity; the host cross-checks every field against
    /// the sidecar at load time. Keep `provides`/`requires` a mirror of
    /// wit/world.wit's export/import lists.
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: SLUG.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Workload),
            provides: vec![
                "wild:worker/handler@0.1.0".into(),
                "wild:worker/meta@0.1.0".into(),
            ],
            requires: vec!["wild:messaging/consumer@0.3.0".into()],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    /// No per-profile config to parse — an honest no-op. A real worker
    /// would decode its config bundle here and return a typed
    /// `InitError` (`missing-config-key`, `backend-init`, …) so
    /// `wild doctor` can surface WHY it was skipped.
    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        Ok(())
    }

    /// Nothing to tear down — no connections, no caches.
    fn shutdown() {}
}

export!(AnnotateWorker);

// ── Outcome publish ────────────────────────────────────────────────

/// Build the child result envelope and publish it on the task's
/// `result_subject` via the host's `wild:messaging/consumer`.
fn publish_result(
    env: &TaskEnvelope,
    status: &'static str,
    summary: String,
    result_json: Option<serde_json::Value>,
) -> Result<(), String> {
    let task = &env.payload;
    let out = ResultEnvelope {
        v: 1,
        id: Uuid::new_v4(),
        ts: chrono::Utc::now(),
        tribe_id: env.tribe_id.clone(),
        trace_id: env.trace_id,
        parent_id: Some(env.id),
        payload: WorkerResultEvent {
            task_id: task.task_id,
            tribe_id: task.tribe_id.clone(),
            cycle_id: task.cycle_id,
            worker_name: task.worker_name.clone(),
            component_type: task.component_type.clone(),
            status,
            result_key: None,
            summary: Some(summary),
            tokens_used: 0,
            duration_ms: 0,
            result_json,
        },
    };
    let body = serde_json::to_vec(&out).map_err(|e| format!("result encode: {e}"))?;

    use crate::wild::messaging::consumer;
    use crate::wild::messaging::types::BrokerMessage as OutMessage;
    consumer::publish(&OutMessage {
        subject: task.result_subject.clone(),
        body,
        // Empty is fine: the host stamps the trusted `wild-actor` /
        // `wild-originator` identity headers itself and they always win.
        headers: vec![],
    })
    .map_err(|e| format!("result publish: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_sidecar_identity() {
        let m = AnnotateWorker::manifest();
        assert_eq!(m.slug, "annotate-worker");
        assert_eq!(m.version, "0.1.0");
        assert!(matches!(m.kind, Some(PluginKind::Workload)));
        assert!(m.provides.iter().any(|p| p == "wild:worker/handler@0.1.0"));
        assert!(m.provides.iter().any(|p| p == "wild:worker/meta@0.1.0"));
        assert_eq!(m.requires, vec!["wild:messaging/consumer@0.3.0"]);
    }

    #[test]
    fn task_envelope_subset_decodes_real_task_json() {
        // A real Envelope<WorkerTask> carries more fields than the local
        // subset reads — decoding must tolerate them.
        let body = serde_json::json!({
            "v": 1,
            "id": "2e9f2a7e-0000-4000-8000-000000000001",
            "ts": "2026-08-14T12:00:00Z",
            "tribe_id": "demo",
            "trace_id": "2e9f2a7e-0000-4000-8000-000000000002",
            "parent_id": null,
            "payload": {
                "task_id": "2e9f2a7e-0000-4000-8000-000000000003",
                "tribe_id": "demo",
                "cycle_id": "2e9f2a7e-0000-4000-8000-000000000004",
                "worker_name": "annotate",
                "component_type": "annotate-worker",
                "prompt": "the quick brown fox",
                "context_keys": [],
                "result_subject": "wild.demo.worker.annotate.result",
                "timeout_seconds": 30
            }
        });
        let env: TaskEnvelope = serde_json::from_slice(&serde_json::to_vec(&body).unwrap())
            .expect("subset decodes the full task JSON");
        assert_eq!(env.payload.prompt, "the quick brown fox");
        assert_eq!(env.payload.result_subject, "wild.demo.worker.annotate.result");
    }
}
