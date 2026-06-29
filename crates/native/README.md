<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-native

`gmeow-native` builds the single unified PyO3 extension module. It folds the Rust
engine crates into one `gmeow_native` cdylib so shared pyclasses, especially the
diagnostics `Report` and `Finding` types, have one runtime identity across all
Python-facing submodules.

## Submodules

The extension registers these submodules:

| Submodule | Backing crate |
| --- | --- |
| `gmeow_native.rdf` | `gmeow-rdf` |
| `gmeow_native.diagnostics` | `gmeow-diagnostics` |
| `gmeow_native.shacl` | `gmeow-shacl` |
| `gmeow_native.validate` | `gmeow-validate` |
| `gmeow_native.logic` | `gmeow-logic` |
| `gmeow_native.slice` | `gmeow-slice` |
| `gmeow_native.docs` | `gmeow-docs` |
| `gmeow_native.pipeline` | `gmeow-pipeline` |
| `gmeow_native.foundation` | `gmeow-foundation-corpus` |
| `gmeow_native.music` | `gmeow-music` |

## Checks

```bash
make native-py
make native-py-wheel
make rust-docs
```
