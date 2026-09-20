//! `echo-llm` — deterministic teaching adapter for the Tier-2
//! `wild:llm-adapter@0.3.0` contract.
//!
//! No network, no secrets, no config. The "completion" is derived from
//! the request itself — the last user message echoed back with a
//! deterministic token estimate — so every surface of the contract can
//! be exercised end-to-end without an upstream provider:
//!
//!   - `wild:plugin-meta/meta` — `manifest` / `init` / `shutdown`, the
//!     lifecycle the host's `ComponentBackedHostPlugin` shim drives on
//!     every Tier-2 plugin.
//!   - `wild:llm-adapter/chat` — `info()` (the capability profile) and
//!     the buffered `chat()` call.
//!   - `wild:llm-adapter/chat-stream` — the poll-cursor streaming
//!     resource. Retained for contract shape; the host no longer
//!     drains it (it drives only the native path below), so like the
//!     shipping adapters this sample serves it as a handle over
//!     pre-built chunks.
//!   - `wild:llm-adapter/chat-stream-native` — the native
//!     component-model-async token stream (`stream<token-chunk>` +
//!     `future<chat-response>`) the host actually drives. For an echo
//!     the reply is known up front, so "streaming" is simply writing
//!     the reply word-by-word — which is exactly what makes the
//!     mechanics readable.
//!
//! To turn this into a real provider adapter: keep every signature,
//! replace the reply derivation with an HTTP call (see the world's
//! inherited `wasi:http/outgoing-handler` import, and read the api-key
//! via `wild:secrets/store`), and decode the provider's wire format
//! into `chat-response` / `token-chunk`. `plugins/llm/openai/` in the
//! development repository is the fully-worked version of that step.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

wit_bindgen::generate!({
    path:  "wit",
    world: "echo-llm",
    generate_all,
});

use exports::wild::llm_adapter::chat::{
    AdapterInfo, ChatError, ChatRequest, ChatResponse, ContentBlock, Guest as ChatGuest,
    StopReason, TokenUsage,
};
use exports::wild::llm_adapter::chat_stream::{
    ChatStreamHandle, Guest as ChatStreamGuest, GuestChatStreamHandle, TokenChunk,
};
use exports::wild::llm_adapter::chat_stream_native::Guest as ChatStreamNativeGuest;
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};

// Lifecycle marker. The component model gives one single-threaded
// instance per pool member, so a `thread_local` cell is the idiomatic
// per-instance state slot (a real adapter parks its parsed config
// here — see `plugins/llm/openai/src/config.rs`). This sample has no
// config, but keeping the flag makes the lifecycle contract visible:
// the host MUST call `meta::init` before any chat dispatch, and the
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
            slug: "echo-llm".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: Some(PluginKind::Provider),
            provides: vec!["wild:llm-adapter@0.3.0".to_string()],
            // The world inherits `wild:secrets/store` and `wasi:http/
            // outgoing-handler` imports from the shared streaming
            // world, but this component never calls either —
            // wit-bindgen tree-shakes uncalled imports out of the
            // binary, so the honest requires-list is empty.
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    /// The host hands every adapter its per-profile config bundle
    /// (JSON for the LLM-adapter flavor). This sample declares no
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

// ── wild:llm-adapter/chat — capability profile + buffered call ──────

impl ChatGuest for Component {
    /// Static capability profile. Host callers read this once at
    /// registration instead of branching on the adapter slug. Every
    /// `false` here is a promise: this adapter streams, and does
    /// nothing else.
    fn info() -> AdapterInfo {
        AdapterInfo {
            name: "echo-llm".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            provider_family: "other".to_string(),
            supports_streaming: true,
            supports_session_affinity: false,
            supports_tool_use_native: false,
            supports_prompt_caching: false,
            max_cache_breakpoints: 0,
            supported_cache_ttls: vec![],
            supports_thinking_blocks: false,
            understood_feature_prefixes: vec![],
            // ADR-0308 D2 — an echo adapter has no upstream and no
            // model, so it has no window to report. `none` is the
            // honest answer and the one the reference implementation
            // should model for anyone copying this file.
            context_window: None,
        }
    }

    /// The buffered one-shot call. Derives the reply from the request
    /// — deterministic, so the same request always produces the same
    /// response, usage numbers included.
    fn chat(request: ChatRequest) -> Result<ChatResponse, ChatError> {
        ensure_initialized()?;
        Ok(build_response(&request))
    }
}

// ── wild:llm-adapter/chat-stream — poll-cursor resource ─────────────
//
// The contract-shape streaming surface: the caller gets an opaque
// resource and polls `next-chunk` until `ok(none)`. The host shim no
// longer drains this path (it drives `chat-stream-native` below), so
// the shipping adapters serve it as a buffered replay; this sample
// pre-builds the per-word chunks and hands them out one per poll,
// which keeps the resource semantics (chunk / none / dead) visible.

pub struct EchoStreamHandle {
    chunks: RefCell<VecDeque<TokenChunk>>,
}

impl ChatStreamGuest for Component {
    type ChatStreamHandle = EchoStreamHandle;

    fn chat_streaming(request: ChatRequest) -> Result<ChatStreamHandle, ChatError> {
        ensure_initialized()?;
        let response = build_response(&request);
        Ok(ChatStreamHandle::new(EchoStreamHandle {
            chunks: RefCell::new(delta_chunks(&response).into()),
        }))
    }
}

impl GuestChatStreamHandle for EchoStreamHandle {
    fn next_chunk(&self) -> Result<Option<TokenChunk>, ChatError> {
        Ok(self.chunks.borrow_mut().pop_front())
    }

    fn cancel(&self) {
        self.chunks.borrow_mut().clear();
    }
}

// ── wild:llm-adapter/chat-stream-native — the path the host drives ──

impl ChatStreamNativeGuest for Component {
    /// Native component-model-async streaming: return a live
    /// `stream<token-chunk>` plus a `future<chat-response>` that
    /// resolves once the stream completes. The host drains both
    /// concurrently. A real adapter's spawned task reads upstream
    /// bytes (SSE, subprocess stdout, …) and decodes as it writes;
    /// here the chunks are known up front, so the task just writes
    /// them in order and then resolves the terminal response.
    async fn chat_streaming_native(
        request: ChatRequest,
    ) -> Result<
        (
            wit_bindgen::StreamReader<TokenChunk>,
            wit_bindgen::FutureReader<ChatResponse>,
        ),
        ChatError,
    > {
        ensure_initialized()?;
        let response = build_response(&request);
        let chunks = delta_chunks(&response);

        let (mut chunk_writer, chunk_reader) = wit_stream::new::<TokenChunk>();
        // The future's constructor supplies the terminal response the
        // host reads if the writer is dropped unwritten (cancel /
        // producer death) — the host treats that shape as incomplete.
        let (response_writer, response_reader) =
            wit_future::new::<ChatResponse>(cancelled_response);
        wit_bindgen::spawn_local(async move {
            for chunk in chunks {
                let _ = chunk_writer.write_all(vec![chunk]).await;
            }
            // Dropping the writer closes the token stream; the host's
            // drain loop then awaits the terminal response future.
            drop(chunk_writer);
            let _ = response_writer.write(response).await;
        });
        Ok((chunk_reader, response_reader))
    }
}

// ── deterministic reply derivation ──────────────────────────────────

/// Chat dispatched before `meta::init` is a host-lifecycle bug — say
/// so instead of answering from an uninitialized instance.
fn ensure_initialized() -> Result<(), ChatError> {
    if INITIALIZED.with(|cell| cell.get()) {
        Ok(())
    } else {
        Err(ChatError::Permanent(
            "echo-llm: chat called before meta::init — the host's plugin \
             lifecycle should have initialized this instance first"
                .to_string(),
        ))
    }
}

/// Text of the most recent user-role message: text blocks joined,
/// tool-result blocks included (they ride on user turns), thinking /
/// tool-use / image blocks skipped.
fn last_user_text(request: &ChatRequest) -> String {
    request
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| {
            message
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text(text) => Some(text.text.as_str()),
                    ContentBlock::ToolResult(result) => Some(result.content.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Crude-but-deterministic token estimate (≈ 4 chars per token, min 1).
/// A real adapter reports the upstream's actual counters.
fn estimate_tokens(text: &str) -> u32 {
    (text.len() as u32 / 4).max(1)
}

/// The whole "model": echo the last user message. Everything else in
/// the response demonstrates which fields the contract expects filled.
fn build_response(request: &ChatRequest) -> ChatResponse {
    let user_text = last_user_text(request);
    let reply = if user_text.is_empty() {
        "echo: (no user text in the request)".to_string()
    } else {
        format!("echo: {user_text}")
    };
    let input_text_len: usize = request.system.len()
        + request
            .messages
            .iter()
            .flat_map(|message| message.content.iter())
            .map(|block| match block {
                ContentBlock::Text(text) => text.text.len(),
                ContentBlock::Thinking(thinking) => thinking.text.len(),
                ContentBlock::ToolUse(call) => call.arguments.len(),
                ContentBlock::ToolResult(result) => result.content.len(),
                ContentBlock::Image(_) => 0,
            })
            .sum::<usize>();
    ChatResponse {
        content: Some(reply.clone()),
        tool_calls: vec![],
        usage: TokenUsage {
            input_tokens: estimate_tokens(&"x".repeat(input_text_len)),
            output_tokens: estimate_tokens(&reply),
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        },
        // Echo the request's model back — the contract's rule when the
        // upstream did not reroute.
        model: request.model.clone(),
        // Always set: the host treats `none` as end-turn for
        // compatibility, but a missing stop-reason is a bug.
        stop_reason: Some(StopReason::EndTurn),
        // Stateless adapter: no session, no stream-level timing.
        session_id: None,
        latency: None,
    }
}

/// Split a finished response into streaming chunks: one text-delta per
/// word, then the terminal chunk carrying usage / model / stop-reason.
fn delta_chunks(response: &ChatResponse) -> Vec<TokenChunk> {
    let text = response.content.clone().unwrap_or_default();
    let mut chunks: Vec<TokenChunk> = text
        .split_inclusive(' ')
        .map(|word| TokenChunk {
            text_delta: Some(word.to_string()),
            thinking_delta: None,
            tool_calls: vec![],
            is_final: false,
            usage: None,
            model: None,
            stop_reason: None,
            session_id: None,
        })
        .collect();
    chunks.push(TokenChunk {
        text_delta: None,
        thinking_delta: None,
        tool_calls: vec![],
        is_final: true,
        usage: Some(response.usage),
        model: Some(response.model.clone()),
        stop_reason: response.stop_reason.clone(),
        session_id: None,
    });
    chunks
}

/// Terminal response when the native stream is cancelled before the
/// producer task resolved the future.
fn cancelled_response() -> ChatResponse {
    ChatResponse {
        content: None,
        tool_calls: vec![],
        usage: TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        },
        model: "echo-llm".to_string(),
        stop_reason: Some(StopReason::Other("cancelled".to_string())),
        session_id: None,
        latency: None,
    }
}

export!(Component);
