//! `sharepoint-connector` — ADR-0141 PR1 existence proof.
//!
//! A single third-party Tier-2 plugin that exercises all four installable
//! compute primitives defined in ADR-0141:
//!
//!   - **tool-provider**   → `wild:tool-provider/tools` (`sharepoint-enumerate`,
//!                          `sharepoint-fetch`, plus the effect-family tools
//!                          `sharepoint-create-folder` and
//!                          `sharepoint-update-list-item`).
//!   - **worker**          → `wild:worker/handler` + `wild:worker/meta`.
//!   - **effect-handler**  → the same tools as the tool-provider, recognised as
//!                          effects via the sidecar `effect.sharepoint`
//!                          capability bundle.
//!   - **function-backing** → `wild:function/backing` (`sharepoint-resolve-user`,
//!                          `sharepoint-resolve-site`).
//!
//! The implementation is intentionally stubbed: every tool and backing returns
//! deterministic JSON so the plugin compiles and loads without a live
//! SharePoint tenant. A real connector would replace the stubs with HTTP calls
//! through the imported `wasi:http/outgoing-handler` and credential reads
//! through `wild:secrets/store`.

#![allow(clippy::all)]

wit_bindgen::generate!({
    path:  "wit",
    world: "sharepoint-connector",
    generate_all,
});

use exports::wild::function::backing::{BackingSpec, FunctionError, Guest as FunctionGuest};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};
use exports::wild::worker::handler::{BrokerMessage, Guest as HandlerGuest};
use exports::wild::worker::meta::{Guest as WorkerMetaGuest, TriggerSpec};

struct SharepointConnector;

// ── tool-provider primitive ──

impl ToolsGuest for SharepointConnector {
    fn list_tools() -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "sharepoint-enumerate".into(),
                description: "List SharePoint lists, libraries, or folders reachable under a site. Returns deterministic stub data for ADR-0141 PR1.".into(),
                json_schema: r#"{
  "type": "object",
  "properties": {
    "site": { "type": "string", "description": "Site path or URL" },
    "scope": { "type": "string", "enum": ["lists", "folders", "drive"], "description": "What to enumerate" }
  },
  "required": ["site"]
}"#.into(),
            },
            ToolSpec {
                name: "sharepoint-fetch".into(),
                description: "Fetch a SharePoint list item or file by ID. Returns deterministic stub data for ADR-0141 PR1.".into(),
                json_schema: r#"{
  "type": "object",
  "properties": {
    "site": { "type": "string" },
    "list": { "type": "string" },
    "id": { "type": "string" }
  },
  "required": ["site", "list", "id"]
}"#.into(),
            },
            ToolSpec {
                name: "sharepoint-create-folder".into(),
                description: "Create a folder in a SharePoint document library (effect-handler; gated). Returns deterministic stub data for ADR-0141 PR1.".into(),
                json_schema: r#"{
  "type": "object",
  "properties": {
    "site": { "type": "string" },
    "library": { "type": "string" },
    "path": { "type": "string" }
  },
  "required": ["site", "library", "path"]
}"#.into(),
            },
            ToolSpec {
                name: "sharepoint-update-list-item".into(),
                description: "Update a SharePoint list item (effect-handler; gated). Returns deterministic stub data for ADR-0141 PR1.".into(),
                json_schema: r#"{
  "type": "object",
  "properties": {
    "site": { "type": "string" },
    "list": { "type": "string" },
    "id": { "type": "string" },
    "fields": { "type": "object" }
  },
  "required": ["site", "list", "id", "fields"]
}"#.into(),
            },
        ]
    }

    fn list_skill_mds() -> Vec<SkillMd> {
        // No bundled skill MDs for this existence proof.
        vec![]
    }

    #[allow(clippy::unused_async)]
    async fn invoke(name: String, _args_json: String) -> Result<ToolResult, ToolError> {
        let json_output = match name.as_str() {
            "sharepoint-enumerate" => {
                r#"{"stub":true,"items":[{"id":"1","name":"Documents"},{"id":"2","name":"Shared"}]}"#
            }
            "sharepoint-fetch" => r#"{"stub":true,"id":"42","title":"Sample Item","fields":{}}"#,
            "sharepoint-create-folder" => {
                r#"{"stub":true,"created":true,"folder_id":"folder-123"}"#
            }
            "sharepoint-update-list-item" => r#"{"stub":true,"updated":true,"item_id":"item-456"}"#,
            other => return Err(ToolError::UnknownTool(other.into())),
        };
        Ok(ToolResult {
            json_output: json_output.into(),
            cost_units: None,
        })
    }
}

// ── worker primitive ──

impl HandlerGuest for SharepointConnector {
    fn handle_message(_msg: BrokerMessage) -> Result<(), String> {
        // Stub: a real worker would decode the envelope, poll SharePoint for
        // changes, and publish derived events via wild:messaging/consumer.
        Ok(())
    }
}

impl WorkerMetaGuest for SharepointConnector {
    fn list_triggers() -> Vec<TriggerSpec> {
        vec![TriggerSpec {
            name: "sharepoint-change-feed".into(),
            description: "React to SharePoint change notifications or poll for delta updates."
                .into(),
            default_subject: Some("wild.{tribe}.source.sharepoint.changed".into()),
        }]
    }
}

// ── function-backing primitive ──

impl FunctionGuest for SharepointConnector {
    fn list_backings() -> Vec<BackingSpec> {
        vec![
            BackingSpec {
                name: "sharepoint-resolve-user".into(),
                description: "Resolve an email address to a SharePoint user ID. Returns deterministic stub data for ADR-0141 PR1.".into(),
                input_schema: r#"{"type":"object","properties":{"email":{"type":"string"}},"required":["email"]}"#.into(),
                output_schema: r#"{"type":"object","properties":{"user_id":{"type":"string"},"display_name":{"type":"string"}},"required":["user_id"]}"#.into(),
            },
            BackingSpec {
                name: "sharepoint-resolve-site".into(),
                description: "Resolve a site name to a SharePoint site ID. Returns deterministic stub data for ADR-0141 PR1.".into(),
                input_schema: r#"{"type":"object","properties":{"site_name":{"type":"string"}},"required":["site_name"]}"#.into(),
                output_schema: r#"{"type":"object","properties":{"site_id":{"type":"string"},"url":{"type":"string"}},"required":["site_id"]}"#.into(),
            },
        ]
    }

    fn invoke(name: String, _args_json: String) -> Result<String, FunctionError> {
        match name.as_str() {
            "sharepoint-resolve-user" => {
                Ok(r#"{"user_id":"user-123","display_name":"Ada Lovelace"}"#.into())
            }
            "sharepoint-resolve-site" => Ok(
                r#"{"site_id":"site-789","url":"https://example.sharepoint.com/sites/acme"}"#
                    .into(),
            ),
            other => Err(FunctionError::UnknownBacking(other.into())),
        }
    }
}

// ── plugin-meta lifecycle ──

impl MetaGuest for SharepointConnector {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "sharepoint-connector".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Provider),
            provides: vec![
                "wild:tool-provider/tools@0.4.0".into(),
                "wild:worker/handler@0.1.0".into(),
                "wild:worker/meta@0.1.0".into(),
                "wild:function/backing@0.1.0".into(),
            ],
            requires: vec![
                "wild:secrets/store@0.1.0".into(),
                "wasi:http/outgoing-handler@0.2.0".into(),
                "wild:messaging/consumer@0.3.0".into(),
            ],
            config_keys: vec!["tenant".into(), "site_url".into()],
            secret_aliases: vec!["sharepoint-client-secret".into()],
            signatures: vec![],
        }
    }

    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        // Stub: a real connector would parse its profile config here.
        Ok(())
    }

    fn shutdown() {
        // No-op for this stub.
    }
}

export!(SharepointConnector);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_lists_all_four_primitives() {
        let m = SharepointConnector::manifest();
        assert_eq!(m.slug, "sharepoint-connector");
        assert!(matches!(m.kind, Some(PluginKind::Provider)));
        assert!(m
            .provides
            .iter()
            .any(|p| p.starts_with("wild:tool-provider/")));
        assert!(m
            .provides
            .iter()
            .any(|p| p.starts_with("wild:worker/handler")));
        assert!(m.provides.iter().any(|p| p.starts_with("wild:worker/meta")));
        assert!(m
            .provides
            .iter()
            .any(|p| p.starts_with("wild:function/backing")));
    }

    #[test]
    fn tool_provider_lists_four_tools() {
        let tools = SharepointConnector::list_tools();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"sharepoint-enumerate"));
        assert!(names.contains(&"sharepoint-fetch"));
        assert!(names.contains(&"sharepoint-create-folder"));
        assert!(names.contains(&"sharepoint-update-list-item"));
    }

    #[test]
    fn invoke_enumerate_returns_stub_json() {
        let result = <SharepointConnector as ToolsGuest>::invoke(
            "sharepoint-enumerate".into(),
            r#"{"site":"acme"}"#.into(),
        );
        let result = poll_once(result);
        assert!(result.is_ok(), "{result:?}");
        let result = result.unwrap();
        assert!(result.json_output.contains("\"stub\":true"));
        assert!(result.json_output.contains("Documents"));
    }

    #[test]
    fn invoke_unknown_tool_errors() {
        let result = <SharepointConnector as ToolsGuest>::invoke(
            "sharepoint-destroy-everything".into(),
            r#"{}"#.into(),
        );
        let result = poll_once(result);
        assert!(matches!(result, Err(ToolError::UnknownTool(_))));
    }

    #[test]
    fn worker_handler_accepts_message() {
        let result = SharepointConnector::handle_message(BrokerMessage {
            subject: "wild.acme.source.sharepoint.changed".into(),
            body: b"{}".to_vec(),
            headers: vec![],
        });
        assert!(result.is_ok());
    }

    #[test]
    fn worker_meta_advertises_change_feed_trigger() {
        let triggers = SharepointConnector::list_triggers();
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].name, "sharepoint-change-feed");
    }

    #[test]
    fn function_backing_lists_two_backings() {
        let backings = SharepointConnector::list_backings();
        let names: Vec<_> = backings.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"sharepoint-resolve-user"));
        assert!(names.contains(&"sharepoint-resolve-site"));
    }

    #[test]
    fn invoke_resolve_user_returns_stub_json() {
        let result = <SharepointConnector as FunctionGuest>::invoke(
            "sharepoint-resolve-user".into(),
            r#"{"email":"ada@example.com"}"#.into(),
        );
        assert!(result.is_ok(), "{result:?}");
        assert!(result.unwrap().contains("Ada Lovelace"));
    }

    #[test]
    fn invoke_unknown_backing_errors() {
        let result = <SharepointConnector as FunctionGuest>::invoke(
            "sharepoint-resolve-group".into(),
            r#"{}"#.into(),
        );
        assert!(matches!(result, Err(FunctionError::UnknownBacking(_))));
    }

    /// Synchronously resolve one poll of the async tool invoke binding.
    /// All stub tools are sync, so one poll is enough.
    fn poll_once<T>(fut: impl std::future::Future<Output = T>) -> T {
        use std::pin::Pin;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake};

        struct NoopWaker;
        impl Wake for NoopWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Arc::new(NoopWaker).into();
        let mut cx = Context::from_waker(&waker);
        let mut fut = Pin::from(Box::new(fut));
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("stub tool invoke should resolve in one poll"),
        }
    }
}
