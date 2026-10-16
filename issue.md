# Issue #1825: build: reconcile producer input selection with compiled docs-print sources

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

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


## Comments (1)

### paudley — 2026-10-16T16:28:01Z

Source inspection narrows the CI failure: doc_render.rs is a live module declared
by docs-print/src/lib.rs and called by render.rs. It must not be deleted.

In crates/xtask/src/producer.rs, resolve_source_selection normalizes package IDs
through metadata_package_ids / explicit_package_id. The compiler-artifact and
build-script-executed branches of build_producer instead use raw Cargo package_id
strings with workspace path substitution only. Thus a Cargo ID whose fragment is
only the version cannot match the explicit name@version selected unit identity.
The artifact filter silently omits the selected package, then the final read seal
correctly refuses its source. This is a source-supported cause to reproduce with
synthetic Cargo metadata/messages; the remote compiler-artifact transcript itself
was not uploaded, so that exact raw message remains unavailable.

Normalize artifact and generated-input owners using the same exact metadata map,
reject unknown package IDs, and preserve the selected-unit and compiler-read seals.
Regression coverage must include implicit workspace identities, generated owners,
explicit IDs, unknown/contradictory IDs and unselected artifacts. No CPU or
environment fingerprint is needed to test message identity binding.


