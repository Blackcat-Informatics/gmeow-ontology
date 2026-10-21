# Bounded transcode JSON migration

Principles 4 and 7 govern the single JSON substrate, deterministic report contracts and hard-failing static owner invariants. Refs #1776; the completed perf-evidence unit is PR1904 and the full parent remains open.

Replace serde/serde_json only in gmeow-transcode with the pinned purrdf_lex::json record/value/writer owners. Preserve RealizedLoss and MatrixEntry declaration-order members, loss-ledger row ownership, codec filtering, and the existing newline distinction (realized loss has none; matrix outputs have one). Free-form substrate matrix parsing keeps sorted last-wins objects. Infallible owner serialization removes silent empty-array failure fallbacks.

Scope: crates/transcode/Cargo.toml, src/lib.rs, src/lib.tests.rs. Add pure literal byte, ledger merge/order, filtering and refusal controls. Keep the generated-artifact drift test intact for separate producer-qualified acceptance. Preserve public String-returning callers; no downstream migration or generic JSON shim. Root owns formatting, qualification, lock resolution and delivery. Full JSON campaign remains open.
