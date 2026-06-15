<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# `gmeow-shacl` — Rust SHACL Core Validator

[![crates.io](https://img.shields.io/crates/v/gmeow-shacl.svg)](https://crates.io/crates/gmeow-shacl)
[![docs.rs](https://docs.rs/gmeow-shacl/badge.svg)](https://docs.rs/gmeow-shacl)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`gmeow-shacl` is an oxigraph-backed SHACL Core validator for the GMEOW
ontology toolchain. It validates an RDF 1.2 data graph against a SHACL shapes
graph with no inference (parity with pySHACL `inference="none"`), using the
non-SPARQL constraint/target surface. SPARQL-based constraints and targets
arrive in issue #577.

The Python `gmeow_shacl` extension (PyO3/maturin) exposes a single
`validate(shapes_ttl, data_nt)` function that returns a dict-form SHACL
conformance report. The engine core (`engine.rs`, `shapes.rs`, `constraints.rs`,
`path.rs`, `report.rs`, `model.rs`) is deliberately **PyO3-free** — it links
as a plain `rlib` into the future Rust compiler without any Python dependency.
Only `py.rs` imports pyo3, keeping the library-first architecture intact.

This crate is gated by a SHACL conformance corpus and is part of **EPIC #575**.

---

## Build

> **Toolchain requirement:** nightly Rust is required. The repo ships a
> `rust-toolchain.toml` at the root that pins the channel to `nightly`;
> `cargo` and `rustup` pick this up automatically.

```bash
cargo build -p gmeow-shacl
```

Via the Makefile:

```bash
make shacl-build
```

---

## Test

```bash
cargo test -p gmeow-shacl
```

Via the Makefile:

```bash
make shacl-test
```

---

## Python extension

```bash
make shacl-py
```

```python
import gmeow_shacl

report = gmeow_shacl.validate(shapes_ttl="...", data_nt="...")
print(report["conforms"])  # True / False
print(report["results"])   # list of violation dicts
```

---

## Project and community

`gmeow-shacl` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite. See EPIC #575 for the full roadmap.

Related packages:

- `gmeow-logic` — world-indexed reasoning engine (Rust)
- `gmeow-gts` — Graph Transport Substrate format engine (Rust)
- Python oracle: `src/gmeow_tools/` (PyPI: `gmeow`)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **Apache License, Version 2.0** — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root.
