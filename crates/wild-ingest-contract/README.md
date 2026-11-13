# wild-ingest-contract

The wire contract of Wild's producer ingest door: every request, reply,
code, key class and bound a producer outside Wild speaks to the door, with
the JSON Schemas derived from these types.

A producer in Rust builds on it together with
[`wild-kit`](https://crates.io/crates/wild-kit):

```sh
cargo add wild-ingest-contract wild-kit
```

A producer in any other language writes against the published OpenAPI
document and JSON Schemas instead:
[docs/ingest-contract](https://github.com/wildstuff/wild/tree/main/docs/ingest-contract).
The guide for connector developers is
[docs/wild-kit.md](https://github.com/wildstuff/wild/blob/main/docs/wild-kit.md).

Licensed under either of MIT or Apache-2.0, at your option.
