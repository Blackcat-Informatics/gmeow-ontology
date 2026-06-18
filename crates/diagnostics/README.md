# gmeow-diagnostics

`gmeow-diagnostics` is the Rust-owned diagnostics core for GMEOW developer
tooling. It defines the canonical `Finding` and `Report` model used by Python
commands to emit human text, JSON, SARIF 2.1.0, and static HTML reports.

The engine model and renderers are PyO3-free. Python bindings live in
`src/py.rs` and expose the `gmeow_diagnostics` extension module for
`gmeow_tools.diagnostics`.
