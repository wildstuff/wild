//! `overlap-rerank` — deterministic teaching adapter for the Tier-2
//! `wild:rerank-adapter@0.1.0` contract.
//!
//! No network, no secrets, no model. The "cross-encoder" is a
//! term-overlap score: query and document are tokenized to lowercase
//! word sets and each document scores the Jaccard index
//! `|Q ∩ D| / |Q ∪ D|` against the query. Deterministic — the same
//! request always produces the same scores and the same order — so the
//! retrieve → rerank second stage can be exercised end-to-end on any
//! machine:
//!
//!   - `wild:plugin-meta/meta` — `manifest` / `init` / `shutdown`, the
//!     lifecycle the host's `ComponentBackedHostPlugin` shim drives on
//!     every Tier-2 plugin.
//!   - `wild:rerank-adapter/rerank` — `info()` (the capability profile)
//!     and the single one-shot `rerank()` call. Rerank has no streaming
//!     sibling; there is nothing else to implement.
//!
//! To turn this into a real provider adapter: keep every signature,
//! replace [`score`] with an HTTP round-trip to a Jina-style
//! `POST /v1/rerank` endpoint (through the world's inherited
//! `wild:http/outbound` import — the GOVERNED egress call, ADR-0090),
//! and read the api-key via `wild:secrets/store` if the provider needs
//! one. `plugins/rerank/llama/` in the development repository is the
//! fully-worked version of that step.

use std::cell::Cell;
use std::collections::BTreeSet;

wit_bindgen::generate!({
    path:  "wit",
    world: "overlap-rerank",
    generate_all,
});

use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::rerank_adapter::rerank::{
    AdapterInfo, Guest as RerankGuest, RerankError, RerankRequest, RerankResponse, RerankResult,
    TokenUsage,
};

// Lifecycle marker. The component model gives one single-threaded
// instance per pool member, so a `thread_local` cell is the idiomatic
// per-instance state slot (a real adapter parks its parsed config here
// — see `plugins/rerank/llama/src/config.rs`). This sample has no
// config, but keeping the flag makes the lifecycle contract visible:
// the host MUST call `meta::init` before any rerank dispatch, and the
// adapter is entitled to rely on that.
thread_local! {
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

struct Component;

// ── wild:plugin-meta/meta — the Tier-2 lifecycle ────────────────────

impl MetaGuest for Component {
    /// Self-report of the plugin's identity. The host cross-checks
    /// every field against the sidecar `.json` written at
    /// `wild plugin add` time — a `slug`/`version` mismatch is a hard
    /// load error, so these values and `sidecar.json` move together.
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "overlap-rerank".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: Some(PluginKind::Provider),
            // Package-form coordinate — the rerank loader matches this
            // string exactly before it wires the adapter.
            provides: vec!["wild:rerank-adapter@0.1.0".to_string()],
            // The world inherits `wild:secrets/store` and
            // `wild:http/outbound` imports from the shared plugin
            // world, but this component never calls either —
            // wit-bindgen tree-shakes uncalled imports out of the
            // binary, so the honest requires-list is empty.
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    /// The host hands every adapter its per-entry config bundle (JSON).
    /// This sample declares no config keys, so the payload is ignored —
    /// a real adapter parses it and returns a typed `InitError`
    /// (`missing-config-key`, `backend-init`, …) so `wild doctor` can
    /// name the problem.
    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        INITIALIZED.with(|cell| cell.set(true));
        Ok(())
    }

    /// Best-effort teardown on `wild down` / plugin unload.
    fn shutdown() {
        INITIALIZED.with(|cell| cell.set(false));
    }
}

// ── wild:rerank-adapter/rerank — capability profile + the call ──────

impl RerankGuest for Component {
    /// Static capability profile. The host shim caches this at
    /// registration; adapters MUST return the same value every time
    /// within a process. `0` means unknown / unbounded — this scorer
    /// genuinely has no document or query-length cap.
    fn info() -> AdapterInfo {
        AdapterInfo {
            name: "overlap-rerank".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            provider_family: "other".to_string(),
            max_documents: 0,
            max_query_chars: 0,
        }
    }

    /// The single rerank call: score every document against the query,
    /// sort descending, honour `top-n`. Scores are provider-relative
    /// per the contract — here they happen to land in `[0, 1]`, but
    /// callers MUST NOT threshold them across adapters.
    fn rerank(request: RerankRequest) -> Result<RerankResponse, RerankError> {
        ensure_initialized()?;

        let query_tokens = tokenize(&request.query);
        let mut results: Vec<RerankResult> = request
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| RerankResult {
                index: index as u32,
                relevance_score: score(&query_tokens, &tokenize(document)),
            })
            .collect();

        // Contract: `results` is sorted by the adapter, best first.
        // Ties break on the lower index (the retrieve stage's order) so
        // the output is fully deterministic. Scores are finite, so
        // `total_cmp` is a plain descending sort.
        results.sort_by(|a, b| {
            b.relevance_score
                .total_cmp(&a.relevance_score)
                .then(a.index.cmp(&b.index))
        });

        // `top-n`: keep only the best N; `None` = score all candidates.
        if let Some(top_n) = request.top_n {
            results.truncate(top_n as usize);
        }

        Ok(RerankResponse {
            results,
            // Echo the request's model back — this adapter has no model
            // of its own, and the contract's `model` field reports what
            // actually served the request.
            model: request.model,
            // Local adapters report zeros (the WIT's own words); a
            // cloud adapter copies the provider's counters here.
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
        })
    }
}

// ── deterministic term-overlap scoring ──────────────────────────────

/// Rerank dispatched before `meta::init` is a host-lifecycle bug — say
/// so instead of answering from an uninitialized instance.
fn ensure_initialized() -> Result<(), RerankError> {
    if INITIALIZED.with(|cell| cell.get()) {
        Ok(())
    } else {
        Err(RerankError::Permanent(
            "overlap-rerank: rerank called before meta::init — the host's plugin \
             lifecycle should have initialized this instance first"
                .to_string(),
        ))
    }
}

/// Lowercased word set: split on any non-alphanumeric character
/// (Unicode-aware), drop empties. A set — not a bag — because Jaccard
/// is defined over sets; repeating a term in the query does not make a
/// document more relevant to this scorer.
fn tokenize(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

/// Jaccard index of the two token sets: `|Q ∩ D| / |Q ∪ D|` in
/// `[0, 1]`. Both sides empty ⇒ `0.0` (nothing overlaps with nothing).
fn score(query: &BTreeSet<String>, document: &BTreeSet<String>) -> f32 {
    let union = query.union(document).count();
    if union == 0 {
        return 0.0;
    }
    let intersection = query.intersection(document).count();
    intersection as f32 / union as f32
}

export!(Component);
