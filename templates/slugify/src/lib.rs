#![allow(clippy::all)]

wit_bindgen::generate!({ path: "wit", world: "slugify", generate_all });

use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    FaultClass, FaultDetail, Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct SlugifyArgs {
    text: String,
}

#[derive(Serialize)]
struct SlugifyOut {
    slug: String,
}

/// Fold a single input `char` into the sequence of ASCII chars that
/// should replace it. German umlauts and eszett map per the spec;
/// ASCII alphanumerics pass through lowercased; every other char
/// becomes a single `-` (which the collapse step then absorbs).
fn fold_char(c: char) -> &'static [u8] {
    match c {
        'ä' => b"ae",
        'ö' => b"oe",
        'ü' => b"ue",
        'Ä' => b"ae",
        'Ö' => b"oe",
        'Ü' => b"ue",
        'ß' => b"ss",
        _ => &[],
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_hyphen = true; // true so a leading hyphen is suppressed
    for c in input.chars() {
        let folded = fold_char(c);
        if !folded.is_empty() {
            // German fold — always ASCII lowercase letters, safe to push.
            for &b in folded {
                out.push(b as char);
            }
            last_hyphen = false;
            continue;
        }
        // Lowercase (may yield multiple chars, e.g. some Unicode).
        for lc in c.to_lowercase() {
            if lc.is_ascii_alphanumeric() {
                out.push(lc);
                last_hyphen = false;
            } else {
                if !last_hyphen {
                    out.push('-');
                    last_hyphen = true;
                }
            }
        }
    }
    // Strip a single trailing hyphen (collapse guarantees at most one).
    if out.ends_with('-') {
        out.pop();
    }
    out
}

struct Component;

impl ToolsGuest for Component {
    fn list_tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "slugify".into(),
            description: "Convert an arbitrary string into a URL-safe kebab-case slug (lowercased, umlauts folded, non-alphanumerics to hyphens, collapsed, trimmed). Use when a caller needs a stable short identifier derived from a human-readable name. Examples: {\"text\": \"Hello World\"} → {\"slug\": \"hello-world\"} {\"text\": \"Grüße Über Ärger Straße\"} → {\"slug\": \"gruesse-ueber-aerger-strasse\"}".into(),
            json_schema: "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}},\"required\":[\"text\"],\"additionalProperties\":false}".into(),
        }]
    }
    fn list_skill_mds() -> Vec<SkillMd> {
        vec![SkillMd {
            slug: "slugify".into(),
            body: include_str!("../skills/slugify.md").into(),
        }]
    }
    async fn invoke(name: String, args_json: String) -> Result<ToolResult, ToolError> {
        if name != "slugify" {
            return Err(ToolError::UnknownTool(name));
        }
        let args: SlugifyArgs = serde_json::from_str(&args_json)
            .map_err(|e| ToolError::InvalidArgs(format!("invalid args: {e}")))?;
        let out = SlugifyOut {
            slug: slugify(&args.text),
        };
        let json_output = serde_json::to_string(&out).map_err(|e| {
            ToolError::Fault(FaultDetail {
                class: FaultClass::Unknown,
                message: format!("serialise output: {e}"),
            })
        })?;
        Ok(ToolResult {
            json_output,
            cost_units: None,
        })
    }
}

impl MetaGuest for Component {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "slugify".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Provider),
            provides: vec!["wild:tool-provider@0.4.0".into()],
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

export!(Component);

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn lowercase_and_space() {
        assert_eq!(slugify("Hello World"), "hello-world");
    }

    #[test]
    fn german_umlauts_folded() {
        assert_eq!(
            slugify("Grüße Über Ärger Straße"),
            "gruesse-ueber-aerger-strasse"
        );
    }

    #[test]
    fn special_chars_to_hyphens() {
        assert_eq!(slugify("foo@bar!baz.qux"), "foo-bar-baz-qux");
    }

    #[test]
    fn collapse_multiple_hyphens() {
        assert_eq!(slugify("foo---bar   baz"), "foo-bar-baz");
    }

    #[test]
    fn trim_leading_and_trailing() {
        assert_eq!(slugify("---Hello World---"), "hello-world");
    }

    #[test]
    fn empty_input() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn all_specials_becomes_empty() {
        assert_eq!(slugify("---   ---"), "");
    }

    #[test]
    fn standalone_eszett() {
        assert_eq!(slugify("ß"), "ss");
    }

    #[test]
    fn non_ascii_alphanumerics_drop() {
        // Non-ASCII letters (outside the German fold set) become hyphens —
        // "URL-safe" reads as ASCII-only per the spec.
        assert_eq!(slugify("café"), "caf");
    }
}
