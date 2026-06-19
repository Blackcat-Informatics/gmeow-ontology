<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: MIT OR Apache-2.0
-->
<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-gts">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-gts/main/docs/gts-logo.svg" alt="GTS logo" width="128" height="128">
  </a>
</p>

# `gmeow-gts` — Rust GTS Engine

[![CI](https://github.com/Blackcat-Informatics/gmeow-gts/actions/workflows/ci.yml/badge.svg)](https://github.com/Blackcat-Informatics/gmeow-gts/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/gmeow-gts.svg)](https://crates.io/crates/gmeow-gts)
[![docs.rs](https://docs.rs/gmeow-gts/badge.svg)](https://docs.rs/gmeow-gts)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/LICENSING.md)

> **A whole graph in a single, verifiable file.**

`gmeow-gts` is the Rust implementation of the **Graph Transport Substrate (GTS)** — a
single-file, language-independent transport for an **RDF 1.2** graph (statements *and*
statement-level metadata) together with any content-addressed binary the graph references.
It is one of four interoperable engines (Python, Rust, Go, TypeScript) that gate against the
same frozen, language-neutral conformance corpus.

This crate provides a library and a command-line tool for reading, writing, verifying,
composing, compacting, and projecting GTS files — with optional COSE signing and encryption.
It is designed for systems that need portable, auditable, content-addressed graph packages:
archives, evidence chains, local-first synchronization, dataset distribution, GMEOW packages,
and agent memory.

---

## Table of Contents

- [What is GTS?](#what-is-gts)
- [What this crate provides](#what-this-crate-provides)
- [Installation](#installation)
  - [As a command-line tool](#as-a-command-line-tool)
  - [As a library](#as-a-library)
- [Quick start](#quick-start)
- [Library API](#library-api)
- [Example: grounded agent memory](#example-grounded-agent-memory)
- [Command-line reference](#command-line-reference)
- [The GTS file format](#the-gts-file-format)
- [Developer documentation](#developer-documentation)
- [Project and community](#project-and-community)
- [Contributing](#contributing)
- [Support](#support)
- [License and copyright](#license-and-copyright)

---

## What is GTS?

GTS is the **Graph Transport Substrate**: a CBOR-sequence, append-only, content-addressed
file format for moving RDF 1.2 graphs and their evidence around. A GTS file is composed of
one or more **segments**, each a header followed by frames chained by BLAKE3 content-id.
Key properties:

- **Append-only and composable.** Concatenating valid GTS files (`cat`) yields a valid GTS
  file whose fold is the value-union of the segment graphs.
- **Content-addressed.** Frames and external binaries are referenced by BLAKE3 digests.
- **Signable and verifiable.** Frames can carry detached COSE_Sign1 signatures, and segments
  carry provenance metadata.
- **Language-independent.** The same file can be read and written by the Python, Rust, Go, and
  TypeScript engines.

For the authoritative specification, see [`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md).
For the reference-implementation guide, see [`docs/gts-reference.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/gts-reference.md).

GTS is ontology-independent. GTS is the primary distribution method for
[GMEOW](https://github.com/Blackcat-Informatics/gmeow-ontology), but GTS does not depend on
GMEOW. A reader does not need GMEOW vocabulary or tooling to parse, verify, fold, or transport
a GTS file.

---

## What this crate provides

- **`gmeow_gts::reader`** — read a GTS byte slice into a `Graph`, verify chains, detect
  torn appends, decrypt `COSE_Encrypt0` frames when a content-key resolver is supplied, and
  handle opaque/degraded frames.
- **`gmeow_gts::writer`** — build frames and emit full GTS files, including
  `Writer::deterministic(&graph, profile)` for reproducible graph authoring and
  `FrameOptions` for transformed, encrypted, explicitly signed, or recipient-addressed frames.
- **`gmeow_gts::cose`** — COSE_Sign1 signing and verification of frame ids (§9.2), plus
  COSE_Encrypt0 AES-256-GCM payload encryption (§9.3).
- **`gmeow_gts::openpgp`** — parse an embedded OpenPGP transport key to its fingerprint.
- **`gmeow_gts::verify`** — high-level embedded-key verification results for libraries,
  including fingerprint, visual hashes, signature counts, diagnostics, and profile findings.
- **`gmeow_gts::compact`** — compact a streamable GTS segment into a self-contained one.
- **`gmeow_gts::files`** — pack and unpack directory trees using the GTS files profile.
- **`gmeow_gts::nquads`** — project a folded graph to N-Quads.
- **`gmeow_gts::model::Graph`** — consume raw quad-id rows with `into_quads()` or
  lazily resolve them with `quad_terms()`.
- **`gmeow_gts::rdf`** — optional `--features rdf` native adapter for
  `oxrdf::Dataset` without an embedded graph-store dependency.
- **`gmeow_gts::oxigraph`** — optional `--features oxigraph-adapter` bridge between
  folded GTS graphs and Oxigraph's in-memory `Store`, with GTS metadata kept in a sidecar.
- **`gmeow_gts::examples::agent_memory`** — a dependency-light grounded-memory
  example built on ordinary GTS frames.
- **`gmeow_gts::stream`** — stream-vocabulary constants and helpers.
- **`gmeow_gts::emojihash`** — re-export of the [`visual-hashing`](https://crates.io/crates/visual-hashing)
  crate's `emojihash` and `randomart` key fingerprints.
- **`gts` binary** — a CLI for all of the above.

The crate gates against the identical frozen conformance corpus used by the Python, Go, and
TypeScript engines; every engine must fold identical bytes to identical expectations.

---

## Installation

### As a command-line tool

```bash
cargo install gmeow-gts
```

The installed binary is named `gts` for ergonomics, even though the crate is `gmeow-gts`.

### As a library

Add to `Cargo.toml`:

```toml
[dependencies]
gmeow-gts = "0.9.0"
```

Verify a signed file with its embedded transport key:

```rust
let data = std::fs::read("signed.gts")?;
let result = gmeow_gts::verify::verify_file(&data);
assert!(result.ok, "{:?}", result.errors);
println!("{}", result.fingerprint.unwrap_or_default());
```

Sign new frames with either raw Ed25519 bytes or an unencrypted Ed25519
OpenPGP secret-key block:

```rust
use ed25519_dalek::SigningKey;
use gmeow_gts::writer::Writer;

let seed = [0u8; 32];
let mut raw = Writer::new("evidence");
raw.sign_with(SigningKey::from_bytes(&seed), "did:example:raw-key");

let armored = std::fs::read_to_string("transport.sec.asc")?;
let mut openpgp = Writer::new("evidence");
// `None` uses the OpenPGP v4 fingerprint as the COSE key id.
openpgp.sign_with_openpgp_secret_key(&armored, None)?;
```

---

## Quick start

```bash
# Inspect a GTS file
gts info example.gts

# Fold it to N-Quads
gts fold example.gts > example.nq

# Verify chain integrity
gts verify example.gts

# Compose two valid files
gts cat -o combined.gts a.gts b.gts

# Package a directory
gts pack ./my-dir -o archive.gts

# Extract it elsewhere
gts unpack archive.gts -C ./restore
```

---

## Library API

Read a GTS file and project it to N-Quads. `reader::read` is **total**: it never returns an
error — a damaged or undecodable frame degrades to an opaque node and surfaces as a diagnostic
on the returned `Graph`, so it always yields a usable (possibly partial) fold. The arguments are
`(data, allow_segments, expected_head)`:

```rust
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read("example.gts")?;
    let graph = gmeow_gts::reader::read(&bytes, false, None);
    println!("{}", gmeow_gts::nquads::to_nquads(&graph));
    Ok(())
}
```

Inline blob entries are content-addressed by digest and may remain transformed on the wire until
they are read. Use `Graph::blob_entry` to inspect presence without decoding, `Graph::blob_bytes`
or `Graph::blob_bytes_cloned` to decode and cache a single blob, and `Graph::decoded_blobs` to
materialize the insertion-ordered table.

Readers that hold content keys can use `ReadOptions::with_content_key` to decrypt
`COSE_Encrypt0` frames during the same total fold. Missing or wrong keys still degrade to
opaque nodes with `MissingKey` diagnostics instead of aborting the read:

```rust
let key = [7u8; 32];
let resolver = |kid: &str| (kid == "did:example:recipient").then_some(key);
let graph = gmeow_gts::reader::read_with_options(
    &bytes,
    gmeow_gts::reader::ReadOptions::new(false, None).with_content_key(&resolver),
);
```

Write a minimal graph. A `Writer` is created with a profile (e.g. `"dist"`), then frames are
appended: a `terms` frame interns the RDF terms by append-order id, and a `quads` frame
references them by those ids (the fourth tuple slot is the graph name, `None` for the default
graph):

```rust
use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::writer::Writer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Cat".into()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("http://www.w3.org/2000/01/rdf-schema#label".into()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("Cat".into()),
            datatype: None,
            lang: Some("en".into()),
            reifier: None,
        },
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    std::fs::write("cat.gts", writer.to_bytes())?;
    Ok(())
}
```

Advanced frame authorship uses `FrameOptions`, matching the Python writer's transform and
encryption surface without adding new Rust dependencies:

```rust
use ciborium::value::Value;
use gmeow_gts::writer::{Encrypt0Options, FrameOptions, Writer};

let key = [7u8; 32];
let mut writer = Writer::new("opaque");
writer.add_frame_with_options(
    "meta",
    FrameOptions {
        payload: Some(Value::Map(vec![("private".into(), Value::Bool(true))])),
        transform: vec!["zstd".into()],
        encrypt: Some(Encrypt0Options {
            kid: "did:example:recipient".into(),
            key,
        }),
        ..FrameOptions::default()
    },
)?;
```

For the full API, see [docs.rs/gmeow-gts](https://docs.rs/gmeow-gts).

---

## Example: grounded agent memory

The crate ships a small agent-memory example that mirrors the Python
`gts.examples.agent_memory` workflow without adding a database, SPARQL engine,
embedding model, or RDF toolkit dependency. It stores claims as reified RDF 1.2
statements, records tool-call provenance as ordinary quads, and revises claims
by appending suppressions plus `gmeow:wasDerivedFrom` audit links. The package
stays append-only and `gts verify`-able after every write.

Run the example:

```bash
cargo run --example agent_memory
```

Use it as a library example:

```rust
use gmeow_gts::examples::agent_memory::{
    Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
};

fn demo() -> Result<(), Box<dyn std::error::Error>> {
    let mem = Memory::new("assistant.gts");
    let old = mem.store(
        "Synthetic rover records battery telemetry in UTC",
        StoreOptions {
            source: Some("synthetic bench run 001"),
            confidence: Some(0.8),
            according_to: Some("example-agent"),
        },
    )?;
    let new = mem.store(
        "Synthetic rover records battery and thermal telemetry in UTC",
        StoreOptions {
            source: Some("synthetic bench run 002"),
            confidence: Some(0.9),
            according_to: Some("example-agent"),
        },
    )?;
    mem.record_tool_call(
        "urn:gmeow:tool:synthetic-search",
        ToolCallOptions {
            arguments: Some("{\"query\":\"battery telemetry\"}"),
            result: Some("matched one synthetic claim"),
            invocation: Some("urn:gmeow:invocation:demo"),
            generated: &[new.id.as_str()],
        },
    )?;
    mem.revise(
        &old.id,
        RevisionOptions {
            reason: Some("synthetic correction"),
            superseded_by: Some(&new.id),
        },
    )?;
    let current = mem.recall(RecallOptions {
        query: "battery telemetry",
        min_confidence: Some(0.5),
        ..RecallOptions::default()
    })?;
    assert!(!current.is_empty());
    assert_eq!(mem.verify()?, Vec::<String>::new());
    Ok(())
}
```

The example is a downstream application shape, not a requirement for parsing or
transporting GTS. It uses only this crate's existing writer, reader, N-Quads,
BLAKE3, and timestamp support.

---

## Command-line reference

```text
gts info <file>...              per-segment composition ledger
gts fold <file>                 fold to N-Quads on stdout
gts from-nq <in.nq> [-o out]    build a GTS from N-Quads (`-` reads stdin)
gts to-sqlite <file> <out>      export to SQLite (requires sqlite3)
gts to-duckdb <file> <out>      export to DuckDB (--features duckdb; requires duckdb)
gts to-parquet <file> <dir>     export to Parquet (--features duckdb; requires duckdb)
gts verify <file>... [--key KID:HEXPUB]
                                verify chains + COSE signatures; exit 1 on any
gts prove <file> <frame-id>      emit detached JSON proof from an index.mmr root
gts verify-proof <proof.json>    verify detached proof JSON without the GTS file
gts heads <file>                 emit JSON segment heads and aggregate comparison digest
gts segments <file>              emit JSON segment byte ranges and layout inventory
gts missing --from-head <head> <file>
                                emit JSON byte ranges needed after a peer head
gts resume --after <frame-id> <file>
                                emit bytes after a verified frame boundary
gts extract-key <file>          print the embedded transport/verification key:
                                kid, OpenPGP fingerprint, emojihash, armored key
gts ls <file>                   list inline blobs: digest, size, media type
gts extract <file> <digest>     write a single content-addressed blob
gts cat -o <out> <file>...      validating composer: refuse degenerate inputs,
                                then byte-concatenate
gts compact <file> -o <out> --streamable
                                compact into the streamable layout state
gts pack <dir|file>... -o <out> package files/directories into a files profile
gts unpack <file> [-C <dir>]    extract a files profile (refuses path traversal)
gts diff <file> <directory>     compare a files profile to a directory
```

Exit codes:

- `0` — success / clean
- `1` — diagnostics or input refused
- `2` — usage or IO error

`verify --key` and `extract-key` are cross-engine: all four `gts` binaries parse the embedded
OpenPGP transport key to the same fingerprint and emojihash, and verify COSE signatures
identically. Library callers can use `gmeow_gts::verify::verify_file` for the same embedded-key
verification summary without duplicating CLI helper code. Rust also exposes the current
MMR/proof surface through `Writer::add_index_with_mmr`, `gts prove`, and `gts verify-proof`,
and the replication surface through `gts heads`, `gts segments`, `gts missing`, and `gts resume`,
without adding runtime JSON, Merkle-tree, or replication dependencies. Rust writers can sign with raw
Ed25519 keys or `Writer::sign_with_openpgp_secret_key`, which accepts the same narrow
unencrypted Ed25519 OpenPGP secret-key shape as Python `Signer.from_gpg_secret_key` without
adding a full OpenPGP crate dependency. `from-nq` and the relational
`to-sqlite`/`to-duckdb`/`to-parquet` exports are
implemented by the Rust and Python CLIs. The Rust relational commands use the same folded
integer table model as Python. `to-sqlite` is in the default build and requires `sqlite3`
on `PATH`; `to-duckdb` and `to-parquet` require the optional no-dependency `duckdb`
Cargo feature plus the `duckdb` binary on `PATH`. Rust streams SQL rows into those tools
instead of buffering all rows or a full SQL script; lazy transformed blobs stay lazy in the
folded `Graph` and are decoded transiently only when the stable `blobs.bytes` column is
emitted.

Default Cargo features are empty. The optional `rdf` feature enables
`gmeow_gts::rdf::{to_oxrdf_dataset, from_oxrdf_dataset}` through `oxrdf`'s RDF
data-model crate. The optional `oxigraph-adapter` feature adds
`gmeow_gts::oxigraph::{graph_to_store, store_to_writer}` and
`Writer::from_store` using Oxigraph's in-memory store with Oxigraph defaults disabled.
The optional `sophia-adapter` feature adds
`gmeow_gts::sophia::{to_sophia_dataset, from_sophia_dataset}` using Sophia's
in-memory dataset plus N-Quads parser/serializer.
The optional `policy-config` feature adds JSON loading helpers for
`gmeow_gts::policy::TrustPolicy` and enables `gts verify --policy <file>` for
release/profile verification workflows. `policy-config-yaml` layers YAML parsing
on top for deployments that want YAML policy files.
The adapters and policy parsers are absent from ordinary default builds.

`cat` output is raw byte concatenation: validation is added, transformation never. It
refuses dirty inputs, contributes-nothing segments, and compositions whose suppressions
hide every folded quad.

---

## The GTS file format

A GTS file is a **CBOR Sequence** (RFC 8742, `application/cbor-seq`) of one or more
**segments** — there are no framing bytes between items. Published GTS artifacts use
`application/vnd.blackcat.gts+cbor-seq`; the `+cbor-seq` suffix records that the file is a CBOR
Sequence, not a single CBOR item. Each segment is a **Header** CBOR map (optionally preceded by
the CBOR self-describe tag `55799`, the human-recognizable magic) followed by zero or more
**frames**, each itself a CBOR map. Every frame carries its own `"id"` — the BLAKE3 content-id of
its canonical contents — and names its predecessor's id in `"prev"`, so a segment is a git-style
content-addressed chain whose head transitively commits to all history.

The logical graph is the **fold** of the log: the deterministic replay of the frames into the
in-memory tables (terms, quads, reifier bindings, annotations, blobs, …). The fold is *not* a
hash — concatenating two valid GTS files (`gts cat`) yields a file whose fold is the
**value-union** of the inputs. "Deletion" is additive **suppression**, never physical removal.

Payloads carry a stackable codec chain; an unknown codec or a held-back key degrades a frame to
an **opaque node** rather than failing the read — the reader is total. External binaries are
referenced by content-id and may be omitted without invalidating the file.

For full details, read [`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md).

---

## Developer documentation

- [GTS Specification](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-SPEC.md) — the authoritative, normative wire-format specification.
- [GTS Reference Guide](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/gts-reference.md) — the Python reference-implementation guide.
- [GTS Ecosystem Integration Contract](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/docs/GTS-ECOSYSTEM-INTEGRATIONS.md) — RDF crate adapter status and deferrals.
- [`CONTRIBUTING.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/CONTRIBUTING.md) — development workflow.
- [`CODE_OF_CONDUCT.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/CODE_OF_CONDUCT.md) — community standards.
- [`SECURITY.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/SECURITY.md) — vulnerability reporting.
- [`CHANGELOG.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/CHANGELOG.md) — release history.

### Building and testing locally

```bash
cd rust
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The conformance tests compare this engine's output against the frozen corpus in
`vectors/`.

---

## Project and community

`gmeow-gts` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca).
GMEOW is a downstream ontology and tooling suite that uses GTS as a distribution substrate.

Related packages and engines:

- Python: [`python`](https://github.com/Blackcat-Informatics/gmeow-gts/tree/main/python) (PyPI: `gmeow-gts`)
- Go: [`go`](https://github.com/Blackcat-Informatics/gmeow-gts/tree/main/go)
- TypeScript/npm: [`ts`](https://github.com/Blackcat-Informatics/gmeow-gts/tree/main/ts) (`@blackcatinformatics/gmeow-gts`)

---

## Contributing

Contributions are welcome. Please read [`CONTRIBUTING.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/CONTRIBUTING.md)
for the development workflow and [`CODE_OF_CONDUCT.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/CODE_OF_CONDUCT.md)
before opening a PR. To report a vulnerability, follow [`SECURITY.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/SECURITY.md)
(do not open a public issue).

All changes must pass `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

---

## Support

- Open an issue: https://github.com/Blackcat-Informatics/gmeow-gts/issues
- Discussions: https://github.com/Blackcat-Informatics/gmeow-gts/discussions

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

Triple-licensed: **MIT OR Apache-2.0 OR proprietary**. You may use this crate under
the terms of [MIT](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/LICENSE-MIT)
**or** [Apache-2.0](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/LICENSE-APACHE),
at your option. A separate commercial/proprietary license is also available — see
[`LICENSING.md`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/LICENSING.md).
