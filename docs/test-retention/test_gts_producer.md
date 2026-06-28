# Retention: `tests/test_gts_producer.py` (+ `test_gts_views.py`)

**Category:** Python tool algorithm (GTS shims)

## What it tests

`test_gts_producer.py`: the RDF→GTS producer (`compile_gts`, `gts_from_graph`)
wire contract — frame codecs, blob metadata, additive report blobs — and the
`to_duckdb` / `to_sqlite` database shims and S3 artifact recovery.
`test_gts_views.py`: the `FoldView` read API (`load_fold`, term accessors, scoped
quad access, RDF-list walks, snapshot scopes, the public-text path) over the
committed snapshot.

## Why it cannot move to Rust today

GTS itself is Rust (`gmeow-gts`), but these exercise live **Python** shims in
`gmeow_tools.gts_producer` / `gmeow_tools.gts_views`: the `compile_gts` seam, the
duckdb/sqlite serializers, and the `FoldView` Python read API. The assertions are
about those Python wrappers' computed output, which no Rust crate test covers.

## What is needed to move it to Rust

Move the compile_gts wire contract + the duckdb/sqlite shims and the `FoldView`
read API into the `gts` Rust crate, covered by crate tests (the MCP read-surface
`McpView` in `crates/pipeline` is the pattern). Then delete both files and this
dossier.
