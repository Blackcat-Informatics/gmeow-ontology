# Issue #1777: deps: move GMEOW-owned serde CBOR records from ciborium to purrdf_lex::cbor

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, dependencies, rust

## Body

## Problem

PurRDF 3.0's `purrdf_lex::cbor` is the substrate's one RFC 8949 codec; it replaced `ciborium` everywhere in PurRDF. The 3.0 upgrade moved every GMEOW CBOR use that touches PurRDF (GTS frames, blob metadata, the ingestion receipt) onto it. The remaining `ciborium` uses serialize GMEOW-owned serde types and need the serde migration first:

- `crates/pipeline/src/cache.rs` (stage cache publications), `handle_identity.rs`, `stages/conformance/source_admission.rs`
- `crates/logic/src/result_rdf/native.rs` (execution receipts), `program_analysis.rs` (program digest), and test round-trips
- `crates/lang-bridge/src/gmn1_codec/native.rs` (GMN1 wire)

## Scope

Encode these records as `purrdf_lex::cbor::Value` (via `ToJson`-style record impls or explicit `Value` builders), then drop `ciborium` from the workspace.

## Acceptance

- Byte-identical encodings for every persisted GMEOW record (cache keys, receipts, handle identities, GMN1 wire), proven by existing golden/round-trip tests plus a before/after byte-parity test per record.
- `ciborium` absent from the dependency graph; `make check` passes.

Depends on the serde→`purrdf_lex::json` migration issue for the record-derive story.


## Comments (1)

### paudley — 2026-10-27T06:50:05Z

The complete prepared GMN codebook CBOR transport is owned by #2007. Its borrowed producer, bounded native decoder and all four record/table boundaries move together. The original authored packet is captured before replacing the encoder; the native implementation must retain its exact header, checksum, field order, source identity and payload bytes. No legacy test-only codec remains after the cutover.

The existing table/identity/refusal assertions stay in place. Authored component checks and actual host callers are selected; browser assets, authenticated corpus and installed/release acceptance remain unverified under the standing producer/full-gate exception. The workspace binary-owner contract remains open, including pipeline publications, execution receipts and typed commitments. Principles 4 and 7.


