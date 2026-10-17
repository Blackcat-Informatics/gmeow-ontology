## Observed failure

Merged main commit `07f4c6c59db06962c6a17d104f320d36359be826` fails the
producer-build job in CI run
https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38927612921/job/117063071573.
The optimized pipeline compilation completes (28m 11s), then authentication fails:

```
producer build: selected Rust implementation absent from compiler reads: crates/docs-print/src/doc_render.rs
```

Dependent producer/generation/ontology/heavy lanes are skipped. Lint passed;
rust-prebuild was still running when this failure was observed. This is a real
failed qualification, not a green merge or a runtime resource diagnosis.

## Required repair and acceptance

Reconcile authored producer input selection with the actual Rust module graph.
Preserve exact-source/recipe authentication, O3/full-LTO, assertions, overflow
checks and the single producer boundary. Do not ignore missing compiler reads or
weaken the seal. If this is an obsolete implementation, remove the obsolete path
after checking all production callers; otherwise wire its intended implementation
into the production module graph.

Exercise the source-selection/dep-info regression without production fingerprint
execution locally, then obtain passing exact-source hosted producer qualification
and downstream gate evidence. Principles 4, 7 and 18 apply. Full reasoning issue
completion remains separate from this build repair.
