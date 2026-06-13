<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

# `gmeow-gts` — Rust GTS Engine

[![crates.io](https://img.shields.io/crates/v/gmeow-gts.svg)](https://crates.io/crates/gmeow-gts)
[![docs.rs](https://docs.rs/gmeow-gts/badge.svg)](https://docs.rs/gmeow-gts)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`gmeow-gts` is the Rust implementation of the **Graph Transport Substrate (GTS)** — a
single-file, language-independent transport for an **RDF 1.2** graph (statements *and*
statement-level metadata) together with any content-addressed binary the graph references.
It is one of four independent engines (Python, Rust, Go, TypeScript) that gate against the same frozen,
language-neutral conformance corpus.

This crate provides a library and a command-line tool for reading, writing, verifying,
composing, compacting, and projecting GTS files. It is designed for agents and systems that
need **portable, auditable, content-addressed memory** — belief revision as suppression
frames rather than destructive edits.

---

## Table of Contents

- [What is GTS?](#what-is-gts)
- [What this crate provides](#what-this-crate-provides)
- [Installation](#installation)
  - [As a command-line tool](#as-a-command-line-tool)
  - [As a library](#as-a-library)
- [Quick start](#quick-start)
- [Library API](#library-api)
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
- **Signable and verifiable.** Segments can carry seals and provenance metadata.
- **Language-independent.** The same file can be read and written by the Python, Rust, and
  Go engines.

For the authoritative specification, see [`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/GTS-SPEC.md).
For the high-level rationale, see [`docs/RATIONALE.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/RATIONALE.md).

GTS is part of the larger [GMEOW](https://github.com/Blackcat-Informatics/gmeow-ontology)
project — a reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary for
modelling people, organizations, documents, agreements, contacts, observations, and
contested facts.

---

## What this crate provides

- **`gmeow_gts::reader`** — read a GTS byte slice into a `Graph`, verify chains, detect
  torn appends, and handle opaque/degraded frames.
- **`gmeow_gts::writer`** — write segments and full GTS files.
- **`gmeow_gts::compact`** — compact a streamable GTS segment into a self-contained one.
- **`gmeow_gts::files`** — pack and unpack directory trees using the GTS files profile.
- **`gmeow_gts::nquads`** — project a folded graph to N-Quads.
- **`gmeow_gts::stream`** — stream-vocabulary constants and helpers.
- **`gts` binary** — a CLI for all of the above.

The crate gates against the identical frozen conformance corpus used by the Python and Go
engines; every engine must fold identical bytes to identical expectations.

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
gmeow-gts = "0.1.1"
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

Read a GTS file and project it to N-Quads:

```rust
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read("example.gts")?;
    let graph = gmeow_gts::reader::read(&bytes)?;
    println!("{}", gmeow_gts::nquads::to_nquads(&graph));
    Ok(())
}
```

Write a minimal graph:

```rust
use gmeow_gts::model::{Graph, Term, TermKind};
use gmeow_gts::writer::Writer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = Graph::default();
    graph.triples.push((
        Term { kind: TermKind::Iri, value: "https://example.org/s".into() },
        Term { kind: TermKind::Iri, value: "https://example.org/p".into() },
        Term { kind: TermKind::Iri, value: "https://example.org/o".into() },
    ));

    let mut writer = Writer::new();
    writer.write_segment(&graph, None)?;
    let bytes = writer.finish()?;
    std::fs::write("example.gts", bytes)?;
    Ok(())
}
```

For the full API, see [docs.rs/gmeow-gts](https://docs.rs/gmeow-gts).

---

## Command-line reference

```text
gts info <file>...            per-segment composition ledger
gts fold <file>               fold to N-Quads on stdout
gts verify <file>...          verify chains; exit 1 on any diagnostic
gts cat -o <out> <file>...    validating composer: refuse degenerate inputs,
                              then byte-concatenate
gts ls <file>...              list segment digests, sizes, and media types
gts pack <dir> -o <out>       package a directory into a GTS files profile
gts unpack <file> -C <dir>    extract a files profile
gts compact <file>            compact a streamable GTS segment
gts diff <file> <directory>   compare a files profile to a directory
```

Exit codes:

- `0` — success / clean
- `1` — diagnostics or input refused
- `2` — usage or IO error

`cat` output is raw byte concatenation: validation is added, transformation never. It
refuses dirty inputs, contributes-nothing segments, and compositions whose suppressions
hide every folded quad.

---

## The GTS file format

A GTS file is a CBOR Sequence (`application/cbor-seq`) of one or more segments. Each
segment begins with a header map containing at least:

- `"gts"` — the magic string `"GTS1"`.
- `"t"` — an RFC 3339 timestamp for the segment.
- `"f"` — an array of frames, each a `[digest, content]` pair.

Frame digests are BLAKE3 hashes of the frame content. Chaining is implicit: the fold of a
segment is the BLAKE3 hash of the concatenation of all frame contents. External binaries are
referenced by content-id and can be omitted (opaque/degraded) without invalidating the file.

For full details, read [`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/GTS-SPEC.md).

---

## Developer documentation

- [GTS Specification](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/GTS-SPEC.md)
- [GTS Reference Guide](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/gts-reference.md)
- [GTS Narrow Waist](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/gts-narrow-waist.md)
- [Engine Cross-check](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/engine-crosscheck.md)
- [Project Rationale](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/RATIONALE.md)
- [GMEOW Constitution (design principles)](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md)
- [Repository `AGENTS.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/AGENTS.md)

### Building and testing locally

```bash
cd crates/gts
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The conformance tests compare this engine's output against the frozen corpus in
`generated/gts-vectors/`.

---

## Project and community

`gmeow-gts` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite.

Related packages and engines:

- Python: [`packages/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/packages/gts) (PyPI: `gmeow-gts`)
- Go: [`go/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/go/gts)
- TypeScript/npm: [`ts/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/ts/gts) (`@blackcatinformatics/gmeow-gts`)

---

## Contributing

Contributions are welcome. Please read the repository
[`AGENTS.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/AGENTS.md)
for the development workflow, and [`CONSTITUTION.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md)
for the design principles that guide every change.

All changes must pass `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

---

## Support

- Open an issue: https://github.com/Blackcat-Informatics/gmeow-ontology/issues
- Discussions: https://github.com/Blackcat-Informatics/gmeow-ontology/discussions

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **Apache License, Version 2.0** — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root.

The GMEOW ontology and vocabulary are dual-licensed under **CC BY 4.0** and **Apache-2.0**;
see [`LICENSE-ontology`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE-ontology).
