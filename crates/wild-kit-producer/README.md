# wild-kit

The producer half of the wild-kit: a Rust client for Wild's producer
ingest door — pairing, the pinned TLS connection, the door's calls and the
MCP view. It links no Wild code but the wire contract,
[`wild-ingest-contract`](https://crates.io/crates/wild-ingest-contract).

## A first producer

```sh
cargo add wild-kit serde_json
```

That is everything the example below uses. It announces itself to a
door, takes its key once the door says yes, and sends one record. Copy it
into `src/main.rs`:

```no_run
use std::{io, thread, time::Duration};

use serde_json::{json, Value};
use wild_kit::producer::{door, http::request};

fn main() -> io::Result<()> {
    // The door: the Unix socket path `wild-kit serve` prints (`{socket}` under `wild-kit run`).
    let socket = std::env::args().nth(1).expect("usage: first-producer <socket>");
    let door = door::socket(&socket).map_err(io::Error::other)?;

    // 1. Announce: who this producer is and the one shape of record it sends.
    let manifest = json!({
        "producer": "first-producer", "version": "1.0.0", "contract_version": "1",
        "fingerprint": "first-producer-on-this-machine", "needs_vocabulary": false,
        "sources": [{
            "slug": "demo", "description": "A first producer's one tick.",
            "shapes": [{
                "name": "tick", "version": 1, "key_class": "live", "key": [],
                "target_type": "Takt", "description": "One tick.",
                "fields": [{"name": "n", "kind": "number", "required": true,
                            "description": "Which tick, e.g. 1."}]
            }]
        }]
    });
    let (status, _, reply) = request(
        &door, "POST", "/api/v1/producers/announcements",
        &[("content-type", "application/json")], manifest.to_string().as_bytes(),
    )?;
    let id = reply["announcement"].as_str()
        .ok_or_else(|| io::Error::other(format!("announcement answered {status}: {reply}")))?
        .to_string();

    // 2. Wait for the operator's yes (the kit's sandbox gives it at once):
    //    the answer carries the producer's key, its tribe and its first epoch.
    let key: Value = loop {
        let (_, _, reply) = request(&door, "GET", &format!("/api/v1/producers/announcements/{id}"), &[], &[])?;
        if reply["producer_key"].is_string() {
            break reply;
        }
        thread::sleep(Duration::from_millis(500));
    };
    let (tribe, token, epoch) = (
        key["tribe"].as_str().unwrap(), key["producer_key"].as_str().unwrap(), key["epoch"].as_str().unwrap(),
    );

    // 3. Send one batch: one JSON record per line, under the key.
    let body = json!({"shape": "tick@1", "record": {"_epoch": epoch, "_seq": 0, "n": 1}}).to_string() + "\n";
    let auth = format!("Bearer {token}");
    let path = format!("/api/v1/tribes/{tribe}/producers/first-producer/sources/demo/batches");
    let (status, _, reply) = request(&door, "POST", &path, &[("authorization", auth.as_str())], body.as_bytes())?;
    println!("the door answered {status}: {}", reply["state"]);
    Ok(())
}
```

**Licence.** This library crate is MIT OR Apache-2.0. The `wild-kit`
binary is not part of that licence: it is commercially licensed (its
package declares `license = "LicenseRef-Wildstuff-Commercial"`) and is
only available from Wild's product releases, not from crates.io.

Try it against a sandbox door, which says yes at once. This library crate
and the `wild-kit` binary are versioned separately: the crate is 0.1.x on
crates.io, the binary ships in Wild's product releases. Take it from the
newest
[release](https://github.com/wildstuff/wild/releases) that carries it (an
asset named `wild-kit-<version>-<target>.tar.gz`; releases up to
`v0.5.0-rc.26` do not), as
[docs/wild-kit.md](https://github.com/wildstuff/wild/blob/main/docs/wild-kit.md#get-the-kit-on-any-machine)
shows. `wild-kit --help` proves it runs (it has no `--version`). Then,
with a short directory (a socket path must stay under 104 bytes):

```sh
wild-kit serve --dir /tmp/wk &
cargo run -- /tmp/wk/system/ingest.sock   # the door answered 202: "received"
wild-kit rows --dir /tmp/wk               # your record, as the sandbox wrote it
```

From here, `wild-kit run -- <your producer command>` plays the hard cases
(a lost reply, a crash mid-send, a paused door), and
[`wild_kit::producer::calls`](https://docs.rs/wild-kit/latest/wild_kit/producer/calls/)
shows how to answer the calls Wild makes on your connector's tools.

API documentation: [docs.rs/wild-kit](https://docs.rs/wild-kit). The guide for connector developers is
[docs/wild-kit.md](https://github.com/wildstuff/wild/blob/main/docs/wild-kit.md#rust-use-the-crate).

The library is licensed under either of MIT or Apache-2.0, at your option. The `wild-kit` binary is not (see Licence above).
