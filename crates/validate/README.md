<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# `gmeow-validate` — Rust Validation-Path Lints

[![crates.io](https://img.shields.io/crates/v/gmeow-validate.svg)](https://crates.io/crates/gmeow-validate)
[![docs.rs](https://docs.rs/gmeow-validate/badge.svg)](https://docs.rs/gmeow-validate)
[![License](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`gmeow-validate` is an oxigraph-backed Rust crate that hosts the GMEOW
validation-path lints. As of **that effort** it carries the two
lowest-risk lints — per-file Turtle syntax checking and the Principle 5
`owl:sameAs`-to-external-entity ban — proving the Rust↔Python validation seam
end-to-end. Further lints migrate here in subsequent tasks.

This crate is **native-only** and carries **no architecture cfg guards**: a
capability cfg would be optionality, not compliance. The engine core
(`store.rs`, `model.rs`) links as a plain `rlib` into the native Rust
toolchain.

---

## Build

> **Toolchain requirement:** nightly Rust is required. The repo ships a
> `rust-toolchain.toml` at the root that selects the latest available `nightly`;
> `cargo` and `rustup` pick this up automatically. This crate does NOT link
> the full reasoning stack, so its heavier build workarounds are not needed.

```bash
cargo build -p gmeow-validate
```

## Test

```bash
cargo test -p gmeow-validate
```

---

## GTS inputs and content-addressed cache

The Rust-native orchestration in `ValidateOptions` supports two ways to supply
the ontology:

- a list of source Turtle files (`source_paths`), or
- a pre-built Graph Transport Substrate bundle passed as `ValidateOptions::gts_bytes`.

When `gts_bytes` is provided, the orchestration builds the shared oxigraph store
from the bundle and skips the per-file Turtle phases (syntax check and the
`owl:sameAs` external-entity ban), because those lints are meaningless for an
already-materialized GTS graph.

If both `source_paths` and `gts_bytes` are supplied, `gts_bytes` takes
precedence: the store is built from the bundle, the per-file Turtle phases are
skipped, and `source_paths` is not inspected.

If `ValidateOptions::project_root` is set, validation results are cached under
`<project_root>/.cache/validate/<kind>/<key>.json`. The cache is purely
content-addressed: there is no TTL, and a changed input produces a new key.

Cache-key composition differs by input kind:

- **GTS inputs:** the merged-SHACL key is derived from the `gmeow_gts::wire`
  segment-head content IDs (BLAKE3 hashes), not from raw bundle bytes. Folding
  the same logical graph through different bundle encodings therefore hits the
  same cache entry.
- **Non-GTS inputs:** the key is derived from source-file paths, mtime size, and
  raw content (SHA-256), matching the legacy Python `generator.source_hash`
  behavior.

In both cases the final key also mixes a toolchain salt that includes the
versions of `gmeow-validate`, `gmeow-shacl`, and the `gmeow-gts` wire format, so
upgrading any of those crates automatically invalidates prior cached results.

---

## Python extension

```bash
make native-py
```

```python
import gmeow_validate

report = gmeow_validate.check_syntax(["a.ttl", "b.ttl"])
print(report["errors"])    # list of "syntax error in ..." strings
```

---

## Project and community

`gmeow-validate` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite. See the issue tracker for the full roadmap.

Related packages:

- `gmeow-shacl` — Rust SHACL Core validator
- `gmeow-logic` — world-indexed reasoning engine (Rust)
- `gmeow-gts` — Graph Transport Substrate format engine (Rust)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **GNU Affero General Public License v3.0 only**
(AGPL-3.0-only) — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root. Separate proprietary/commercial terms are available;
contact `licensing@blackcatinformatics.ca`.
