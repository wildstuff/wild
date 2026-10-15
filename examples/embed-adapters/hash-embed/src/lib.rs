//! `hash-embed` — deterministic teaching adapter for the Tier-2
//! `wild:embed-adapter@0.1.0` contract.
//!
//! No network, no secrets, no config. The "model" is a hashed
//! bag-of-words: lowercase, split on whitespace, hash every token into
//! one of `dims` buckets (FNV-1a, with one hash bit picking the sign),
//! l2-normalize. The vectors are real — unit-length `f32`, batch
//! index-aligned, deterministic across machines — so every surface of
//! the contract can be exercised end-to-end without an upstream
//! provider:
//!
//!   - `wild:plugin-meta/meta` — `manifest` / `init` / `shutdown`, the
//!     lifecycle the host's `ComponentBackedHostPlugin` shim drives on
//!     every Tier-2 plugin.
//!   - `wild:embed-adapter/embed` — `info()` (the capability profile)
//!     and the one-shot batch `embed()` call. No streaming sibling.
//!
//! The embedding is semantically NAIVE: two texts are close only when
//! they share literal words. That is the point — the *contract*
//! mechanics are the teaching value, not the vectors.
//!
//! To turn this into a real provider adapter: keep every signature and
//! replace ONE function — [`embed_text`] — with an HTTP round-trip to
//! the provider (the world's inherited `wild:http/outbound` import;
//! api-key via `wild:secrets/store`). `plugins/embed/ollama/` in the
//! development repository is the fully-worked version of that step.

use std::cell::Cell;

wit_bindgen::generate!({
    path:  "wit",
    world: "hash-embed",
    generate_all,
});

use exports::wild::embed_adapter::embed::{
    AdapterInfo, EmbedError, EmbedRequest, EmbedResponse, Guest as EmbedGuest, TokenUsage,
};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};

/// Native output dimensionality when the request doesn't ask for one.
/// Any `embed-request.dimensions >= 1` is honoured — hashing works at
/// every width — which is why `info()` reports `supports-dimensions:
/// true`.
const DEFAULT_DIMS: u32 = 256;

// Lifecycle marker. The component model gives one single-threaded
// instance per pool member, so a `thread_local` cell is the idiomatic
// per-instance state slot (a real adapter parks its parsed config
// here — see `plugins/embed/ollama/src/config.rs`). This sample has no
// config, but enforcing the flag makes the lifecycle contract visible:
// the host MUST call `meta::init` before any embed dispatch, and the
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
            slug: "hash-embed".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: Some(PluginKind::Provider),
            provides: vec!["wild:embed-adapter@0.1.0".to_string()],
            // The world inherits `wild:secrets/store` and
            // `wild:http/outbound` imports from the shared
            // embed-adapter-plugin world, but this component never
            // calls either — wit-bindgen tree-shakes uncalled imports
            // out of the binary, so the honest requires-list is empty.
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    /// The host hands every adapter its per-profile config bundle
    /// (JSON for the embed-adapter flavor). This sample declares no
    /// config keys, so the payload is ignored — a real adapter parses
    /// it and returns a typed `InitError` (`missing-config-key`,
    /// `backend-init`, …) so `wild doctor` can name the problem.
    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        INITIALIZED.with(|cell| cell.set(true));
        Ok(())
    }

    /// Best-effort teardown on `wild down` / plugin unload.
    fn shutdown() {
        INITIALIZED.with(|cell| cell.set(false));
    }
}

// ── wild:embed-adapter/embed — capability profile + the batch call ──

impl EmbedGuest for Component {
    /// Static capability profile. The host shim caches this once at
    /// registration instead of branching on the adapter slug. MUST
    /// return the same value every time within a process.
    fn info() -> AdapterInfo {
        AdapterInfo {
            name: "hash-embed".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            provider_family: "other".to_string(),
            // The width produced when the request doesn't pin one.
            max_dimensions: DEFAULT_DIMS,
            // Hashing works at any width, so `embed-request.dimensions`
            // is honoured exactly — a rare honest `true` (most real
            // models only support their native dim or Matryoshka
            // truncation).
            supports_dimensions: true,
            // No upstream, no batch cap.
            max_batch_size: 0,
        }
    }

    /// The single embed call. `embeddings` is index-aligned to
    /// `request.input`; every vector has length `dims` and unit l2
    /// norm.
    fn embed(request: EmbedRequest) -> Result<EmbedResponse, EmbedError> {
        if !INITIALIZED.with(|cell| cell.get()) {
            return Err(EmbedError::Permanent(
                "hash-embed: embed() called before meta::init — the host's \
                 ComponentBackedHostPlugin lifecycle should warm the pool first"
                    .to_string(),
            ));
        }
        if request.input.is_empty() {
            return Err(EmbedError::Permanent(
                "hash-embed: empty input list".to_string(),
            ));
        }
        let dims = request.dimensions.unwrap_or(DEFAULT_DIMS);
        if dims == 0 {
            // Malformed request → permanent, never retryable: retrying
            // the same bytes can't succeed (the two-bucket rule).
            return Err(EmbedError::Permanent(
                "hash-embed: `dimensions` must be >= 1".to_string(),
            ));
        }

        let mut embeddings = Vec::with_capacity(request.input.len());
        let mut total_tokens: u32 = 0;
        for text in &request.input {
            let (vector, tokens) = embed_text(text, dims as usize);
            embeddings.push(vector);
            total_tokens = total_tokens.saturating_add(tokens);
        }

        Ok(EmbedResponse {
            embeddings,
            // Echo the request's model back — the contract's rule when
            // the upstream did not reroute. The host fills this from
            // the adapter's `embed-adapters.yaml` entry.
            model: request.model,
            // This "model" genuinely knows its token count (one token
            // per whitespace word), so report it instead of zeros —
            // a real local adapter without usage data reports zeros.
            usage: TokenUsage {
                input_tokens: total_tokens,
                output_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
            dims,
        })
    }
}

// ── The "model" — replace exactly this to go real ───────────────────

/// Hashed bag-of-words: lowercase, split on whitespace, FNV-1a each
/// token, bucket = `hash % dims`, sign from the hash's top bit (the
/// signed hashing trick — keeps the expected component sum near zero
/// so common words don't all pile onto positive mass), l2-normalize.
///
/// Returns `(vector, token_count)`. The vector always has length
/// `dims` and unit norm — a text with no tokens (empty / whitespace)
/// gets a deterministic sentinel bucket so the output is never the
/// zero vector (conformance: at least one non-zero component).
///
/// A real adapter replaces this ONE function with the provider
/// round-trip: build the wire payload from the batch, POST it through
/// `wild:http/outbound::fetch`, decode `{embeddings: [[f32]]}` — see
/// `plugins/embed/ollama/src/ollama.rs`.
fn embed_text(text: &str, dims: usize) -> (Vec<f32>, u32) {
    let mut vector = vec![0.0f32; dims];
    let mut tokens: u32 = 0;
    for word in text.split_whitespace() {
        let hash = fnv1a64(word.to_lowercase().as_bytes());
        let bucket = (hash % dims as u64) as usize;
        let sign = if hash >> 63 == 0 { 1.0 } else { -1.0 };
        vector[bucket] += sign;
        tokens += 1;
    }
    if tokens == 0 {
        vector[0] = 1.0;
    }
    l2_normalize(&mut vector);
    (vector, tokens)
}

/// FNV-1a, 64-bit. Not cryptographic — stable, tiny, and good enough
/// spread for bucketing words.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x1000_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Scale to unit l2 norm. Callers guarantee at least one non-zero
/// component, so the norm is never zero; the guard is belt-and-braces
/// against a future refactor breaking that invariant.
fn l2_normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|c| c * c).sum::<f32>().sqrt();
    if norm > 0.0 {
        for c in vector.iter_mut() {
            *c /= norm;
        }
    }
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_normalized() {
        let (a, tokens) = embed_text("The quick brown Fox", 256);
        let (b, _) = embed_text("the QUICK brown fox", 256);
        assert_eq!(a, b, "case-insensitive and deterministic");
        assert_eq!(tokens, 4);
        assert_eq!(a.len(), 256);
        let norm: f32 = a.iter().map(|c| c * c).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "unit norm, got {norm}");
    }

    #[test]
    fn empty_text_is_not_the_zero_vector() {
        let (v, tokens) = embed_text("   ", 8);
        assert_eq!(tokens, 0);
        assert!(v.iter().any(|c| *c != 0.0));
    }

    #[test]
    fn overlap_beats_disjoint() {
        let dot = |x: &[f32], y: &[f32]| -> f32 {
            x.iter().zip(y).map(|(a, b)| a * b).sum()
        };
        let (fox, _) = embed_text("quick brown fox", 256);
        let (fox2, _) = embed_text("quick brown dog", 256);
        let (tax, _) = embed_text("invoice ledger accrual", 256);
        assert!(
            dot(&fox, &fox2) > dot(&fox, &tax),
            "shared words must score higher than disjoint ones"
        );
    }

    #[test]
    fn honours_requested_dimensions() {
        let (v, _) = embed_text("hello world", 32);
        assert_eq!(v.len(), 32);
    }
}
