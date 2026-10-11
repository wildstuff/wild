# Plugin trust — schema + CLI

How Wild decides **what an installed plugin may do**, and how you
widen or narrow that as an operator. Three pieces own it:

| Piece | What it owns |
|---|---|
| The **signature check** | Derives the trust tier from *which key* signed the plugin's sidecar (`wild_core::provenance`). |
| The **trust file** | `<wild_root>/plugin-trust.json` — your publisher allowlist plus per-plugin and per-tribe capability grants (`wild_plugins::trust`). |
| The **load-time gate** | Checks a plugin's required capabilities against its tier's allowance before the host accepts it (`plugin_trust_gate`, called from one shared entry point by both boot and live-reload). |

[`plugin-concept.md`](plugin-concept.md) carries the *why*. This doc
is the operator reference: how the tier is earned, the file shape,
the command flow, and what the loader does when a plugin comes up.

## The tier is derived, never declared

The `trust.tier` a publisher writes into a sidecar is a **claim**.
What counts is the detached **ed25519 signature** over the sidecar
bytes, checked against keys the publisher does not control
(ADR-0153):

```text
signature verifies against the key that SHIPS with Wild   → verified
signature verifies against a key YOU allowlisted          → community
no signature, or one that verifies against neither        → unknown
```

Two rules the implementation enforces by construction:

- **Nothing raises itself.** A tier token read off a sidecar can
  never become a tier — only a verifying key produces one.
- **You may lower, never raise.** An allowlist entry tops out at
  `community`; `verified` means "signed by the key that ships with
  this binary", and no operator entry can mint that. Lowering a
  publisher to `unknown` is yours to do at any time.

The signature covers the **sidecar**, and the sidecar's `source`
pins the component's manifest digest — so a verified signature is a
verified statement about the *only bytes that can run*. An unsigned
plugin still installs; it simply earns nothing (the `unknown`
allowance below).

## On-disk shape — `<wild_root>/plugin-trust.json`

**System-wide** (at the user root, not under a profile): trusting a
publisher's key is an operator-level decision, not a per-profile one.

```json
{
  "publishers": [
    {
      "name": "acme-gmbh",
      "key": "9f3c…64 hex characters…a1",
      "tier": "community",
      "registered_at": "2026-04-27T12:00:00Z"
    }
  ],
  "capability_overrides": [
    {
      "plugin_slug": "math-tools",
      "added_caps": ["wasi:http/outgoing-handler"],
      "granted_at": "2026-04-27T12:00:00Z",
      "granted_by": "wild plugin grant math-tools --cap wasi:http/outgoing-handler"
    }
  ],
  "tribe_grants": [],
  "requires_certified": []
}
```

- All arrays default to empty. Missing file → empty store (a fresh
  install runs without a manual `touch`). Empty file → empty store
  too. Unknown top-level fields are ignored (forward-compat).
- `publishers[].key` is the publisher's **ed25519 public key itself**
  — exactly 64 hex characters (32 bytes), validated as a real curve
  point at the write door. It is deliberately not a URL: a fetched
  key would be a second trust decision at the moment of use, and you
  made yours when you added the row.
- `publishers[].tier` accepts `community` or `unknown`. Writing
  `verified` is refused (see the raise/lower rule above), and the
  reserved `verified-local` token is rejected at parse and write
  time with a typed error so a forward-written file cannot silently
  downgrade on read.
- `capability_overrides[]` are per-plugin grants on top of the tier
  allowance; `granted_by` records the issuing command line so
  `wild plugin grants` can show how a grant got added.
- `tribe_grants[]` raises capabilities for an entire *tribe* rather
  than one plugin slug: a forged worker inherits the union of its
  tribe's grants, so only a capability outside that union triggers
  an escalation (ADR-0046). A tribe with no row inherits nothing.
- `requires_certified[]` lists capability qnames that demand a
  *certified* component: the gate refuses any component providing
  one of these unless it carries a live certification. Opt-in and
  operator-controlled — the default empty list changes nothing.
- The file is written mode 0600 via a temp file + atomic rename.

## Where the per-plugin tier lives

The per-plugin trust tier lives in the plugin's **sidecar** —
`<profile_root>/system/plugin-cache/<slug>.json` — which is the
authoritative record. `plugin-trust.json` does **not** mirror
per-plugin tiers; it holds operator state *on top of* install
metadata.

Every tier write goes through one funnel
(`PluginTrustWriter::update_tier`): the sidecar is rewritten first
(preserving every other field), then — for workload-flavor plugins
that have one — the per-plugin YAML entry under
`<profile_root>/system/component-types/` is mirrored. That order
guarantees a partial failure leaves the canonical state correct and
the secondary copy merely behind; a mirror failure surfaces as its
own error and the catalog catches up on the next write.

## Trust-tier allowance map

| Tier | Policy |
|---|---|
| `verified` | Full caps the manifest declares — the gate is a no-op. |
| `community` | Blocklist. Blocked: `wild:secrets/store`, `wild:secrets/admin`, `wild:bundles/admin`, the whole `wild:plugin-storage` package, `wild:data/store` (authored writes), `wild:data/schema` (schema writes), the whole `wild:effect` package (business-effect submission), and `wild:tools/invoke` (arbitrary tool dispatch). The read surfaces — `wild:data/query`, `/graph`, `/ontology`, `/types`, `wild:tools/catalog`, secrets *reads* — stay open. |
| `unknown` | Allowlist: `wild:blobs/read` and `wild:messaging/consumer` only. Raw `wasi:http/outgoing-handler` is **not** blanket-granted here (it is an ungoverned outbound socket — no egress allowlist, no per-call audit); an unknown-tier plugin that needs it gets a per-slug override, minted automatically by an operator-confirmed connector install (see below) or by hand via `wild plugin grant`. |

The Community blocklist cannot silently rot: a repo test enumerates
every host-served `wild:` package and fails the build until a new
package is explicitly triaged into blocked-or-open.

**Override stacking** is additive across all tiers — every
`capability_overrides` entry for the slug is treated as pre-allowed
regardless of tier. So an override can unblock a
Community-blocklisted cap (`wild:plugin-storage/store`) AND extend
an Unknown-tier allowlist with a single cap, with the same shape.

**Block matching:**

- Exact qname match: `wild:secrets/store` blocks the exact qname.
- Package-prefix match: `wild:plugin-storage` blocks every
  `wild:plugin-storage/*` interface. The `/` separator is required —
  `wild:plugin-storageish/whatever` does **not** match.

## Operator CLI flow

```sh
# Grant one or more caps to a plugin (repeatable --cap).
wild plugin grant math-tools --cap wasi:http/outgoing-handler
wild plugin grant math-tools --cap wasi:http/outgoing-handler --cap wild:secrets/get

# Secret grants route through the same verb (per-profile ACL).
wild plugin grant math-tools --secret openai-api-key

# Render every override (or filter by slug).
wild plugin grants
wild plugin grants --slug math-tools

# Drop a single cap from every entry that lists it; entries that
# held only the target cap are dropped entirely.
wild plugin revoke math-tools wasi:http/outgoing-handler

# Drop every override for a slug.
wild plugin revoke math-tools
```

The legacy positional form (`wild plugin grant math-tools
wasi:http/outgoing-handler`) still works; new scripts should prefer
the explicit `--cap` / `--secret` flags.

The gate reloads `<wild_root>/plugin-trust.json` on every check, so a
`grant` issued during a long-running `wild up` lands for the next
plugin instantiation — no host restart needed.

### Publisher allowlist

```sh
# Trust a publisher's signing key (64-hex ed25519 public key).
wild plugin publisher add acme-gmbh --key 9f3c…a1 --tier community

# Stop trusting one — anything they signed drops to `unknown`
# at its next load-time check.
wild plugin publisher rm acme-gmbh

# Show the publishers this machine trusts.
wild plugin publisher ls
```

You cannot add anyone at `verified` — that tier means "signed by
the key that ships with Wild". See
`plugin-cli.md` for the full verb reference and
`marketplace.md` for the signed marketplace index
(the same signature scheme, applied to the catalog).

### Connector-install auto-grant

An OCI-pulled connector lands at the `unknown` tier, but a
marketplace install is an explicit, operator-confirmed act — the
operator saw the offer (secrets, sign-in, egress) and said yes. So
when the confirmed install completes, the host mints a per-slug
override for exactly the raw egress cap the connector *declares* it
needs (`wasi:http/outgoing-handler`) and nothing else. The install
response echoes it under `granted_capabilities`. A connector that
declares no raw-http import gets no grant; a more powerful cap
(e.g. `wild:secrets/store`) is never auto-granted — it stays an
explicit `wild plugin grant`. This is what lets a deliberately
installed connector reach its API while the tier's *blanket* raw
egress stays closed to a plugin that merely appeared on disk.

## Load-time gate

One shared entry point checks every Tier-2 plugin before the host
accepts its registration — on the boot wiring path and again on
live reload (a sidecar change while the daemon runs). On a denial
the plugin is skipped (boot logs a warning; a reload surfaces the
error), and the message carries:

- the plugin slug and its derived tier,
- the first denied cap (short-circuit, so output stays readable),
- the full allowed set for the tier (override caps appended),
- the exact `wild plugin grant <slug> --cap <cap>` command that
  would unblock it.

So the recovery path is always in the error itself: read the denied
cap, decide whether the plugin *should* hold it, grant it or leave
it denied.
