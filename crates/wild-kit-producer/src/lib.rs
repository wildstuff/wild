//! `wild-kit` as a library: the producer half of the ingest contract, for a
//! connector's developer to build on ([`producer`], ADR-0398 D10). It links
//! no Wild code but the wire contract (`wild-ingest-contract`).
//!
//! The kit's binaries — `wild-kit`, which serves a door and judges a
//! producer, and the reference producer — live in the `wild-kit-cli`
//! package and sit on top of this library. The crate's README holds a first
//! producer that compiles as it stands — announce, take the key, send a
//! record — and [`producer::calls`] shows how to answer the calls Wild makes
//! on a connector's tools. The reference producer's full source
//! (`crates/wild-kit/src/bin/reference_producer.rs`) is in the repository
//! on GitHub, not in this crate.

pub mod producer;

// The README's example is compiled as a doctest (`cargo test --doc -p
// wild-kit`, a CI step), so it cannot drift from the API it shows.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExample;
