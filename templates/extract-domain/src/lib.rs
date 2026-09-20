#![allow(clippy::all)]

wit_bindgen::generate!({ path: "wit", world: "extract-domain", generate_all });

use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};

struct Component;

/// Reduce a URL to its bare registered host.
///
/// Rules (from the spec):
/// - Strip the scheme (`scheme://`) if present.
/// - Strip everything from the first `/`, `?` or `#` onwards (path/query/fragment).
/// - Strip userinfo (`user:pass@`) if present.
/// - Strip a trailing `:port`.
/// - Strip a leading `www.` / `www2.` / `www3.` / `wwwN.` label
///   (`www` optionally followed by ASCII digits, then a literal `.`).
/// - If the result is empty, return the input unchanged (unparseable).
fn extract_registered_host(url: &str) -> String {
    match parse_host(url) {
        Some(host) if !host.is_empty() => strip_www_label(&host).to_string(),
        _ => url.to_string(),
    }
}

/// Return the authority-host slice of `url`, or None if we cannot see one.
fn parse_host(url: &str) -> Option<String> {
    let s = url.trim();
    if s.is_empty() {
        return None;
    }

    // Strip scheme: look for the FIRST `://`. A bare `not-a-url` has none.
    let after_scheme = match s.find("://") {
        Some(idx) => {
            // Validate the scheme is ASCII alpha + [a-z0-9+.-]; anything
            // else and we treat the whole input as unparseable.
            let scheme = &s[..idx];
            if scheme.is_empty() || !is_valid_scheme(scheme) {
                return None;
            }
            &s[idx + 3..]
        }
        None => return None, // no scheme -> not a parseable URL for this tool
    };

    if after_scheme.is_empty() {
        return None;
    }

    // Cut off path / query / fragment.
    let authority_end = after_scheme
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];

    if authority.is_empty() {
        return None;
    }

    // Strip userinfo `user:pass@` if present. Use rfind so a `@` inside
    // userinfo does not split the host later.
    let host_and_port = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };

    if host_and_port.is_empty() {
        return None;
    }

    // Strip trailing `:port`. Split on the LAST colon so an IPv6-like
    // authority (which would be bracketed) is not mangled by an internal
    // colon. The spec is narrow about named hosts; we treat any
    // colon-suffix as port-ish and drop it.
    let host = match host_and_port.rfind(':') {
        Some(colon) => &host_and_port[..colon],
        None => host_and_port,
    };

    if host.is_empty() {
        return None;
    }

    Some(host.to_string())
}

fn is_valid_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// If `host` begins with a `www` / `wwwN` label followed by `.`, drop it.
/// `wwwx.example.com` and `www` (no dot) are preserved.
fn strip_www_label(host: &str) -> &str {
    // Find the first `.`; the label is the slice before it.
    let dot = match host.find('.') {
        Some(i) => i,
        None => return host,
    };
    let (label, rest) = host.split_at(dot);
    // `rest` starts with `.`; the tail after the dot must be non-empty.
    if rest.len() <= 1 {
        return host;
    }
    if is_www_label(label) {
        &rest[1..]
    } else {
        host
    }
}

/// `www` optionally followed by one or more ASCII digits.
fn is_www_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    if &bytes[..3] != b"www" {
        return false;
    }
    if bytes.len() == 3 {
        return true;
    }
    bytes[3..].iter().all(|b| b.is_ascii_digit())
}

impl ToolsGuest for Component {
    fn list_tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "extract_domain".into(),
            description: "Reduce a URL to its bare registered host (e.g. `https://www.example.com/x?q=1` \u{2192} `example.com`); use when a caller has a URL and wants the site it belongs to. Examples: {\"url\":\"https://www.example.com/article/123?q=1\"} \u{2192} {\"domain\":\"example.com\"} {\"url\":\"http://www2.bbc.co.uk/news\"} \u{2192} {\"domain\":\"bbc.co.uk\"}".into(),
            json_schema: "{\"type\":\"object\",\"properties\":{\"url\":{\"type\":\"string\"}},\"required\":[\"url\"],\"additionalProperties\":false}".into(),
        }]
    }

    fn list_skill_mds() -> Vec<SkillMd> {
        vec![]
    }

    async fn invoke(name: String, args_json: String) -> Result<ToolResult, ToolError> {
        if name != "extract_domain" {
            return Err(ToolError::UnknownTool(name));
        }
        let args: serde_json::Value = serde_json::from_str(&args_json)
            .map_err(|e| ToolError::InvalidArgs(format!("args_json is not valid JSON: {e}")))?;
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArgs("missing required field `url` (string)".into())
        })?;
        let domain = extract_registered_host(url);
        let output = serde_json::json!({ "domain": domain }).to_string();
        Ok(ToolResult {
            json_output: output,
            cost_units: None,
        })
    }
}

impl MetaGuest for Component {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "extract-domain".into(),
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
    use super::*;

    #[test]
    fn strips_www_and_path_and_query() {
        assert_eq!(
            extract_registered_host("https://www.example.com/article/123?q=1"),
            "example.com"
        );
    }

    #[test]
    fn strips_numbered_www_prefix() {
        assert_eq!(
            extract_registered_host("http://www2.bbc.co.uk/news"),
            "bbc.co.uk"
        );
    }

    #[test]
    fn keeps_non_www_subdomain() {
        assert_eq!(
            extract_registered_host("https://api.github.com/repos"),
            "api.github.com"
        );
    }

    #[test]
    fn returns_unparseable_input_unchanged() {
        assert_eq!(extract_registered_host("not-a-url"), "not-a-url");
    }

    #[test]
    fn strips_port() {
        assert_eq!(
            extract_registered_host("https://example.com:8443/path"),
            "example.com"
        );
    }

    #[test]
    fn strips_userinfo() {
        assert_eq!(
            extract_registered_host("https://user:pass@www.example.com/x"),
            "example.com"
        );
    }

    #[test]
    fn strips_fragment_only() {
        assert_eq!(
            extract_registered_host("https://example.com#top"),
            "example.com"
        );
    }

    #[test]
    fn strips_query_only() {
        assert_eq!(
            extract_registered_host("https://example.com?a=b"),
            "example.com"
        );
    }

    #[test]
    fn preserves_wwwx_label() {
        // `wwwx` is not `www` + digits, so it is a normal subdomain.
        assert_eq!(
            extract_registered_host("https://wwwx.example.com/"),
            "wwwx.example.com"
        );
    }

    #[test]
    fn preserves_bare_www_without_dot() {
        // No dot after the label -> not a www prefix, whole host stays.
        assert_eq!(extract_registered_host("https://www"), "www");
    }

    #[test]
    fn strips_www_with_many_digits() {
        assert_eq!(
            extract_registered_host("https://www12.example.com/x"),
            "example.com"
        );
    }

    #[test]
    fn empty_string_is_unchanged() {
        assert_eq!(extract_registered_host(""), "");
    }

    #[test]
    fn no_scheme_returns_input_unchanged() {
        // Spec: unparseable -> return input unchanged. No `://` means we
        // do not know what scheme was meant, so we do not strip anything.
        assert_eq!(
            extract_registered_host("example.com/path"),
            "example.com/path"
        );
    }

    #[test]
    fn is_www_label_matrix() {
        assert!(is_www_label("www"));
        assert!(is_www_label("www1"));
        assert!(is_www_label("www42"));
        assert!(!is_www_label("wwwx"));
        assert!(!is_www_label("ww"));
        assert!(!is_www_label("wwww"));
        assert!(!is_www_label(""));
    }

    #[test]
    fn valid_scheme_matrix() {
        assert!(is_valid_scheme("http"));
        assert!(is_valid_scheme("https"));
        assert!(is_valid_scheme("ftp"));
        assert!(is_valid_scheme("h+t.p-s"));
        assert!(!is_valid_scheme(""));
        assert!(!is_valid_scheme("1http"));
        assert!(!is_valid_scheme("ht tp"));
    }
}
