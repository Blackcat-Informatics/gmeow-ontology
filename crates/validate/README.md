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
validation-path lints. As of **EPIC #575 / issue #579** it carries the two
lowest-risk lints — per-file Turtle syntax checking and the Principle 5
`owl:sameAs`-to-external-entity ban — proving the Rust↔Python validation seam
end-to-end. Further lints migrate here in subsequent tasks.

This crate is **native-only** and carries **no architecture cfg guards**: a
capability cfg would be optionality, not compliance. The engine core
(`store.rs`, `model.rs`) is deliberately **PyO3-free** — it links as a plain
`rlib` into a future Rust compiler without any Python dependency. Only `py.rs`
imports pyo3.

The Python `gmeow_validate` extension (PyO3/maturin) exposes:

- `check_syntax(paths)` → `{"errors": [...], "warnings": [...]}`
- `check_sameas_ban(paths, namespace, allowlist)` → `{"errors": [...], "warnings": [...]}`

---

## Build

> **Toolchain requirement:** nightly Rust is required. The repo ships a
> `rust-toolchain.toml` at the root that pins the channel to `nightly`;
> `cargo` and `rustup` pick this up automatically. This crate does NOT link
> Nemo, so the `crates/logic` zstd/TMPDIR build workarounds are not needed.

```bash
cargo build -p gmeow-validate
```

Via the Makefile:

```bash
make validate-build
```

---

## Test

```bash
cargo test -p gmeow-validate
```

Via the Makefile:

```bash
make validate-test
```

---

## Python extension

```bash
make validate-py
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
suite. See EPIC #575 for the full roadmap.

Related packages:

- `gmeow-shacl` — Rust SHACL Core validator
- `gmeow-logic` — world-indexed reasoning engine (Rust)
- `gmeow-gts` — Graph Transport Substrate format engine (Rust)
- Python oracle: `src/gmeow_tools/` (PyPI: `gmeow`)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **GNU Affero General Public License v3.0 only**
(AGPL-3.0-only) — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root. Separate proprietary/commercial terms are available;
contact `licensing@blackcatinformatics.ca`.
