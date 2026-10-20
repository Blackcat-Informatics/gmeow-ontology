# gmeow-errors

`gmeow-errors` owns live diagnostics, their causal ledger and the report
projections consumed by GMEOW tooling. `Diag` retains the original typed error
until lowering. `DiagLedger` owns the content-addressed witness DAG; serialized
`DiagNode` values support deterministic replay.

## Projections

`Finding` and `Report` project the canonical ledger into wire records. The Rust
substrate can describe failures before an RDF carrier exists. Renderers consume
the same report projection:

- **SARIF 2.1.0** (`render::to_sarif`) — GitHub-navigable: `runs[].artifacts`,
  versioned `partialFingerprints` (the canonical ledger address when supplied,
  otherwise a graph-sensitive BLAKE3 fingerprint), and `logicalLocations` + `properties`
  carrying GTS wire coordinates (`gts:term`/`gts:quad`/`gts:reifier`/`gts:frame`/
  `gts:segment`) so a result resolves to a position *inside* a `.gts` bundle.
- **`gmeow:` RDF** (`render::to_gmeow_rdf`) — the native in-bundle form: each
  finding as a `gmeow:Finding` from `slices/core/diagnostics` in the diagnostics named
  graph, emitted as N-Quads, SPARQL-queryable beside the data it describes.
- **Flat JSON, static HTML and console text** expose the same findings and causes.

## Wire coordinates

`Location` mirrors the physical, logical and GTS wire coordinates of native
`RdfLocation`. Its subject remains a separate focus, and the named source graph
has its own validated IRI field.

`Diag::from_rdf` consumes a native diagnostic directly, retaining its code,
severity, original typed cause and anchor without requiring a ledger. A caller
with an explicit GMEOW code, grade and observation uses `with_rdf_source` to attach
the native cause and anchor. More specific physical context can then be added
without rewriting the original cause. `from_rdf_with_losses` additionally attaches
each loss as a non-gating child witness to the supplied ledger.
