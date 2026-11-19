//! `sticky-note` — teaching sample for `wild:ui/widget@0.1.0` (ADR-0173).
//!
//! The smallest complete Tier-2 widget plugin: `describe()` advertises the
//! widget kind (`sticky-note`), and `render()` turns the view's `config`
//! (`title`, `text`, optional `color`) into an HTML fragment the carrier
//! places inside the view's card. The render is a pure, deterministic
//! function of its JSON inputs — no host imports, no clock, no randomness —
//! so the whole contract is exercised by plain `cargo test`.
//!
//! This widget is CONFIG-only: it ignores `data_json` (the host passes
//! `"null"` for a bind-less view). The data-bound path — a view whose `bind`
//! makes the host run a governed ui-query and pass the records in — is shown
//! by the reference plugin `plugins/widgets/hello-widget/` in the development
//! repository.

#![allow(clippy::all)]

wit_bindgen::generate!({
    path:  "wit",
    world: "sticky-note",
    generate_all,
});

use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::ui::widget::{Guest, WidgetError, WidgetMeta};
use serde::Deserialize;

/// The minimum Tier-2 lifecycle. The host's plugin loader cross-checks
/// `manifest()` against `sidecar.json` at load — slug/version/provides must
/// match, or the plugin is refused.
impl MetaGuest for StickyNote {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "sticky-note".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: Some(PluginKind::Provider),
            // Interface form, matching `sidecar.json` verbatim.
            provides: vec!["wild:ui/widget@0.1.0".to_string()],
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        Ok(())
    }

    fn shutdown() {}
}

/// The view's `config` blob, parsed against this widget's own schema (the
/// host never validates it — `config-schema` in `describe()` is for
/// authoring surfaces only).
#[derive(Deserialize, Default)]
struct NoteConfig {
    #[serde(default = "default_title")]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default = "default_color")]
    color: String,
}

fn default_title() -> String {
    "Note".to_string()
}

fn default_color() -> String {
    // The DEFAULT accent is the app's own token, not a colour of this
    // widget's choosing: an operator who set a brand colour sees it here
    // too, and the card follows the light/dark theme. A `config.color`
    // overrides it, so the knob keeps working. (Same rule the reference
    // widget follows — a widget never hardcodes its own palette.)
    "var(--wv-accent)".to_string()
}

struct StickyNote;

impl Guest for StickyNote {
    fn describe() -> WidgetMeta {
        WidgetMeta {
            kind: "sticky-note".to_string(),
            label: "Sticky Note".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            config_schema: r##"{
  "type": "object",
  "properties": {
    "title": { "type": "string", "default": "Note" },
    "text":  { "type": "string", "default": "" },
    "color": { "type": "string", "default": "var(--wv-accent)" }
  },
  "additionalProperties": false
}"##
            .to_string(),
        }
    }

    fn render(view_json: String, _data_json: String) -> Result<String, WidgetError> {
        let view: serde_json::Value = serde_json::from_str(&view_json)
            .map_err(|e| WidgetError::InvalidConfig(format!("view json is not valid: {e}")))?;

        let config: NoteConfig = match view.get("config") {
            Some(cfg) => serde_json::from_value(cfg.clone())
                .map_err(|e| WidgetError::InvalidConfig(format!("config malformed: {e}")))?,
            None => NoteConfig::default(),
        };

        // Everything interpolated into the fragment is operator input —
        // escape it ALL, the CSS colour value included. The carrier injects
        // this fragment as innerHTML (ADR-0173 D2), so an unescaped `<`
        // would become live markup.
        let safe_title = html_escape(&config.title);
        let safe_text = html_escape(&config.text);
        let safe_color = html_escape(&config.color);

        // Layout + type come from the renderer's `--wv-*` tokens so the card
        // matches the app's theme and font scale; the one input-driven knob
        // is the accent edge (`config.color`). `white-space:pre-wrap` keeps
        // the operator's line breaks without letting them inject markup.
        Ok(format!(
            r#"<div class="sticky-note" style="padding:var(--wv-control-pad);border-radius:var(--wv-radius);background:var(--wv-shell);color:var(--wv-fg);border-left:4px solid {safe_color};">
  <h3 style="margin:0;font-size:var(--wv-text-sm);font-weight:600;">{safe_title}</h3>
  <p style="margin:var(--wv-space-2) 0 0;color:var(--wv-muted);font-size:var(--wv-text-sm);white-space:pre-wrap;">{safe_text}</p>
</div>"#
        ))
    }
}

/// Escape the minimal set of HTML characters so operator-supplied config
/// cannot break out of the card markup. The sample keeps it inline and
/// readable; a production widget should use a proper HTML-escaping crate
/// (or return a structured payload once a later rung defines one).
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

export!(StickyNote);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_advertises_sticky_note_kind() {
        let meta = StickyNote::describe();
        assert_eq!(meta.kind, "sticky-note");
        assert_eq!(meta.label, "Sticky Note");
        assert_eq!(meta.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn render_uses_defaults() {
        let html = StickyNote::render(
            r##"{"type":"custom","widget_kind":"sticky-note","config":{}}"##.to_string(),
            "null".to_string(),
        )
        .unwrap();
        assert!(html.contains(">Note</h3>"));
        assert!(html.contains("var(--wv-accent)"));
    }

    #[test]
    fn render_shows_title_text_and_color_from_config() {
        let html = StickyNote::render(
            r##"{"type":"custom","widget_kind":"sticky-note","config":{"title":"Reminder","text":"Call the tax office on Monday.","color":"#f59e0b"}}"##
                .to_string(),
            "null".to_string(),
        )
        .unwrap();
        assert!(html.contains(">Reminder</h3>"));
        assert!(html.contains("Call the tax office on Monday."));
        assert!(html.contains("border-left:4px solid #f59e0b"));
    }

    #[test]
    fn render_escapes_operator_input() {
        // The fragment is injected as innerHTML — a config value must never
        // become live markup. Title, text AND colour all pass the escape.
        let html = StickyNote::render(
            r##"{"config":{"title":"<script>alert(1)</script>","text":"a & b < c","color":"red\" onload=\"x"}}"##
                .to_string(),
            "null".to_string(),
        )
        .unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("a &amp; b &lt; c"));
        assert!(!html.contains(r#"red" onload"#));
        assert!(html.contains("red&quot; onload=&quot;x"));
    }

    #[test]
    fn render_is_deterministic() {
        let view = r##"{"config":{"title":"Same","text":"in, same out."}}"##;
        let a = StickyNote::render(view.to_string(), "null".to_string()).unwrap();
        let b = StickyNote::render(view.to_string(), "null".to_string()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn render_ignores_bound_data() {
        // Config-only by design: a view with a `bind` still renders, and the
        // records the host fetched never appear in the fragment.
        let html = StickyNote::render(
            r##"{"config":{"title":"Pinned"},"bind":{"source":"projection","projection":"debtor"}}"##
                .to_string(),
            r##"{"records":[{"partner":"Schmidt AG"}],"total":1}"##.to_string(),
        )
        .unwrap();
        assert!(html.contains(">Pinned</h3>"));
        assert!(!html.contains("Schmidt AG"));
    }

    #[test]
    fn render_rejects_invalid_json() {
        let err = StickyNote::render("not json".to_string(), "null".to_string()).unwrap_err();
        match err {
            WidgetError::InvalidConfig(_) => {}
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn render_rejects_malformed_config() {
        let err = StickyNote::render(
            r##"{"config":{"title":42}}"##.to_string(),
            "null".to_string(),
        )
        .unwrap_err();
        match err {
            WidgetError::InvalidConfig(_) => {}
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }
}
