# sharepoint-connector

ADR-0141 PR1 existence proof: a third-party Tier-2 plugin that exercises all four installable compute primitives in a single component.

## Roles

| Primitive | WIT export | Tools/backings declared |
|---|---|---|
| tool-provider | `wild:tool-provider/tools@0.4.0` | `sharepoint-enumerate`, `sharepoint-fetch` |
| effect-handler | `wild:tool-provider/tools@0.4.0` + `effect.sharepoint` bundle | `sharepoint-create-folder`, `sharepoint-update-list-item` (defaults in sidecar `effects` map) |
| worker | `wild:worker/handler@0.1.0`, `wild:worker/meta@0.1.0` | `sharepoint-change-feed` trigger |
| function-backing | `wild:function/backing@0.1.0` | `sharepoint-resolve-user`, `sharepoint-resolve-site` |

## Imports

- `wild:secrets/store@0.1.0`
- `wasi:http/outgoing-handler@0.2.0`
- `wild:messaging/consumer@0.3.0`

## Build

```bash
./build.sh
```

This runs `cargo build --target wasm32-wasip2 --release`. The produced `.wasm` is a `wasm32-wasip2` component meant to be loaded by the Wild embedded host.

## Note

All tool and backing bodies return deterministic stub JSON. No real HTTP calls are made, so the plugin can be loaded and inspected without a live SharePoint tenant. Replace the stubs with real `wasi:http` calls and `wild:secrets` reads to turn this into a working connector.
