---
name: "slugify"
version: 0.1.0
source: component
component_type: "slugify"
method: "slugify"
description: "Convert an arbitrary string into a URL-safe kebab-case slug (lowercased, umlauts folded, non-alphanumerics to hyphens, collapsed, trimmed). Use when a caller needs a stable short identifier derived from a human-readable name."
args_schema: {"additionalProperties":false,"properties":{"text":{"type":"string"}},"required":["text"],"type":"object"}
returns_schema: {"additionalProperties":false,"properties":{"slug":{"type":"string"}},"required":["slug"],"type":"object"}
---

# slugify

Convert an arbitrary string into a URL-safe kebab-case slug (lowercased, umlauts folded, non-alphanumerics to hyphens, collapsed, trimmed). Use when a caller needs a stable short identifier derived from a human-readable name.

## Examples

### lowercase-and-space

Input: {"text": "Hello World"}
Output: {"slug": "hello-world"}

### german-umlauts-folded

Input: {"text": "Grüße Über Ärger Straße"}
Output: {"slug": "gruesse-ueber-aerger-strasse"}

### special-chars-to-hyphens

Input: {"text": "foo@bar!baz.qux"}
Output: {"slug": "foo-bar-baz-qux"}

### collapse-multiple-hyphens

Input: {"text": "foo---bar   baz"}
Output: {"slug": "foo-bar-baz"}

### trim-leading-and-trailing

Input: {"text": "---Hello World---"}
Output: {"slug": "hello-world"}
