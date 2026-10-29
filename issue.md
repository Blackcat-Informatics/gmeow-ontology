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


## Comments (2)

### paudley — 2026-10-27T06:50:05Z

The complete prepared GMN codebook CBOR transport is owned by #2007. Its borrowed producer, bounded native decoder and all four record/table boundaries move together. The original authored packet is captured before replacing the encoder; the native implementation must retain its exact header, checksum, field order, source identity and payload bytes. No legacy test-only codec remains after the cutover.

The existing table/identity/refusal assertions stay in place. Authored component checks and actual host callers are selected; browser assets, authenticated corpus and installed/release acceptance remain unverified under the standing producer/full-gate exception. The workspace binary-owner contract remains open, including pipeline publications, execution receipts and typed commitments. Principles 4 and 7.


### paudley — 2026-10-29T14:26:55Z

The complete source-catalog identity operation landed in PR https://github.com/Blackcat-Informatics/gmeow-ontology/pull/2051. Its borrowed native CBOR writer preserves every receipt field, all base origins, recorded order and multiplicity, nulls and BLAKE3 framing. Ten original complete packets and actual identities matched in the existing synthetic control; focused consumer diagnostics, helpers, review and normal hooks passed. This is part of the parent contract, which remains open. Principles 4 and 7.

The next complete LogicProgram migration has a confirmed substrate parity prerequisite: signaling NaN bits `7ff0040000000000` serialize as `fb7ff0040000000000` originally and `f97c01` through the pinned native CBOR codec. The existing preparation control passed while capturing seven complete original scope, numeric-range, precision-range and program records plus the actual preparation digest. Finite half, single and double values, both zeros and the quiet-NaN case matched. No native replacement, state rejection or local float codec was installed. A substrate API change needs separate cross-repository authority.

The complete correspondence handle commitment can proceed independently: its full recursive formula, literal, law, path, ownership and projection-support closure has no raw float fields. The complete writer and original capture are source-reviewed; original runtime capture is in progress. Persistence remains on its established binary codec. No tiny child issues were created.

Full CI remains waived under the current campaign instruction. No local optimized producer, canonical corpus or corpus-fixture construction, sync or full gate was run. Dependency removal, remaining owners and full parent acceptance remain open.


