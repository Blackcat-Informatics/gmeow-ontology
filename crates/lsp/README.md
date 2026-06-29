<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-lsp

`gmeow-lsp` is the native editor-diagnostics surface for GMEOW source files. It
serves synchronous LSP diagnostics for `.ttl` and `.logic` files and also exposes
a SARIF-emitting CLI mode for review/report workflows.

## Source Map

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Shared analysis core used by the binary and tests. |
| `src/main.rs` | `gmeow-lsp` executable: LSP stdio mode plus the `sarif` subcommand. |
| `tests/` and snapshots | Golden diagnostics and SARIF rendering coverage. |

The crate depends on `gmeow-diagnostics` for user-facing finding shapes,
`gmeow-logic-compile` for `.logic` parsing, and `gmeow-logic` for runtime-side
diagnostic reports. It should stay a thin delivery surface over those shared
diagnostic contracts.

## Local Checks

```bash
make lsp-build
make lsp-sarif
make diagnostics-rust-sarif
```

Use `make lsp-release` when a release build must be staged under `dist/bin/`.
