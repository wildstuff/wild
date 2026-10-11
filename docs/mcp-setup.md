# MCP setup — chat with your Wildnis from Claude Desktop / Cursor

`wild` ships an [MCP](https://modelcontextprotocol.io/) server that
bridges any MCP-speaking client (Claude Desktop, Cursor, the
`claude` CLI, …) to one running Wildnis. Once configured, you can
ask the Elder questions ("frag den Elder, was die wildnis macht"),
inspect tribes, hand them tasks, and read documents — all from
your existing chat client without juggling terminals.

## Prerequisites

1. A running daemon: `wild up --offline` in another terminal. The
   MCP subprocess connects to the same NATS that daemon publishes
   on; without it, every `ask` call times out.
2. An LLM adapter wired to the Elder (root tribe). Check with
   `wild config llm list` — at least one entry must be present and
   reachable. The default ships with `kind: anthropic-cli` (talks
   to a locally-installed `claude` binary via OAuth) — no API key
   env var needed.
3. The MCP-speaking client of your choice (this guide uses Claude
   Desktop; Cursor and the `claude` CLI follow the same JSON
   shape).

## Quick start (3 commands)

```sh
# 1. Generate a copy-paste-ready snippet for your install:
wild config mcp setup

# 2. Open Claude Desktop's config (paths below per OS):
#    Paste the inner `wild-<profile>` entry into the existing
#    `mcpServers` map, or use the whole snippet as the file's
#    content if it doesn't exist yet.

# 3. Quit + re-open Claude Desktop. The `wild-<profile>` server
#    appears in the tools menu (the connector / hammer icon).
```

The `setup` command auto-detects:

- Your active profile (`wild-<profile>` becomes the server name —
  multiple parallel installs land under distinct names).
- The NATS URL from `<wild_root>/cli.toml`.
- The absolute path of the running `wild` binary (so a future
  `~/.cargo/bin` move doesn't silently break the config).
- `WILD_HOME` (so `wild mcp` finds the same profile data the
  daemon's using).

## Claude Desktop config paths

| OS | Path |
|---|---|
| **macOS** | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| **Linux** | `~/.config/Claude/claude_desktop_config.json` |
| **Windows** | `%APPDATA%\Claude\claude_desktop_config.json` |

If the file doesn't exist, create it with the `wild config mcp
setup` snippet as the entire content. If it does, merge the inner
`wild-<profile>` entry into the existing `mcpServers` object —
Claude Desktop happily runs many MCP servers in parallel.

## Cursor + `claude` CLI

Both speak the same MCP protocol; the JSON shape is the same.
Cursor's config lives at `~/.cursor/mcp.json`; the `claude` CLI
reads `~/.claude/mcp.json`. Paste the same snippet.

## Verification

After restarting your client, ask:

> *"frag den Elder, was die wildnis macht"*  
> *"ask the Elder to summarize the active tribes"*

The client calls the `ask` MCP tool, which publishes on
`wild.root.user.cli.message` and waits for the Elder's response on
`wild.root.user.cli.response`. The reply text comes back inline.

You can also verify the wiring without a chat client:

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | \
  wild mcp
```

Should print a single JSON-RPC response listing every MCP tool
this server exposes (`ask`, `give_task`, `list_tribes`,
`vital_signs`, …).

## Available tools

The full list lives in `crates/runtime/mcp-tools/src/protocol/tool_catalog.rs` — every tool is described in
LLM-friendly prose so the model knows when to pick it.
Highlights:

- **`ask`** — synchronous Q&A with Elder (default) or any tribe's
  chief. The dominant entry point.
- **`give_task`** — instruction + optional file attachment
  (PDF, image, audio); the tribe processes it on its next cycle.
- **`list_tribes` / `tribe_status` / `vital_signs`** — discovery /
  state.
- **`recent_decisions` / `reflect_insights`** — activity history
  + the Elder's mining output.
- **`operator_inbox`** — the one "what is waiting for me?" view:
  every open `to: operator` item (audit findings, chief escalations,
  forge grants, open `chief_ask_user` questions, and any other open
  decision) aggregated into a single list, each row naming the existing
  tool to resolve it (`inbox_resolve` for findings/escalations/forge-
  grants, `ask` for questions, `decision_resolve` for generic decisions).
  Read-only; `inbox_status` / `inbox_list` / `inbox_resolve` remain
  the per-source / resolve surfaces.
- **`decision_resolve`** — the operator-resolve path for a GENERIC
  decision on the decisions trail (`goal.requirements.ratify`, an
  autonomy-confirm, an `evolve.*` proposal) — i.e. anything that is not a
  finding/escalation/forge-grant (those go through `inbox_resolve`) and
  not a `chief_ask_user` question (answer those with `ask`). Args: `id`
  (the decision UUID from `operator_inbox`), `outcome`
  (`confirmed` | `rejected`), optional `edited_blob` (a JSON edit of the
  proposal — e.g. an adjusted ratify bar — that lands as the outcome),
  optional `note`. Routes through the hook-carrying decisions-resolve
  seam, so a confirmed ratify persists the (edited) goal-requirements bar
  FS-canonical under `system/goal-requirements/<tribe>.yaml` and fires the
  capability-fit genesis. Operator-only; idempotent on an already-resolved
  decision.
- **`list_documents` / `read_document`** — tribe-side docs.
- **`read_blueprint` / `update_blueprint` / `set_schedule`** —
  mutation.
- **`create_tribe`** — synthesize a minimal tribe from a name +
  persona description (one-shot zero-to-running).

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `ask` always times out | `wild up` not running | Start the daemon |
| `ask` reaches a tribe but no reply | LLM adapter missing / unauthorized | `wild config llm list`; rotate creds; `claude /login` |
| Server doesn't appear in Claude Desktop | JSON syntax error in config | Validate with `jq . < claude_desktop_config.json` |
| Server appears but tools fail | Stale `WILD_HOME` from a removed profile | Re-run `wild config mcp setup` and re-paste |

## HTTP transport (remote clients)

For Claude.ai web or any MCP client that doesn't spawn a stdio
subprocess, run the MCP HTTP listener:

```sh
wild mcp --transport http --bind 127.0.0.1:8080
```

The bearer token lives at `<profile>/system/token` (auto-generated on
first run, mode 0600) and is shared with the REST transport. Rotate via
`wild config token rotate`.

### Ports — managed in `profile.yaml`

Each profile pins its own HTTP ports in
`<profile_root>/profile.yaml`, so several installs can run side
by side without colliding. Two planes, both speaking to the **same**
authenticated tool surface:

```yaml
mcp_port: 7531     # MCP-over-HTTP — always on (the port only picks where)
rest_port: null    # REST control/query plane — off; enter a port to enable
```

- **MCP** is on by default — `wild up` brings up the listener on
  `mcp_port` (override per-launch with `--mcp-http <addr>` or
  `WILD_MCP_BIND`). An **explicitly requested** listener (flag or env)
  is required: if the bind fails — typically because another daemon
  already holds the port — boot aborts with a non-zero exit instead of
  silently reporting healthy while your MCP calls land on the foreign
  daemon (issue #3196). A failed bind on the plain `mcp_port` fallback
  only logs a WARN and the session continues socket-only.
- **REST** (a plain HTTP/JSON plane for UIs and scripts — `GET /healthz`,
  `GET /api/v1/openapi.json`, `POST /api/v1/tools/{name}`, …) always serves
  over the **Unix socket** at `<profile>/system/api.sock` (mode `0600`,
  owner-only) — that is the operator plane's default door and what the native
  desktop app dials, so no open TCP port is needed. The **TCP** door is
  **opt-in** (P3c): a fresh profile shows `rest_port: null` (no network listener).
  **Enter a port to turn TCP on** — set `rest_port: 7532`, restart the daemon,
  and REST comes up on `127.0.0.1:7532`. You need this for the **browser**
  dashboard (a browser can't dial a Unix socket). It is loopback-only for now.
  The bind also honours `--rest-http <addr>` / `WILD_REST_BIND` (these win over
  the profile).

Every plane — the socket, an opted-in HTTP door, MCP — uses the same bearer
token and the same authorization gate; there is no second, unlocked door.
