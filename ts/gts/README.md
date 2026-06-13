<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

# `@blackcatinformatics/gmeow-gts` — TypeScript GTS Engine

[![npm version](https://img.shields.io/npm/v/@blackcatinformatics/gmeow-gts.svg)](https://www.npmjs.com/package/@blackcatinformatics/gmeow-gts)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`@blackcatinformatics/gmeow-gts` is the TypeScript/npm implementation of the **Graph
Transport Substrate (GTS)** — a single-file, language-independent transport for an
**RDF 1.2** graph (statements *and* statement-level metadata) together with any
content-addressed binary the graph references. It is one of four independent engines
(Python, Rust, Go, TypeScript) that gate against the same frozen, language-neutral conformance corpus.

This package provides a library and a command-line tool for reading, writing, verifying,
composing, compacting, and projecting GTS files. It is designed for agents and systems that
need **portable, auditable, content-addressed memory** — belief revision as suppression
frames rather than destructive edits.

---

## Table of Contents

- [What is GTS?](#what-is-gts)
- [What this package provides](#what-this-package-provides)
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

## What this package provides

- **`reader`** — read a GTS byte buffer into a `Graph`, verify chains, detect torn appends,
  and handle opaque/degraded frames.
- **`writer`** — write segments and full GTS files.
- **`compact`** — compact a streamable GTS segment into a self-contained one.
- **`files`** — pack and unpack directory trees using the GTS files profile.
- **`nquads`** — project a folded graph to N-Quads.
- **`stream`** — stream-vocabulary constants and helpers.
- **`gts` binary** — a CLI for all of the above.

The package gates against the identical frozen conformance corpus used by the Python and
Rust engines; every engine must fold identical bytes to identical expectations.

---

## Installation

### As a command-line tool

```bash
npx @blackcatinformatics/gmeow-gts info example.gts
```

Or install globally:

```bash
npm install -g @blackcatinformatics/gmeow-gts
gts info example.gts
```

The installed binary is named `gts` for ergonomics, even though the package is
`@blackcatinformatics/gmeow-gts`.

### As a library

```bash
npm install @blackcatinformatics/gmeow-gts
```

Requires Node.js ≥ 22.16.0.

---

## Quick start

```bash
# Inspect a GTS file
npx @blackcatinformatics/gmeow-gts info example.gts

# Fold it to N-Quads
npx @blackcatinformatics/gmeow-gts fold example.gts > example.nq

# Verify chain integrity
npx @blackcatinformatics/gmeow-gts verify example.gts

# Compose two valid files
npx @blackcatinformatics/gmeow-gts cat -o combined.gts a.gts b.gts

# Package a directory
npx @blackcatinformatics/gmeow-gts pack ./my-dir -o archive.gts

# Extract it elsewhere
npx @blackcatinformatics/gmeow-gts unpack archive.gts -C ./restore
```

---

## Library API

Read a GTS file and project it to N-Quads:

```typescript
import { read } from "@blackcatinformatics/gmeow-gts";
import { readFileSync } from "node:fs";

const bytes = readFileSync("example.gts");
const graph = read(bytes);
console.log(graph.toNQuads());
```

Write a minimal graph:

```typescript
import { Graph, Term, TermKind, Writer } from "@blackcatinformatics/gmeow-gts";
import { writeFileSync } from "node:fs";

const graph = new Graph();
graph.triples.push([
  new Term(TermKind.Iri, "https://example.org/s"),
  new Term(TermKind.Iri, "https://example.org/p"),
  new Term(TermKind.Iri, "https://example.org/o"),
]);

const writer = new Writer();
writer.writeSegment(graph);
writeFileSync("example.gts", writer.finish());
```

For the full API, explore the TypeScript declarations in `dist/index.d.ts` after building,
or browse the source under [`src/`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/ts/gts/src).

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
cd ts/gts
npm ci
npm run build
npm run lint
npm test
```

The conformance tests compare this engine's output against the frozen corpus in
`generated/gts-vectors/`.

---

## Project and community

`@blackcatinformatics/gmeow-gts` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite.

Related packages and engines:

- Python: [`packages/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/packages/gts) (PyPI: `gmeow-gts`)
- Rust: [`crates/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/crates/gts) (`gmeow-gts`)
- Go: [`go/gts`](https://github.com/Blackcat-Informatics/gmeow-ontology/tree/main/go/gts)

---

## Contributing

Contributions are welcome. Please read the repository
[`AGENTS.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/AGENTS.md)
for the development workflow, and [`CONSTITUTION.md`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md)
for the design principles that guide every change.

All changes must pass `npm run lint` and `npm test`.

---

## Support

- Open an issue: https://github.com/Blackcat-Informatics/gmeow-ontology/issues
- Discussions: https://github.com/Blackcat-Informatics/gmeow-ontology/discussions

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This package is licensed under the **Apache License, Version 2.0** — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root.

The GMEOW ontology and vocabulary are dual-licensed under **CC BY 4.0** and **Apache-2.0**;
see [`LICENSE-ontology`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE-ontology).
