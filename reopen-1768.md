The distribution work has landed, but this issue's full consumer-proof contract is not complete. Reopening to keep its acceptance state accurate.

Current main fb73513b8 still reaches `load_theory` over caller paths and builds `SourceSelection::all_classical_roots` for that caller compilation in `crates/gmeow-cli/src/commands/prove.rs`. It does not load an authenticated prepared complete-canon native proof base, compose a compact caller request with that base, or demonstrate the required shared native/external execution from an installed binary. Those acceptance criteria remain open under #1770. The latest source-law increments merged in #1831 do not claim otherwise.

The existing one-file distribution evidence remains valid for its stated scope; reopening does not discard it. This also restores the state described by the last status comment, which explicitly said the issue remained open for complete-canon semantics. Principles 4, 7, 12, 17, 18 and 19 apply.
