# gmeow-errors

`gmeow-errors` is the Rust-owned diagnostics core for GMEOW developer
tooling. It defines the canonical `Finding` and `Report` model used by Python
commands to emit human text, JSON, SARIF 2.1.0, and static HTML reports.

The engine model and renderers are PyO3-free. Python bindings live in
`src/py.rs` and expose the `gmeow_diagnostics` extension module for
`gmeow_tools.validate`.

## Projections

The canonical `Report` is the single source of truth — it must report on a graph
too broken to parse, so it lives in Rust, not RDF. Every user-facing format is a
projection of it:

- **SARIF 2.1.0** (`render::to_sarif`) — GitHub-navigable: `runs[].artifacts`,
  stable `partialFingerprints` (FNV-1a over the deterministic `Finding::sort_key`,
  so code-scanning dedupes across runs), and `logicalLocations` + `properties`
  carrying GTS wire coordinates (`gts:term`/`gts:quad`/`gts:reifier`/`gts:frame`/
  `gts:segment`) so a result resolves to a position *inside* a `.gts` bundle.
- **`gmeow:` RDF** (`render::to_gmeow_rdf`) — the native in-bundle form: each
  finding as a `gmeow:Finding` (the `slices/core/diagnostics` vocabulary, a
  `gufo:SubKind` of `gmeow:Observation`) in the `gmeow:graph/diagnostics` named
  graph, emitted as N-Quads, SPARQL-queryable beside the data it describes.
- **Flat JSON, static HTML, coloured CLI** — unchanged.

## Wire coordinates

`Location` mirrors `purrdf::RdfLocation`'s GTS wire coordinates. `Diag::from_rdf`
(here) carries them from the RDF diagnostics model, and the
`gmeow-validate::findings` bridge (`finding_from_shacl`) carries SHACL focus nodes,
into the single `Report`, so all projections anchor a diagnostic to the same bundle
position.

## Self-describing feedback bundle

`gmeow-dev feedback` always writes `dist/gmeow-feedback.gts` (via
`crates/gmeow-dev-cli/src/feedback_bundle.rs`): the findings RDF as the snapshot
graph plus the SARIF and JSON projections as content-addressed blobs. The
snapshot content id is stamped into the report metadata as a self-attestation
(`verify_feedback_bundle`). The committed `gmeow.gts` never carries a report —
only the feedback bundle does (artifact separation, no flag).
