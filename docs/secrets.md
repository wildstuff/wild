# wild:secrets — Design + status

Status: **Shipped.** The critical path (P6.1, P6.3–P6.5, P6.8) is
implemented and tested — `wild:secrets@0.1.0` WIT, the
`SecretsBackend` chain, the full `wild secret` CLI, ACL
enforcement, audit events, and the encrypted-file backend all
exist. The lean-CLI split (ADR-0036) moved the engine into a
standalone `crates/runtime/wild-secrets` crate; the host plugin
(`wild_secrets_plugin.rs`) is now the thin auth adapter that links
it. ADR-0059 added a required-secrets activation gate on top (see
§ "Required-secrets gate" below). Still pending: the
`wild-secrets-component` helper crate (P6.7, DX-only). This doc
keeps the original design rationale and marks where the shipped
code diverges from it.

Phase 7 (Tier-2 component adapters) is the immediate consumer and
is **no longer blocked** — its secrets precondition is met. See
`wild/docs/llm-adapters.md` "Adapter delivery — two tiers" for the
cross-cutting context.

## Why

Components today have three ways to get an API key, all bad:

1. **Hardcoded in the WASM** — leaks via `wild component add` OCI
   image inspection. Non-starter for any real key.
2. **Env var passthrough via `wasi:cli/environment`** — works for
   Tier-1 native adapters that we audit (`OPENROUTER_API_KEY`),
   but means the wild process has every key in its env. A 3rd-
   party adapter (Phase 7) would inherit them all, which is
   exactly what we want to avoid.
3. **`wasi:keyvalue` against a file** — too generic. The component
   could enumerate keys, observe missing ones, etc. Wrong shape.

What we need: a **scoped secrets surface** where a component
declares which named secrets it wants, the user explicitly grants
each one, and the host enforces both at every read. The user's
secrets stay in the OS keychain, never in process memory longer
than one read, and every access lands an audit event.

This is the precondition for Phase 7 (Tier-2 OCI adapters).
Without it, the only way to ship a 3rd-party adapter is to
hardcode the key — meaning we can't ship 3rd-party adapters at
all.

## Scope

In scope:
- `wild:secrets@0.1.0` WIT contract — `get(name) -> result<string,
  secret-error>`. Read-only from the component side.
- `SecretsBackend` Rust trait + backend implementations
  (one cross-platform `KeychainBackend` via the `keyring` crate,
  `EnvBackend`, `EncryptedFileBackend`).
- `wild secret {add,list,show,remove,rotate}` CLI for managing
  values.
- `wild secret {grant,revoke,grants}` CLI for managing the
  component → secret ACL.
- `<profile_root>/system/secret-grants.json` ACL persistence.
- Host-side enforcement: every `get(name)` checks the ACL before
  hitting the backend.
- Audit events on `wild.{tribe}.system.secrets.access` —
  component_id + alias + result, **never the value**.

Out of scope (separate phases / docs):
- **Per-tribe scoped secrets** — Phase 6.x extension. Today
  every secret is wild-installation-wide. A multi-tenant deploy
  may want org-scoped namespaces; the WIT surface stays
  compatible because the host resolves `name` against the calling
  component's org context.
- **Secret-rotation events on a NATS subject** — components today
  re-read on every call. A push-based "your secret was rotated"
  invalidation channel is a Phase 6.x nice-to-have for components
  that legitimately cache (none planned).
- **Hardware-backed keys (TPM, Secure Enclave passkeys)** — out
  of scope; the OS keychain backend already uses these
  transitively on Mac/Win.
- **Secret sharing across wild installations** (e.g. a team-
  shared keystore) — Phase 8+, probably an external secrets-
  manager bridge (`vault://`, `aws-secrets-manager://`).

## Non-goals

- **Component-side write/delete/list** — read-only is the whole
  trust model. A write is always a host-side act on the operator's
  behalf, never something a guest component can perform.
- **Implicit grants** — a component does NOT auto-receive grants
  for "secrets it asks for". The user MUST grant explicitly
  (`wild plugin grant`, or a yes to the Security & Keys form's named
  question). `wild adapter add` proposes grants interactively
  but doesn't apply them silently.
- **Anonymous secrets / capability tokens** — every secret has a
  user-chosen name. No `wild secret get-temp-token` or similar.
- **Per-call secret injection from the trigger payload** — would
  let any caller override what the component reads. Secrets come
  from the keychain, period.

## Credentials that end up in a chat

A secret can reach the host without ever going near this service:
you paste an API key into a chat to ask what it is, a tool error
echoes an `Authorization` header, or the model quotes either back
inside its own reasoning. Every chat turn comes to rest in at least
three places: the Denkverlauf trace under `tribes/<slug>/chat-traces/`,
the session transcript in the `domain-elder-chat` KV, and the
`WILD_USER_MESSAGES` stream, which keeps every reply, question and
`autonomy` notification sent to the operator for an hour (under the
profile's `jetstream/`). Anything in that text used to come to rest
on disk in the clear.

These three run a redaction pass (`wild_core::redact`) at the last
point before the text is stored. They are the places covered so far,
not every place a turn's text reaches:

- the trace and transcript writers, inherited by both the operator
  and the customer plane;
- the reply envelope writer (`wild_chat::reply_wire::reply_body`) and
  the chief's own reply, question and notification publishes, before
  they reach the stream.

One line still reaches its operator whole: a released pairing code.
The daemon holds it in memory and pushes it only to the window that
owns the conversation; the stream keeps the redacted copy.


**The bus itself (#10985).** Every other message a stream keeps is
redacted on its way out of the process that publishes it: what the
operator types (`…user.{channel}.message`, #10983), both ways of the
customer plane's `WILD_CUSTOMER_MESSAGES` (#10984), and a plugin that
publishes on a reply subject itself. Our `async-nats` fork runs a
filter on every publish; `wild_ipc::outbound::arm()` installs ours in
every binary that links the bus client (gate `every binary arms the
outbound filter`). The filter asks `wild_core::stream_table::keeps`
which stream would keep the subject and what it keeps: a chat stream
takes the text rule, the operative streams (effects, config, data
events, the forge lane) take key shapes only, a subject no stream
binds — and every KV bucket — goes out as given. Identity headers stand;
a body that is not UTF-8 is scanned as text and kept byte for byte
unless the scan finds a key, then it becomes `{"unreadable": …,
"bytes": n}`. Boot health's **bus** row publishes a made-up key and
reads back what left, so a daemon whose filter does not rewrite says
so (WARN, never a refused boot).

**Not covered:** a value a tool call carries: a tool receives its
arguments as the model wrote them (#12953). The fault stream and the
daemon log hold what they were handed, and a measured run found both
quoting tool input (#12954). These are stored as written.

**The model's own copy.** The claude-cli adapter hands the model the
operator's sentence as typed — a pasted key included — and the
`claude` program it spawns keeps no copy of that turn on disk
(`--no-session-persistence`, #11909): nothing under
`~/.claude/projects/`, which sits outside the profile and which neither
a profile reset nor a profile-wide check would see. The turn lives in
the helper process's memory and goes with it. Session files that
adapter versions before 1.10.9 wrote there are not removed.

What the redaction removes:

| Shape | Example |
|---|---|
| Vendor-prefixed keys | `sk-…` · `ghp_…` · `AKIA…` · `xoxb-…` · `xapp-…` · `AIza…` · `ya29.…` · `glpat-…` · `sk_live_…` · `whsec_…` · `npm_…` · `hf_…` · `gsk_…` |
| JWTs | `eyJ….….…` |
| PEM private-key blocks | `-----BEGIN … PRIVATE KEY-----` |
| Bearer tokens | `Bearer <token>` |
| Labelled assignments | `api_key=…` · `Kennwort=…` · `token:…` · `Passwort: geheim123` · `DB_PASSWORD=…` · `"password":"…"` |

Each becomes `[redacted:<kind>]`, so the record stays readable and
the operator can see that something was removed. A name of a secret
stays as written: `secret: brave-api-key` and `SECRET-REF: m365-mail`
name a secret, they do not hold one (ADR-0397).

**A key quoted back without its name.** When you type
`key=FAKE-KEY-0000` and the Elder's reply repeats only
`FAKE-KEY-0000`, nothing in the reply names the value. So each turn
remembers the values the table above took out of what you typed in
that turn, including anything you added while it ran. The same
turn's reply loses them too: in the trace, in the transcript and on
the reply stream. You see `[redacted:credential]` where the final
reply quoted the key. This applies only within that one turn. A
later turn's reply is read by the table alone, so an ordinary word
that happens to equal an earlier key stays as written
(`wild_chat::turn_redaction`, #12343). While a reply is still
streaming in, the chat window shows the model's words as they
arrive, a quoted key included, until the final reply replaces them.

> **What this does NOT do.** A password made of ordinary words is
> not detectable — it looks exactly like the prose around it, and a
> rule loose enough to catch it would redact the sentence it sits
> in. Redaction is defence-in-depth over the shapes that are
> directly exploitable by whoever reads the file, never a guarantee
> that a persisted record is secret-free. The right move for a
> secret is still to put it here, with `wild secret add`, and never
> into a chat.

### What is promised

Text that looks like a credential (a known key shape, or a value a
name declares a credential) is kept nowhere readable by this system,
whichever door it came through. Nothing below that line is promised.

That sentence is the goal the open work above is closing, and it holds
today only for the stores this section names as covered. Where a line
below speaks of tool arguments, the stores in #12954 or the operator's
own line, it describes #12953, #12954 and #12955, not the running
system.

| Road | The model sees | Kept |
|---|---|---|
| Elder chat | the key, for that turn | a marker |
| A tribe's own chat | a marker | a marker |
| Customer chat | a marker | a marker |
| Anything a model says or calls, on any road | | a marker in every kept copy; the live frames are not kept and are not redacted |

**Limits.** Each of these stands, and none is a bug to report:

- A bare password with no name beside it, a kebab-case passphrase under
  a name (`correct-horse-battery-staple`), or a key typed in an earlier
  resumed CLI session.
- Encoded or split text.
- A hostile plugin, and a plugin's private storage.
- The model sees a key typed into the Elder road, and the provider's
  copy is beyond this system. Live frames on screen are not redacted.
  `model_client_llm_delegate` returns a client model's text to that same
  client and keeps nothing.
- Mail already sent, `/save`, operator exports, and grants the host
  issues (a download token).
- Old copies: nothing goes looking for them.
- An address handed to a tool stays whole. Its stored locator is cut by
  `source_locator`.
- A literal `auth_header` in `channels.yaml`, which is operator
  configuration, not chat.
- A stale `wild` binary, and a client outside these binaries that holds
  the bus token.
- A name no rule declares a credential: `{"token":"abc123"}` has no
  known shape and stands.
- Lake internals, forge workspaces and KV buckets, until a measured run
  reaches them.
- A model quoting a key as evidence no longer matches the raw ask, so a
  quote check says no.
- A key pasted into a config field that names no secret arrives as a
  marker. Nothing leaks, the thing does not work, and a tool that is not
  staged says nothing before it writes.
- A store whose job is the credential: the secrets themselves, a pairing
  file at 0600.

### What a marker does where it lands

A marker is `[redacted:<kind>]` standing where a value was. It never
works as the value, and it fails closed: a marker in a channel's
`inbound_secret` resolves as a missing secret, so that webhook answers
503 to every caller, and a marker as an egress login never authenticates.
Five things make that legible:

1. **A staged call says so.** When the arguments of any staged call hold
   a marker, the row the operator answers says a value was withheld and
   only a placeholder stands in its place, and the chat's own line about
   what waits says so too. This holds for every tool and carries no list
   of them.
2. **A field that names a secret refuses at once.** See
   [Reference doors](#reference-doors--where-a-name-is-bound).
3. **The secret store refuses one as a value.** Every door that writes a
   value (`wild secret add`, `wild secret rotate`, the Keys form, the LLM
   setup, a governed provisioning) refuses a value that holds a marker,
   before the write, and names `wild secret add <name>` as the way the key
   itself is stored. Stored, it would be a secret that opens nothing.
4. **The reader is told what to do.** A reply whose pairing code was
   withheld says where a pairing is made (the dashboard's Settings →
   Connections, or `wild ingest pair`), and the chief, meeting a marker in
   what the operator wrote, points to `wild secret add`.
5. **Everything else stands.** In a location or in free configuration
   the marker stays, the thing does not work, and the marker shows why.
   In free text it stands too, and it can reach a customer that way with
   no confirmation. The customer never gets the key.

## The WIT contract

```wit
// wit/secrets/secrets.wit

package wild:secrets@0.1.0;

interface store {
    /// Read a named secret. Component receives the value as a
    /// transient string — no implementation hint about persistent
    /// storage on the component side. Components MUST NOT cache
    /// the value across calls; rotation works because every read
    /// hits the host, the host hits the keychain backend.
    ///
    /// `name` is the component-side alias (e.g. "api-key", "org-id"),
    /// NOT the keychain entry name. The host resolves the alias
    /// through the per-component map declared in the adapter's
    /// `<profile_root>/llm-adapters.yaml` entry (or for non-adapter
    /// components, in
    /// `<profile_root>/system/plugin-cache/<slug>.json`).
    get: func(name: string) -> result<string, secret-error>;

    variant secret-error {
        /// The aliased keychain entry doesn't exist. Either the
        /// user removed it via `wild secret remove`, or never ran
        /// `wild secret add` for that name in the first place.
        not-found,
        /// The component asked for an alias that isn't granted to
        /// it in `<profile_root>/system/secret-grants.json`. Logged as a
        /// security event before the error returns.
        access-denied,
        /// The keychain backend itself failed (locked keychain,
        /// daemon down, decrypt-error on EncryptedFileBackend).
        /// Carries the raw cause string.
        backend-unavailable(string),
    }
}

world secrets-host {
    export store;
}
```

**Why `get` only, not `try-get` + `must-get`:** every error case
is structurally distinct (`secret-error`), so a typed `result` +
match is enough. Adding sugar variants gives the component three
ways to do the same thing.

**Why no `list`:** even listing names is information leakage. A
malicious component asking "do you have a `slack-token`?" learns
something about the user's setup. Tighten down.

## Storage backends — `SecretsBackend` trait + chained resolver

> **Shipped layout (ADR-0036 lean split).** The trait, backends,
> resolver, grant store, audit sink and side-index all live in the
> standalone `crates/runtime/wild-secrets` crate (modules
> `backends.rs`, `grants.rs`, `grants_check.rs`, `audit.rs`,
> `index.rs`) so the lean CLI can run every `wild secret` command
> without linking the Wasm host. `wild_secrets_plugin.rs` in
> `wild-host` re-exports from that crate and adds only the
> WIT-boundary `Host::get` wiring. The trait is **synchronous** in the
> shipped code (keychain reads are fast, ~ms) and exposes
> `id` / `writable` / `get` / `put` / `remove`, mirroring the
> `ChatBackend` / `TasksBackend` shape. `get` returning `Ok(None)`
> means "this backend doesn't have it" (the resolver continues to the
> next backend); `Err` is a backend-side failure.

### Available backends

**Shipped: three backend types, not five.** The original design
listed a separate Rust type per OS keystore. The implementation
collapsed those into **one** cross-platform `KeychainBackend`
backed by the `keyring` v3 crate, which dispatches per-OS via
cargo features — `apple-native` (macOS), `sync-secret-service` +
`crypto-rust` (Linux), `windows-native` (Windows). So the keystore
is one Rust type, three platform backends underneath. Backend ids
recognised by the resolver are exactly `keychain` / `env` / `file`
(`backends.rs`). `vault` / `1password` appear as *reserved* ids in
`docs/config-vars.md` but are **not implemented** — an unknown id
in the order is dropped with a WARN.

| Backend (`id`) | Storage | Writable | Notes |
|---|---|---|---|
| `KeychainBackend` (`keychain`) | OS keychain via `keyring` v3, scoped service `"wild"` | yes | macOS Keychain / Linux Secret Service (D-Bus) / Windows Credential Manager, picked by cargo feature. On macOS the first read shows a TouchID prompt; subsequent reads ~ms. In Keychain Access each entry displays as **`Wild - <name>`** (label stamped on write; the `(service="wild", account=name)` lookup key is unchanged, and pre-existing entries pick the label up on their next write). On Windows the same string is the credential's TARGET NAME, not a label: `Entry::new` would compose `<name>.wild` and scatter the entries across Credential Manager's alphabet, so the target is set explicitly and the vault groups them under `Wild - ` exactly as Keychain Access does. Windows credentials are DPAPI-protected per user and written `CRED_PERSIST_ENTERPRISE` by `keyring`, so on a domain desk with roaming profiles they follow the user between machines. The OS keychain has no enumerate API → `wild secret list` reads a side-index (see below) **and** probes every name Wild declares, because the index is per profile while this backend is installation-wide (#13420). |
| `EnvBackend` (`env`) | none — env-passthrough only | **no** | Always available. Maps `secret-name` → env var (uppercased, dashes→underscores; `my-key` → `MY_KEY`, pinned by `EnvBackend::env_var_name`). Read-only by design. |
| `EncryptedFileBackend` (`file`) | `$WILD_HOME/system/secrets.enc` (override `WILD_SECRETS_FILE_PATH`), AES-256-GCM, master key from `WILD_SECRETS_KEY` (64-hex) else the generated `system/secrets.key` sibling, atomic writes, mode 0600. Every read answers from the file and every write re-reads it under `secrets.enc.lock`, so a key set from the CLI is the key a running daemon reads next — no restart (#10616) | yes | Joins the chain whenever `file` is in the order and a key SOURCE is named — the DEFAULT leader on any host without an OS credential store. The key itself is produced on first use (ADR-0287 D7); a key that can be neither read nor generated surfaces then, as a backend error the chain soft-fails past. Dropped with a WARN at boot only when `WILD_SECRETS_KEY` is set and malformed. |

> **Not the same store as OAuth refresh tokens — but the same choice.**
> ADR-0143's OAuth broker keeps its rotating **refresh tokens** in a
> *separate* host-managed store (`wild_oauth::TokenStore`), deliberately
> NOT part of the resolver chain and with no guest read path.
> Machine-managed/rotating credentials are kept apart from the
> operator-provisioned, immutable-from-guest secrets above so the
> `store::get(alias)` contract stays clean (a guest can never `get` a
> refresh token, and a rotation never looks like an operator edit).
> WHERE that store lives follows the same order as the chain
> (ADR-0379 D2, `TokenStore::for_backend_order`): with `keychain` leading
> and reachable, each sign-in is its own keychain entry under service
> `wild-oauth` (account `oauth-<profile>-<digest>/<binding>`), a service
> the guest chain (`wild`) never resolves against; with `file` leading,
> `$WILD_HOME/system/oauth-tokens.enc`, encrypted with the master key
> below; with an unreachable keychain leading, nowhere — every sign-in
> is refused with the reason. A sign-in an older build left in
> `oauth-tokens.enc` moves into the keychain on its first read. The
> only `wild:secrets` value OAuth stores in the chain is the static
> `client_secret`.

> **An unreachable keychain refuses, it does not fall through
> (ADR-0379 D1).** On macOS the keychain is the data-protection
> keychain, reachable only by a signed, provisioned `Wild.app`. A build
> that cannot address it — a tree build, a walk, a worktree daemon —
> keeps the `keychain` slot as `UnreachableKeychain`: every write is
> refused with the cause and the way to choose the file deliberately
> (`WILD_SECRETS_BACKEND_ORDER=file,env`); every read answers with the
> same reason, so the chain walks on to the backends behind it and a
> secret an earlier run stored there stays readable. Before ADR-0379 the
> slot was dropped (ADR-0287 B2) and the next writable backend — the
> file — silently took the write, with its master key in
> `system/secrets.key` beside it.
>
> **And it is said before anyone tries to save (ADR-0379 D3).** At every
> start the daemon reads the built chain's standing
> (`ChainedResolver::standing`) and publishes it as a fault of the
> installation: tribe `system`, source `secret-store`, the `Keys` row,
> with the remedy row `secret-store-unreachable` (writes refused) or
> `secret-store-partly-unreachable` (the keychain stands behind the store
> new keys go to). A sound store clears it. The Keys form's save is
> refused up front with a 503 carrying that `remedy_key`, which the
> dashboard says in the operator's language; a tribe's own `Keys` row
> names the installation's fix instead of "store X" while writes are
> refused.

### The chained resolver

A `ChainedResolver` wraps `Vec<Arc<dyn SecretsBackend>>` and is what
the wild:secrets plugin actually dispatches against. `get(name)`
walks the chain in declared order and returns the **first hit**:

```rust
fn get(&self, name: &str) -> Result<Option<String>, BackendError> {
    for backend in &self.chain {
        match backend.get(name) {
            Ok(Some(v)) => return Ok(Some(v)),
            Ok(None) => continue,                  // try next
            Err(e) => {
                tracing::warn!(backend = %backend.id(), error = %e,
                    "secrets: backend errored mid-chain; trying next");
                continue;
            }
        }
    }
    Ok(None)
}
```

The walk is **synchronous and allowed to block** — `KeychainBackend`
calls the OS keychain through `keyring`, which on macOS is a blocking
FFI call into the Security framework that waits indefinitely when the OS
wants a user confirmation a daemon cannot give.

That is why the host's `Host::get` (`wild-secrets-plugin`) binds its WIT
import **async** and runs this walk on `tokio::task::spawn_blocking`.
The WIT is unchanged — the component still calls a plain `get: func` —
but the guest's fiber now suspends across the walk instead of pinning a
runtime worker inside a single `poll`.

**It is not an optimisation, it is the only thing that keeps a guest call
bounded.** A blocking call in a *sync* host function blocks the guest
inside one `poll`, and every bound the host has lives downstream of a
poll that returns: `tokio::time::timeout` polls the future it wraps
before its own timer, so the timer is never polled; and the wasm epoch
fence cannot fire either, because a guest parked in a host call is not
executing wasm. Issue #3284 was exactly this — an LLM turn hung for 65
minutes with no `audit.egress` row, because the adapter reads its API key
*here*, before it opens the socket. **Any new host function that can
block must do the same** — enforced by the `host-fn blocking` pre-push
gate.

### The 20-second backend fence

The walk is bounded (`SECRET_GET_TIMEOUT`, 20 s). A healthy read is
milliseconds; the one way it takes real time is a keychain waiting on a
confirmation dialog no daemon can answer. On expiry the component gets
`backend-unavailable` carrying the cause **and the way out**:

> `no secrets backend answered within 20s — if the OS keychain is in the
> chain it may be waiting on a confirmation dialog that a daemon cannot
> answer. Unlock the login keychain, or move this secret to a backend that
> never prompts (`wild secret backend`, e.g. file or env)`

That message is the point. Without it a wedged keychain surfaced 240 s
later as the LLM shim's generic "the call ran out of budget" — true, and
useless to the operator (ADR-0116: faults surface as operator-legible
outcomes, not as a timeout somewhere up the stack).

The fence is only *reachable* because `get` is async and the walk is on
the blocking pool — around a sync host call, a timeout can never fire.
It releases the **guest**, not the thread: a blocking FFI call has no
cancellation point, so the pool thread stays parked until the OS returns.
The daemon stays alive and legible; the thread is not reclaimed.

`put(name, value)` writes to the **first writable backend** in the
chain:

```rust
async fn put(&self, name: &str, value: &str) -> Result<(), BackendError> {
    for backend in &self.chain {
        if backend.writable() {
            return backend.put(name, value).await;
        }
    }
    Err(BackendError::NoWritableBackend)
}
```

`remove(name)` removes from **every backend that has it** —
otherwise a "shadowed" entry in a later backend would be silently
exposed after the user thought they deleted the secret.

### Default chain order

The default is **per-platform**, because the chain's job is to have a
backend that can actually hold a value on the host it is running on:

```
macOS / Windows:  Keychain (OS) → EncryptedFile → Env
everywhere else:  EncryptedFile → Env
```

Reasoning:
- **Keychain first where there IS one.** Strongest storage,
  OS-managed, and what a user expects for "I added a secret". It
  keeps the LEAD on a desktop, so a write still lands there — `put`
  goes to the first *writable* backend, and nothing about where an
  operator's secrets live changes.
- **EncryptedFile behind it on a desktop, and not to store secrets.**
  The chain's only writer on a desktop is the keychain: an unreachable
  one refuses rather than handing its writes to the file (ADR-0379 D1),
  and OAuth sign-ins follow the keychain too (ADR-0379 D2). The file
  stays in the order for what was written there before — a key an
  earlier development run stored stays readable — and so the master
  key is still named. History: with the older `keychain,env` a desktop
  daemon booted `oauth_token_store=false`, because refresh tokens could
  only live in `oauth-tokens.enc` and that needs the master key;
  every `authorization_code` binding degraded to `ConsentRequired`.
  Since ADR-0379 a keychain-led order keeps sign-ins without the file.
- **EncryptedFile alone where there is no keychain.** On Linux,
  `KeychainBackend` is
  the D-Bus Secret Service — present on a desktop, absent on a server.
  It reports `writable() == true` and then fails every write, so
  `keychain,env` on a headless box is a chain whose only writable
  member cannot write: `wild secret add` had nowhere to put a value,
  and the way out was two env vars. The file store needs nothing
  installed and always works.
- **Env last** — for the "this is already in CI's env, just pass it
  through" case. Last so a stored entry wins over a same-named env
  var (useful when a user wants to override the CI-injected key
  locally).

A Linux **desktop** with a working keyring is the case this trades
away, and it is one variable back: `WILD_SECRETS_BACKEND_ORDER=`
`keychain,file,env`. The reverse is not available to a server at all,
which is why the server case takes the default.

#### The file backend's master key

`WILD_SECRETS_KEY` (64 hex chars) still wins and is the right answer
when the key belongs outside the profile — an injected env var, a
mounted secret. Absent it, and only when the chain actually lists
`file`, the key is generated once into `<profile>/system/secrets.key`:
`0600`, beside the store it opens, the same lifecycle as the profile's
`system/token`. Regeneration never happens — that would orphan every
secret already encrypted under the old key.

**When** it is fetched is its own fact (ADR-0287 D7). The config names
the source; the key is produced by the first byte that actually needs
encrypting. A presence probe, a removal, and a read for a name the
store does not hold all answer without one — so a profile that has
never written a file-backed secret never mints a key at all. It used
to be resolved while the config was parsed, which on macOS meant the
daemon met the login keychain twice before its control socket was
armed, for a store that measurement found empty on every profile.

Be clear about what that protects. The key sits next to the ciphertext,
so the pair is exactly as private as the profile directory: it defends
a stolen backup, a synced folder, a copied disk image — **not** an
attacker who can already read the profile as its owner. That trade is
deliberate: a server operator who cannot store a secret at all is worse
off than one whose key is protected by the same file permissions as the
rest of their runtime.

### Override

`WILD_SECRETS_BACKEND_ORDER=<csv>` env var swaps the order, drops
backends, or pins a single one for ops control:

```bash
# CI-only: env-passthrough, nothing else
WILD_SECRETS_BACKEND_ORDER=env

# Headless server: file first, env as last-resort
WILD_SECRETS_BACKEND_ORDER=file,env

# Linux desktop with a working keyring: OS store first, file as backup
WILD_SECRETS_BACKEND_ORDER=keychain,file,env

# macOS/Windows development build (no keychain entitlement): choose the
# file store on purpose — the default order would refuse every write
WILD_SECRETS_BACKEND_ORDER=file,env
```

Unknown ids in the list are ignored with a WARN-log.

### Mixing — the win

The chained model lets a user do this without touching code or
config:

```bash
# Personal API key — strongest storage, OS-keychain
wild secret add my-openrouter-key
> Enter value: sk-or-v1-...

# Team-shared bot token — comes from the deploy environment via env
export SLACK_BOT_TOKEN=xoxb-...

# Both names appear in `wild secret list` — the list asks the chain,
# not just the profile's index, so the source-backend column shows
# where each value lives.
wild secret list
NAME                  BACKEND
my-openrouter-key     keychain
slack-bot-token       env
```

Components don't see the difference — `secrets::get(alias)` works
the same way regardless of which backend held the value.

## Naming + aliasing

Two-name model. Component-side names are stable across deploys
(the component code references `"api-key"` forever); user-side
names are whatever the user picked when `wild secret add`-ing
(`"my-openrouter-personal-key"` vs `"team-openrouter-shared"`).
The mapping is per-component config:

```yaml
# <profile_root>/llm-adapters.yaml — adapter case
- id: awesome
  kind: component
  oci_image: ghcr.io/3rd/awesome:0.1.0
  secrets:
    api-key: my-openrouter-personal-key   # alias : keychain-name
    org-id:  my-openrouter-org

# <profile_root>/system/plugin-cache/<slug>.json — non-adapter custom component
{
  "id": "my-slack-poster",
  "oci_image": "...",
  "secrets": {
    "slack-token": "team-slack-bot-token"
  }
}
```

At component instantiation, the host builds a per-store
`SecretsCtx { component_id, alias_to_keychain_name: HashMap<String, String> }`.
Every `secrets::get(alias)` call:
1. Looks up `alias` in the alias map → keychain name.
2. Checks `<profile_root>/system/secret-grants.json` — does
   `component_id` have a grant on this keychain name?
3. Reads the backend.
4. Publishes the audit event.
5. Returns the value (or the typed error variant).

## ACL — the grants file

**Shipped path is profile-aware** (`GrantStore::default_path`):
`<profile_root>/system/secret-grants.json`, with a
legacy fallback to `<profile_root>/system/secret-grants.json` when no profile is
active (tests/tooling). The `wild-secrets` `GrantStore` does atomic
writes via `tempfile::persist`. Source of truth for "who can read
what". Hand-editable but the CLI is the safe path:

```json
{
  "grants": [
    {
      "component_id": "awesome",
      "secret_name": "my-openrouter-personal-key",
      "granted_at_ms": 1714234567890,
      "granted_via": "wild secret grant awesome my-openrouter-personal-key"
    },
    {
      "component_id": "awesome",
      "secret_name": "my-openrouter-org",
      "granted_at_ms": 1714234567891,
      "granted_via": "wild adapter add ghcr.io/... --secret org-id=my-openrouter-org"
    }
  ]
}
```

Why a separate file from `llm-adapters.yaml` / `secrets`
backend storage:
- yaml is shareable (no values, just names + aliases) — safe to
  commit to a repo. Grants ARE deployment-specific.
- Backend storage has the values; this file has the ACL. Two
  concerns, two files.

**A grant follows what was checked, not only the slug** (#9575). The
first time an LLM, embed or rerank adapter loads under a grant, the grant
records the source the trust check admitted it from (`bound_to`:
`oci:<repository>` for a pulled image, `local` for the operator's own
build). An adapter that later loads from another source under the same
slug does not inherit the key: its grants are withdrawn and the operator
gets an inbox row naming the adapter and the key, asking them to grant it
again. The binding is the source, not the digest: a change of repository,
or a switch between a local build and a pulled image, asks again; a
version bump from the same repository does not (that is the routine
update path, and asking on every update would teach the operator to click
through).

`wild secret grants` renders this as a table. `wild secret revoke`
filters it. Bulk-revoke per component (`wild secret revoke awesome`)
removes every row matching that component_id.

**A grants file that cannot be read** (#9410; the same for
`oauth-grants.json`). Every write also writes `<file>.last-good`. When a
door opens a file that does not parse, it sets the file aside as
`<file>.unreadable-<ms>` (kept, never deleted) and restores the list from
the last good copy: the state after the last write Wild made, revokes
included. Two cases bring back an older state: a grant revoked by
editing the file by hand after that write, and a crash between a write of
the list and the write of its copy, which leaves the copy one write behind
(after a revoke, still holding the revoked grant). With no readable copy, the list starts empty and every grant has
to be given again. Either way the operator gets one inbox row saying which
it was; the CLI grant verbs also print the sentence. The host plugin's
per-call re-read never repairs the file: while it cannot be read, nothing
is granted. A daemon whose boot could not read the file is bound to the
real path all the same, so a repaired file counts without a restart.

## Entry doors — where a value comes in

A secret value is typed by a human and written host-side. Three doors do
that today, and they differ in **who names the secret**:

| Door | Names the secret | Reach |
|---|---|---|
| `POST /api/v1/secrets` (dashboard § Security & Keys) | the **operator** | any name they choose |
| `POST …/plugins/{slug}/settings/{key}/secret` (ADR-0196) | a plugin's `settings:` schema | only a declared `type: secret` field |
| `POST …/provisioning-items/{subject}/invoke` (ADR-0146) | a projected `ProvisioningItem` | only an unmet readiness item |
| `wild secret add <name>` (CLI) | the operator | any name — the engineer path |

The last three are **derived**: they render and accept only for a name
some declaration produced, and both REST ones fail closed otherwise
(`` `AUTO` is not a declared setting for this plugin ``). That is correct
for what they are — it is what makes a typo'd subject an immediate error
rather than a stray keychain entry — but it left "store a secret called
`AUTO`" with no surface at all until the operator door existed, and the
only answer an assistant could give was "run this in a terminal".

The operator door normalises the typed name to the stored form by the
same ADR-0059 rule aliases use (`AUTO` → `auto`, `PORTAL_PASSWORT` →
`portal-passwort`) and returns the stored name, so the mapping is
reported rather than silent.

Each write through the operator door (and each grant through
`POST /api/v1/secrets/{name}/grant`) is logged once at INFO
(`provisioning.secret-stored` / `provisioning.secret-granted`) with the
sender's identity as the request carries it: `actor` (the principal the
bearer resolved to), `client` (the declared `User-Agent`, `undeclared` when
absent; the desktop app sends `wild-dashboard/<release>`) and `peer` (the TCP
peer, or `local-socket`). The line used to say `via="dashboard"` whoever sent
the write, so a key nobody typed could not be traced to its sender (#9462).
`client` is self-declared: it tells the app from `curl` or a script, not an
impostor holding the token from the app. The side-index `added_via` column
still names the door (`dashboard`), not the sender.

The dashboard's credential fields (one shared `SecretInput`) ask the webview
not to fill them (`autocomplete="new-password"`) and hand a value to their
form only after the operator touched the field: a key press, a paste or a
drop. Text that arrives without one is cleared and never reaches a save. The
case that made this a rule: macOS AutoFill filled the operator's Brave key from
their Passwords app into the welcome wizard's field on a first run, and the
step stored it (#9462).

### One name, whichever door (#7307)

The rule itself is `wild_core::secret_name::canonical_secret_name` —
trim, lowercase, `_` → `-`. It sits in `wild-core` because that is the
only layer every writer reaches, and it has one body: `keychain_name_for_alias`,
the REST door, the intake-readiness slot and the connector status report all
delegate to it.

Four doors write a secret, and they used to disagree. The read path asks the
backend for a stored name **verbatim**, so a key written `my_key` through the
terminal is simply not there when a binding that declared `my-key` looks for
it — a silent absence, not an error.

- **Creating** a secret refuses a non-canonical spelling and names the one that
  works (`secret name \`my_key\` is not the stored form — use \`my-key\``).
  That covers `wild secret add`, the provisioning writer, and — through the
  shared upsert — the LLM setup and adapter doors.
- **Acting on a stored** secret does not. `show`, `remove`, `rotate`, `grant`
  and `revoke` accept the spelling a name was stored under, or the fix would
  put every pre-existing `my_key` beyond its operator's reach.
- **`wild secret list`** marks those entries `(old spelling)` and names the
  canonical form to migrate to, so the remaining ones are visible rather than
  merely tolerated.

The charset check (`[a-z0-9_-]`, which still admits `_`) is a separate
question and stays where it was: a name can be canonical and invalid (`a.b`),
and a name can be valid and non-canonical (`my_key`). Where the canonical form
would ALSO fail the charset, no suggestion is offered — proposing a second
name that fails too is worse than one clear error.

Two rules on that door, both because it writes **installation-wide**:

- **Replacing is explicit.** A name that already resolves answers `409`;
  the caller confirms with `replace: true`. Re-typing a rotated key is an
  expected path, but so is a collision — normalisation means `PORTAL_PW`
  and `portal-pw` are one name — and an overwritten credential cannot be
  recovered. A probe the store could not answer refuses too, with `503`:
  the chain that could not be read is the chain the value would go into,
  and `put` lands in the first WRITABLE backend, which need not be the one
  holding the existing key — so a blind write can leave the operator's new
  value shadowed by the key it was meant to replace, reported as success.
  `replace: true` proceeds through both refusals. The response says which
  happened (`"replaced": true|false`) — or `null`, when the probe never
  saw and the write went ahead on the caller's authority; a claim about
  what was displaced is only made when something observed it.
- **The three write doors hold three different postures on a taken name,
  on purpose.** This door refuses, because the name is the OPERATOR's and
  a collision is as likely as a rotation. `POST /llm-settings` and the
  adapter doors reach `cmd_set_with_value`, documented as *the upsert
  door*: their name is derived from the adapter the operator is
  configuring, they always carry a key that operator just typed, and a
  refusal there would strand the Welcome wizard's LLM step on any
  re-created profile — the keychain keeps `openrouter`, the new profile's
  index does not, and the step would dead-end exactly as the search-key
  step used to. `wild secret rotate` replaces by name because that is what
  the verb means. Same question, three answers, each right for its door;
  what would be wrong is any of them acquiring another's posture by
  accident.
- **The caller must be unrestricted.** A tribe-scoped principal is refused
  (`403`): a fleet-wide keychain entry is not one tenant's to write. The
  per-plugin door (`…/settings/{key}/secret`) is the tribe-scoped way in,
  and it checks `permits_tribe` for the same reason.
- **The caller must be an administrator (#9757).** Storing a key
  (`POST /api/v1/secrets`) and allowing a component to use one
  (`POST /api/v1/secrets/{name}/grant`) take the shared daemon token or a
  named, unscoped user with role `admin` — what `wild user add --operator`
  and the native shell's first-run mint create. Every other role
  (`approver`, `editor`, `reviewer`, `viewer`) and every customer or
  anonymous caller is refused `403`, nothing written. This does not wait
  for `WILD_RBAC_ENABLED`: that flag gates decision rows by risk, and a key
  is not a decision row. The two read doors (`GET /api/v1/secrets`,
  `…/{name}/presence`) are unchanged — they read names, never values.
  Testing a stored key (`POST /api/v1/secrets/{name}/test`, the Keys
  form's „Verbindung testen", ADR-0362 D7) takes the same check: it runs
  each connector that reads the key through its own `PROBE:` tool under
  the installation's credential. It writes nothing and answers in remedy
  keys (`assets/i18n/connector-remedies.yaml`), never the server's text; a
  connector that declares no probe yet answers `key-test-no-probe`. A
  mailbox password (`mail-<account>`) is found through the `system/mail.yaml`
  row that names it and tested by the `imap-connector`'s `imap-probe`. An
  OAuth client secret is found through the `system/oauth.yaml` binding
  whose `client_secret_ref` names it (or, before that row exists, the
  sidecar template's default) and tested by the connector's own probe —
  for Microsoft 365, `m365-mail-probe`, with a token minted for the test,
  never a cached one; a refusal answers `oauth-credential-refused`, a
  token endpoint that is down `unreachable`, a binding not set up yet
  `oauth-not-set-up`, and a sign-in (authorization-code) binding is not
  refreshed for a test (`key-test-no-probe`). The
  probe must be the connector's own read-only tool, files and clears no
  fault (in `wild:http` too), and a key is tested once at a time, at most
  every 5 s.
  The two LLM doors that store a key and grant it to the provider —
  the setup wizard (`POST /api/v1/llm-settings`, `choice: api`) and
  add provider (`POST /api/v1/llm-adapters` with an `api_key`) — admit
  the same callers through the same check (#9826), before any plugin is
  switched on or egress opened. Their keyless answers (`choice:
  claude-cli`, a provider added without a key) write no secret and are
  not asked.

The value rides the request **body** on every door — never a path,
query, or view address (an address is echoed into the command stream and
logged; ADR-0196's guardrail is that an address LOCATES the field and
never carries the value).

## Reference doors — where a NAME is bound

Every other credential surface stores only a **name** pointing at a value one
of the doors above took: an egress destination's `auth: { secret: … }`, a
channel binding's `inbound_secret`, a plugin's `type: secret` setting, an
OAuth binding's `client_secret_ref`.

None of them can tell a stored name from a typo, and none should — a
reference is written while wiring, and the value may legitimately arrive
afterwards. What they must not do is stay **silent** about the mismatch,
which is what they did until 2026-08-02: the write succeeded, the form
reported success, and the first sign of trouble was a `401` on a live call,
far from the field that caused it.

What they can tell is a withheld value: a marker is never a name. So a field
that names a secret refuses one at once, before anything is staged or
previewed, in this sentence:

> This field takes the name of a stored secret, not the secret itself. The
> value was withheld and nothing was changed. Store it under Settings → Security & Keys or
> with `wild secret add <name>`, then ask again with that name.

The question belongs to the writer (`wild_egress::auth_refusal`,
`wild_channels::binding_secret_refusal`,
`wild_oauth::config_writer::client_secret_refusal`). Each door asks it first so
the operator is not asked to approve something that cannot work, and the writer
asks again, because a host-direct caller has no door. A component calling the
same doors gets the sentence as its error.

A forged component's declared login is the one road without an operator-legible
outcome yet. A build that needs a grant refuses at the grant question, so that
question is never put, but the refusal only reaches the daemon log and the
build card still points at a grant card that does not come. A build inside the
tribe's budget is asked as usual, and the writer's refusal after the yes also
only reaches the log. Tracked in #13084.

The tribe's `Keys` Vitals row now carries that check as a family of its own
(`wild_vitals::referenced_secrets`): every name a live binding of the tribe
references must resolve to a stored value, and one that does not reads

> Bindings of `acme` reference secrets with no value: `whapi-token`
> (egress destination `acme-whapi`) has no stored value.

naming both the secret and the thing pointing at it. It is `attention`, never
`broken` — a dangling reference kills the wired path, not the tribe, and a
tribe must not be blocked from starting by an optional outbound sender. The
remedy is the ordinary one (the entry form, or `wild secret add`), under
**exactly** the name the binding already uses.

### Which family judges which credential

Three of the four `Keys` families answer "is this credential usable", and an
operator must never see one gap reported twice in two vocabularies. The first
split is the credential's declared **kind** (ADR-0343 D4): an `oauth2` binding
is a connected account and lands in the OAuth family whatever else its
declaration carries; every static kind (bearer, header, basic, query, path) is
a stored value, and which stored family judges it is decided by where the
credential is READ, not where it is declared:

| The plugin… | Reads the value | Judged by |
|---|---|---|
| calls `wild:secrets/store::get("<key>")` (openai, openrouter) | itself, from the keychain | the declared **plugin-secret** family |
| declares an `auth_binding` of the same name (brave-search) | never — the host injects it from the egress destination | the **referenced** family, on the destination's `secret:` name |
| declares a binding of kind `oauth2` | never — the broker mints a bearer token | the **OAuth** family (its `client_secret_ref` rides the referenced family) |

The second row is the one that bites: `brave-search` declares
`settings: [{key: brave_key, type: secret}]` **and**
`auth_bindings: [{name: brave_key}]`. The setting names the binding, not a
keychain entry — the real entry is whatever the operator put in the egress
destination's `secret:` field, under a different name. Projecting the settings
key as a keychain read would report a correctly wired install as broken.

## CLI surface

### Value management

```bash
wild secret add <name>
  # interactive value-prompt (stdin, no echo). NEVER as argv —
  # process listings would leak it. `--from-stdin` flag for piping
  # in CI: `cat secret.txt | wild secret add foo --from-stdin`.

wild secret list
  # names only, NEVER values. Sorted, with the source-backend
  # column showing where each value lives (keychain/file/env)
  # so a user can spot mis-configured chains. With audit events
  # enabled, also shows last-used hint.

wild secret show <name> --confirm
  # shows the value. Requires --confirm to avoid accidental shell-
  # history leak. Emits a special audit event "human-show".
  # stdout carries the value ALONE (one line, so it can be piped);
  # the "printing value" notice and any chain warning go to stderr.
  # Optionally prints to a tmp file the user opens (so it's not
  # in scrollback): wild secret show foo --to-clipboard.

wild secret remove <name>
  # warns if any grant exists for this name + lists them. --force
  # removes anyway, cascading the grants.

wild secret rotate <name>
  # interactive new-value prompt. Atomic add-new + drop-old. All
  # existing grants stay attached to the same name.
```

### ACL management

> **P7 update**: `wild secret grant` is now a deprecated alias for
> `wild plugin grant <slug> --secret <name>`. The legacy verb still
> works for one release with a stderr deprecation hint; new scripts
> should use the unified plugin verb. See
> plugin-cli.md for the full surface.
> Post-P7 the grants file lives at
> `<profile_root>/system/secret-grants.json` (per-profile),
> with a legacy fallback to `<profile_root>/system/secret-grants.json` when no
> profile is active.

```bash
# Post-P7 canonical form:
wild plugin grant <slug> --secret <name> [--force]
  # routes to <profile_root>/system/secret-grants.json.

# Legacy (deprecated, removed in next release):
wild secret grant <component-id> <secret-name>
  # adds an entry to <profile_root>/system/secret-grants.json.

wild secret revoke <component-id> [<secret-name>]
  # removes one grant, or all grants for that component when name
  # is omitted.

wild secret grants
  # renders the table. Filters: --component <id>, --secret <name>.
```

### Discovery

```bash
wild secret backend
  # shows the active resolver chain in order, each backend's
  # writability + health probe. Renders WILD_SECRETS_BACKEND_ORDER
  # if set so the user can audit overrides.

wild doctor
  # already exists; gains a "secrets" row showing
  # backend-name + count of stored secrets + count of grants.
```

## Component-side helper

> **Status: not yet built (P6.7).** Components today call the raw
> `wild:secrets/store::get` bindgen output directly. The wrapper
> crate below is the planned DX sugar — it does not exist yet.

Components shouldn't have to handle the WIT bindgen output by
hand for every `get`. Phase 6.7 ships `wild-secrets-component`
as a small wrapper crate (cargo-component compatible):

```rust
// In a component:
use wild_secrets_component::{secrets, SecretError};

let api_key = match secrets::get("api-key") {
    Ok(v) => v,
    Err(SecretError::NotFound) => {
        // Tell the user how to fix it
        return Err("api-key secret missing. Run: wild secret add ...".into());
    }
    Err(SecretError::AccessDenied) => {
        return Err("api-key not granted to this component. Run: wild secret grant ...".into());
    }
    Err(SecretError::BackendUnavailable(reason)) => {
        return Err(format!("secrets backend down: {reason}"));
    }
};
```

Same pattern for Go / JS / Python WASI-P2 toolchains down the
line; helper crate is Rust-only at first.

## Audit events

Every `secrets::get` call publishes a JSON event on
`wild.{tribe}.system.secrets.access`:

```json
{
  "component_id": "awesome",
  "alias": "api-key",
  "keychain_name": "my-openrouter-personal-key",
  "result": "ok",
  "timestamp_ms": 1714234567890,
  "tribe_id": "acme-sales"
}
```

`result` is `"ok"`, `"not-found"`, `"denied"`, or
`"backend-error: <kind>"`. **The value is NEVER in the event.**
TUI / observability dashboards subscribe to this subject for
real-time visibility; long-term retention via JetStream is opt-
in (the org's policy decision).

`wild secret show --confirm` emits a distinct event with
`result: "human-show"` so an audit reviewer can spot human
inspection separately from component reads.

## Bootstrap question — pre-load or lazy?

**Lazy** (every `get` call hits the backend). Empirically:
- macOS Keychain reads: ~1–3ms per call after the first
  TouchID prompt.
- Linux Secret Service: ~1ms per call (D-Bus round-trip).
- Compared to a 1–60s LLM call, the keychain hit is noise.

**Pre-load** would mean the host reads every secret at component
instantiation and injects values into the component-store; values
sit in process memory for component-life. That's a longer
attack surface for a memory-dump exploit.

**Decision: lazy.** Doc-pin the rule, reject component caching
("re-read every call"). If a measured component cycle shows
keychain reads dominate latency, revisit.

## Rotation semantics

`wild secret rotate <name>` atomically replaces the value in the
backend. Long-running cycles see the new value on their next
`get` — no signal, no event, just "next read returns new value".

This is enough for the Tier-2 LLM-adapter use case: every chat
call reads its api-key, so within one request-response round
the new key takes effect. Components that build long-lived
clients (e.g. a database driver pinned at instantiate) would
need a different model — flagged as a Phase 6.x extension if a
real use case appears.

## Shipped extensions beyond the original design

Three things landed in code that the original design above didn't
name:

- **Secret side-index** (`wild-secrets/src/index.rs`). The OS
  keychain can't enumerate entries, so `wild secret add` also
  records `{name, added_at_ms, added_via}` into
  `<profile_root>/system/secret-index.json`. The index is a name
  catalogue only, never values. Reads observed at the host can also
  feed it (`record_observed_read`).

  **The index is not the list** (#13420). It is per profile while the
  keychain is installation-wide, so a re-created profile has every key
  and no catalogue. `wild secret list` and `GET /api/v1/secrets` now
  read `wild_shared::secret::catalog`: the **union** of the index and
  every name Wild itself declares — bundled and installed plugin
  sidecars (`auth_bindings`, `secret_aliases`), LLM/embed/rerank
  adapter `secrets:`, egress `AuthBinding::Static`, OAuth
  `client_secret_ref`, forged component-type `auth_bindings` — and ask
  the presence door once per name. Mail-account passwords
  (`system/mail.yaml`) are not in the union yet; naming that is a gap
  this list still has.

  Each row carries one of three operator-legible states. The BACKEND
  column in `wild secret list` renders them (and the backend id for a
  stored row):

  | Cell | State | Means |
  |---|---|---|
  | `<id>` | stored | this backend resolved it |
  | `(awaiting your value)` | missing | a declaration wants this name and no value is stored — the operator's turn, not a fault |
  | `(missing)` | missing | every backend the ORDER names was consulted, none held it — an index entry is stale, `wild secret remove <name>` is right |
  | `(could not check)` | unknown | at least one configured backend was **never built** or did not answer, so absence proves nothing — do NOT remove |

  The unknown state is not hypothetical on macOS. An unentitled `wild`
  — a host-tarball or `cargo install` build, anything outside the
  signed `Wild.app` — cannot reach `keychain` (ADR-0287; until
  ADR-0379 the slot was dropped, now it answers every call with the
  reason). Every key the packaged app stored there then listed as
  `(missing)`, and the hint told the operator to delete the index entry
  for a secret that works. The verdict now keys on the *configured*
  order rather than on the built chain, and names the binary that can
  reach what this one cannot.

  The dashboard's Security & Keys page shows the same states: "Not
  stored" only when the store said so, "Could not check" when it did
  not answer. The two must never be folded together.

  **The index outlives less than the keys it indexes.** It lives
  under `<profile_root>`, while the keychain backend is
  installation-wide — so deleting a profile dropped the whole
  catalogue and kept every key. The list used to serve only that
  catalogue, so it said "nothing stored" while `POST
  /api/v1/secrets` still answered `409` for the same name — how the
  welcome wizard came to ask for a Brave key and answer the paste
  with "already stored" (measured 2026-09-21). The list now reads the
  union above, so those two doors cannot disagree about a name the
  store resolves. A surface deciding whether to ask for a
  key at all still reads the per-name door,
  **`GET /api/v1/secrets/{name}/presence`**, which probes the
  resolver chain per name via `contains_bounded` — attributes only,
  so macOS raises no approval dialog, and fenced, so a wedged
  keychain costs a timeout. It answers four ways:
  `present` · `missing` · `unreadable` · `unknown` — `unreadable` a
  backend that answered with an error (a keychain this build cannot
  reach, ADR-0379 D3), `unknown` a chain that did not answer. Neither
  is "no key is stored", and a caller that reads one as that will
  offer to overwrite a credential it never saw; a client that does not
  know `unreadable` reads it as `unknown`.

- **The provider key notice closes on "usable", not on "stored".** The
  operator inbox row `secret-gap-<plugin>` is open while a secret the
  plugin declares is not usable: bound, stored, AND granted to the plugin
  that reads it. The rule is one function,
  `wild_runtime::health::secret_gap`; boot health raises and clears by
  it, and the two secret writes that can end a gap while the daemon runs
  (the store, and the grant) settle the row by it in
  `wild_shared::secret_gap`. A key that arrives ungranted leaves the row
  open and restates it to name the plugin and the place that allows it
  (the Security & Keys form, carried as the row's door on the ungranted
  names); the grant closes it. A released call whose answer names a key
  nobody stored yet (a mailbox set up on a server no preset names) files
  the same kind of row, one per key NAME (`secret-gap-key-<name>`,
  `wild_shared::secret_gap::raise_key_gap`): its button opens the Keys
  form with the name filled in, and the store closes it. Which doors grant:

  | Door | Stores | Grants |
  |---|---|---|
  | `wild secret add <name>` | yes | no |
  | Security & Keys form (`POST /api/v1/secrets`) | yes | only on the operator's yes, see below |
  | provider setup wizard (`POST /api/v1/llm-settings`) | yes | yes, to the provider's plugin |
  | add provider (`POST /api/v1/llm-adapters`) | yes | yes, to the provider's plugin |
  | `wild plugin grant <slug> --secret <name>` | no | yes |

  The store itself never grants: it takes a name and has no plugin in
  hand, and a grant is an authorization (§ Non-goals: no implicit
  grants). What the Keys form does instead (#9359): its store answers
  `awaiting_grant`, the components an OPEN `secret-gap` notice waits on
  to use exactly that name (`wild_shared::secret::readers_awaiting_grant_of`).
  The form asks about each by name — „`<plugin>` wartet auf diesen
  Schlüssel. Darf es ihn verwenden?" / "`<plugin>` is waiting for this
  key. May it use it?" — and only the operator's yes sends
  `POST /api/v1/secrets/{name}/grant`. That door records the grant
  through the same writer as `wild plugin grant`, and refuses (`409`,
  nothing recorded) a reader nothing waits on for that name, so it
  is the notice's answer and never a general grant path.

  A tribe worker (`<tribe>-<worker>`) waits too, though no notice names
  it — boot health raises `secret-gap` notices only for bootstrap
  plugins (#9965). When its tribe's blueprint `## Secrets` declares the
  name and the worker in the tribe's deployed roster
  (`tribe-state/<slug>/render-input`) has no grant on it, it is in
  `awaiting_grant` and the grant door accepts it
  (`wild_tribe_ops::tribe_lifecycle::tribe_workers_awaiting_grant`, over
  `wild_secrets::grants_check::ungranted_readers_of`). These are the
  workers the Keys row reports "ungranted" and the activate gate refuses
  on. A tribe never applied, created or deployed has no roster yet and
  names no worker. "Not now" stores
  the key without the grant, and the notice stays open saying so.

  Each daemon door files its grant row under its own name and the caller
  it acted for — `granted_via` reads `keys form, by human:alice`,
  `llm setup wizard, by system` or `add provider, by human:alice`
  (`system` is the shared daemon token), where the CLI keeps its command
  text. The caller is the same `actor` the log lines above carry.

- **Required-secrets activation gate (ADR-0059).** A worker
  manifest can declare a `## Secrets` block; the tribe render
  projects those into the worker's `wasi:config`, and the activate
  gate (`grants_check.rs`) **refuses to start a component whose
  declared-required secrets aren't granted** — turning a runtime
  `access-denied` into an upfront, legible boot failure. This is
  the "Component declares required secrets at install time?" open
  question, answered: yes, via the manifest.

- **Pooled-component grants by slug.** A pooled adapter instance
  presents a `component_id` like `plugin/<slug>/instance`; the ACL
  check (`grant_allows`) resolves the grant against the bare
  `<slug>` so one grant covers every pooled instance.

The alias→keychain-name map also reaches the host through
`wasi:config` under the `secret-alias.` key prefix
(`SECRET_ALIAS_CONFIG_PREFIX`, projected by `extract_alias_map`),
in addition to the per-component config files described above.

## Phase 6 roadmap — status

| Step | Scope | Status |
|---|---|---|
| **P6.1** | `wild:secrets@0.1.0` WIT + `SecretsBackend` trait + `ChainedResolver` + KeychainBackend + EnvBackend (always-on read-only). | ✅ Shipped |
| **P6.2** | Linux Secret Service backend. **Folded into `KeychainBackend`** via the `keyring` crate's `sync-secret-service` feature — no separate Rust type. | ✅ Shipped |
| **P6.3** | `wild secret {add,list,show,remove,rotate,backend}` CLI. Interactive prompts, atomic writes, no-argv-values. `list` reads the side-index; `backend` renders the active chain. | ✅ Shipped |
| **P6.4** | ACL grants file + `wild secret {grant,revoke,grants}` CLI + host-side enforcement on every `get`. | ✅ Shipped |
| **P6.5** | Audit-event publishing on `wild.{tribe}.system.secrets.access` (shared `AuditSink`). | ✅ Shipped |
| **P6.6** | Windows Credential Manager backend. **Folded into `KeychainBackend`** via the `keyring` crate's `windows-native` feature. | ✅ Shipped |
| **P6.7** | `wild-secrets-component` helper crate + cargo-component example. | ⏳ Pending (DX-only) |
| **P6.8** | EncryptedFileBackend with master key from `WILD_SECRETS_KEY`. | ✅ Shipped |

Phase 7's secrets precondition (P6.1 + P6.3 + P6.4 + P6.5) is met.
Only the P6.7 helper crate is outstanding, and it's a developer-
experience convenience, not a blocker.

**Two lanes, and only one of them is this document's.** P6.6 covers the
OPERATOR's secrets — LLM keys, connector credentials — which on Windows live
in the Credential Manager and never touch the profile tree. Wild's OWN
credentials are the other lane: `system/token`, `tokens.toml`,
`dashboard-token`, `identity/install_key` (`forge/install_key` on a
profile that predates 2026-08-22 — both are read, both are restricted),
and `secrets.key` when the file
backend is opted into. Those must be readable unattended at boot, so they stay
files, and ADR-0271 decides what protects them: an inheritable, owner-only
restriction on the DIRECTORY, verified after it is set, with an operator-legible
outcome when it cannot be. The two lanes are independent — the operator-secret
path was never affected by the gap ADR-0271 closes.

## References

- Shipped engine: `crates/runtime/wild-secrets/` (`backends.rs`,
  `grants.rs`, `grants_check.rs`, `audit.rs`, `index.rs`,
  `lib.rs`). Host wiring: `crates/runtime/wild-host/src/wild_secrets_plugin.rs`.
  CLI handlers: `crates/runtime/shared/src/secret.rs`. WIT:
  `wit/secrets/secrets.wit`. Config: `SecretsConfig` in
  `crates/runtime/wild-runtime/src/config.rs`.
- Sibling plugin pattern: `crates/runtime/wild-host/src/wild_ai_plugin.rs`,
  `wild_blobs_plugin.rs`, `wild_tasks_plugin.rs` — all share the
  trait + bindgen + host-adapter shape `wild:secrets` follows.
- Cross-cutting consumer: `wild/docs/llm-adapters.md` "Adapter
  delivery — two tiers" — Tier-2 components are the proximate
  driver of Phase 6.
- D4 install pattern:
  `<profile_root>/system/plugin-cache/<slug>.json` —
  the alias map for non-adapter components rides on the same
  daemon-managed sidecar (`family_dir(Family::Plugin)`).
- Operation event pattern: `wild.{tribe}.system.operation.started/
  finished` — secrets-access events follow the same subject
  shape.
- Keychain Rust crate: `keyring` v3 (one cross-platform crate),
  selected per-OS by cargo feature — `apple-native` (macOS),
  `sync-secret-service` + `crypto-rust` (Linux), `windows-native`
  (Windows). Replaces the three separate per-OS crates the original
  design assumed.
