//! The wire contract of the producer ingest door (ADR-0365).
//!
//! A **producer** is a program outside the daemon, in any language — written by
//! a developer or built by an external agent — that streams closed records into
//! a tribe through the ingest door. This crate is the one
//! definition of what crosses that door:
//!
//! - [`batch`] — a batch's body, its id (the hash of its decoded body) and the
//!   bounds every batch is held to;
//! - [`shape`] — a declared shape, its version and its key class;
//! - [`record_id`] — how a record's id is formed, scoped to its device;
//! - [`reply`] — every reply the door gives, and how an unknown one is read;
//! - [`codes`] — the closed sets of refusal and conflict codes;
//! - [`manifest`] — what a producer announces about itself (ADR-0365 D16);
//! - [`tools`] — the functions a connector offers, and the calls and answers
//!   that reach it on its call wait (ADR-0398 D1, D4);
//! - [`device`] — epochs, the commit mark and the vocabulary;
//! - [`door_certificate`] — the pin a producer checks the door's TLS
//!   certificate against;
//! - [`pairing_file`] — the door, its pin and the one-time code, in a file the
//!   program reads once;
//! - [`routes`] — the door's routes, as data, and [`openapi`] — the OpenAPI
//!   document rendered from them;
//! - [`sandbox`] — the marker of a `wild-kit` sandbox profile.
//!
//! **A producer is an intake plugin that runs elsewhere.** A batch the door
//! receives is held durably by Wild and then runs through the source's own
//! intake flow, exactly like an item a local connector fetched: the operator's
//! enrich plan, the sink, the stage walk, per-item dead-letter and retry
//! (ADR-0365 D0). So a producer drops a batch once it reads `received`,
//! `processed` or `failed`; what the flow does with it is Wild's, reported for
//! information only.
//!
//! **Additive only (ADR-0365 D1b).** Within [`CONTRACT_VERSION`] the contract
//! only grows: a field, a code, a state or a route may be added; none is ever
//! renamed, removed or given another meaning. Every type here therefore reads
//! an unknown field without failing, and every closed set reads an unknown
//! member as its `Unknown` variant, which a producer must treat as "not
//! received: keep the batch and back off".
//!
//! [`schema::documents`] derives the published JSON Schemas from these types.

pub mod agent_connection;
pub mod batch;
pub mod codes;
pub mod device;
pub mod door_certificate;
pub mod manifest;
pub mod openapi;
pub mod pairing_file;
pub mod record_id;
pub mod reply;
pub mod routes;
pub mod sandbox;
pub mod schema;
pub mod shape;
pub mod tools;

#[cfg(test)]
mod tests;

/// The contract version a producer states in its manifest and the door
/// serves. A change that cannot be additive is a new version, served beside
/// this one (ADR-0365 D1b).
pub const CONTRACT_VERSION: &str = "1";

/// The URL path prefix of every route of this contract version.
pub const ROUTE_PREFIX: &str = "/api/v1";
