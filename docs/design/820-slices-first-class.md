<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# RFC: Slices as first-class, persistent, flowing compilation units

Tracking epic: [#820](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/820).
Parent: [#672](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/672)
(META-EPIC — the reference RDF-1.2 stack). **Hard dependency:
[#819](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/819)** (interned
id-graph IR) — the `SliceId` provenance column rides on `RdfGraph`. Assumes the
fully-Rust end-state ([#630](https://github.com/Blackcat-Informatics/gmeow-ontology/issues/630)).

This is a design RFC. No engine code lands with this document; the architecture is
realized through the staged children S1–S7 below.

## Context

A **slice** (`slices/<group>/<name>/`) is the atomic unit of the GMEOW ontology, and it
is a far richer object than its raw triples. Beyond the terms in `module.ttl`, a slice
carries:

- **Identity & governance** (`manifest.ttl`, the sole tier truth): IRI, `gmeow:sliceTier`
  (core/extension), `gmeow:sliceDependsOn`, `gmeow:sliceConsumer` (Principle 15),
  `gmeow:sliceProfile`, `gmeow:providesSubcommand`, `gmeow:builtAgainstCore`.
- **Citation**: `dcterms:title`, `dcterms:creator`, optional per-slice DOI → projected to
  `CITATION.cff`.
- **Validation**: SHACL `shapes.ttl`; the 3-cell test-DSL (`tests/competency.ttl`,
  `tests/structural.ttl`, `tests/example-conformance.ttl`) with per-competency reasoning
  lane (`gmeow:cqReasoning`).
- **Alignment**: `mappings/` (compiled to SSSOM).
- **Worked data**: `examples/`, `fixtures/`.
- **Queries**: `queries/competency/`, `queries/verify/` (ROBOT QC).
- **Docs**: `docs.md`.

The slice meta-ontology itself (`gmeow:Slice`, `gmeow:SliceTier`, and the predicates
above) is defined in `slices/vocabulary.ttl` and structurally validated by
`shapes/slice-manifest-shapes.ttl`.

**The engine sees almost none of this.** As slices flow into the Rust engine, their
identity and metadata are flattened away:

- `discover_slices()` (`src/gmeow_tools/slices.py`) reads the full manifest into a `Slice`
  dataclass, then `build_store` / `load_sources_into_store`
  (`crates/validate/src/store.rs:71-87`, `crates/slicetest/src/stores.rs:47`) **merges
  every `module.ttl` into one oxigraph default graph with zero slice provenance** — it is
  impossible to ask "which triples came from which slice?" at validation time.
- Only `docs.md` survives the GTS fold (as a guide blob). Tier, dependencies, creators,
  title, profiles, and consumers are **never embedded** in the bundle or the engine
  (`gts_gen.py:193-200,541`).
- GTS *segments* exist but are a size/streaming optimization; `RdfSegmentRecord`
  (`crates/rdf/src/lookaside.rs:199-207`) has **no slice field**, and there is no
  segment→slice map.
- The slice-ownership lint (`crates/validate/src/lint.rs:696-729`) derives the slice IRI
  from the **filesystem path** (not the manifest), builds a per-slice store, uses it once,
  and discards it — it never retains a `slice → terms` map.
- **No `slice` field exists anywhere** in `RdfLocation` (`crates/rdf/src/diagnostic.rs`),
  `Location`/`Finding`/`Report` (`crates/diagnostics/src/model.rs`), `ValidationResult`
  (`crates/shacl/src/report.rs`), or logic provenance
  (`crates/logic/src/provenance.rs`). A derived axiom cannot record which slices its
  premises came from; a SHACL violation cannot name its owning slice; a diagnostic cannot
  attribute itself to a slice.

**Intended outcome:** the slice becomes a first-class, persistent, flowing **compilation
unit** in the Rust engine — carrying its full payload, attributing every engine output
back to its slice, and letting the engine compile/reason/validate/project per-slice and
write derived per-slice facts back. This realizes the maximal-information-flow north star:
every product a slice carries persists through the pipeline rather than being flattened on
entry.

## Target architecture

### 1. Native slice model (`crates/slice`)

A new kernel-level crate owning the **full slice payload** as a Rust type, with native
discovery that replaces `src/gmeow_tools/slices.py`:

```text
Slice {
    iri, name, group, tier, depends_on, consumers, profiles,
    subcommands, built_against_core, title, creators, doi?,
    module:   RdfGraph-fragment,           // the terms
    shapes:   Option<ShapesPayload>,
    tests:    TestDsl { competency, structural, example_conformance },
    mappings: Vec<MappingPayload>,
    examples: Vec<ExamplePayload>,
    queries:  { competency, verify },
    docs:     Option<MarkdownBlob>,
}
```

Discovery globs `slices/*/*/manifest.ttl`, parses the manifest natively (the
`gmeow:Slice` vocabulary), and resolves the fixed slice anatomy. The slice catalogue
(`SliceId → Slice`) is built once and shared by reference across the engine.

### 2. SliceId carriage on the #819 id-graph

Extend the interned `RdfGraph` (#819) with a **slice-provenance column**:

- A `SliceId` interning table (`slices: Vec<SliceMeta>`, `HashMap<SliceIri, SliceId>`).
- A per-quad slice column `quad_slices: Vec<SliceSet>` (and, where useful, per-term).
  Attribution sources:
  - **Vocabulary triples** → the `rdfs:isDefinedBy <slice>` already authored on every term.
  - **Instance / example triples** → load-origin (which slice file they were read from).
  - **Derived triples** → the **set of contributing slices**, computed from the slice
    columns of the premises. Cross-slice derivation thereby becomes a first-class,
    queryable signal.
- An invariant (hard-fail, per no-optionality): every quad resolves to at least one
  `SliceId`; an orphan triple is an error, not a silent default.

### 3. Slice-aligned GTS segments

Make the GTS fold partition by slice so the bundle round-trips slice structure:

- One segment (or a stable segment range) per slice; `RdfSegmentRecord` gains
  `slice_iri`, and the bundle carries a `segment → slice` map.
- Per-slice lookaside metadata keyed by slice IRI (replacing today's segment-number /
  file-level keying, `crates/rdf/src/gts.rs:95-119`).
- **The full manifest payload is embedded as slice records** in the bundle — making it
  self-describing and repo-free (the "bundle is the useful surface" doctrine): governance,
  citation, tests, mappings, queries, docs all travel with the GTS.

### 4. Slice-attributed outputs

Add a `slice` channel through every output model so results attribute back to their slice:

- `RdfLocation` / `Location` gain a `slice: Option<SliceId>` (resolvable to IRI/name).
- `Finding` / `ValidationResult` name the owning slice of the focus node / shape.
- Logic provenance records the **contributing slice set** of each derivation (alongside
  the existing content-addressed reifier/derivation IRIs).
- Rendering emits `gmeow:sliceId` into the SARIF/RDF projections
  (`crates/diagnostics/src/render.rs`).

### 5. Slice as a compilation unit

The engine treats each slice as a unit it can build independently and incrementally:

- Compile / reason / validate / project **per-slice against core**.
- A **content-addressed per-slice cache** keyed on the slice's term-closure + its
  dependency set, so an unchanged slice is not recompiled.
- Incremental composition: rebuilding one slice does not force a full-ontology rebuild;
  the merged view is recomposed from cached per-slice artifacts.

### 6. Bidirectional write-back, ontology-native

The engine does not just read slices — it augments them. It computes derived per-slice
facts and persists them back, **expressed in the `gmeow:Slice` vocabulary itself** (not a
side-channel):

- The **actual** cross-slice reference graph (what the module *really* references) vs the
  *declared* `gmeow:sliceDependsOn` — e.g. a computed `gmeow:sliceActuallyReferences`.
- Per-slice **term coverage** (defined vs referenced vs exemplified).
- **Profile closures** (the dependency-closed membership the slice contributes to).
- **Cross-slice-derivation edges** surfaced from §2's contributing-slice sets.

This **replaces** the path-derived slice-ownership lint with a manifest-driven, native
check, and **subsumes the Python slice gate** (Constitution Principles 15 & 16) with a
native check that is strictly more accurate (it catches IRI/path mismatches the current
path-derivation cannot).

## Epic decomposition

| Child | Scope |
| ----- | ----- |
| **S1** | Native Rust slice model + discovery (`crates/slice`), full-payload `Slice` struct; parity with `slices.py`. |
| **S2** | `SliceId` interning + slice-provenance column on the #819 `RdfGraph`; populate from `rdfs:isDefinedBy` + load-origin; derived-triple slice-set attribution. |
| **S3** | Slice-aligned GTS segments + `RdfSegmentRecord.slice_iri` + per-slice lookaside; embed full manifest payload in the bundle. |
| **S4** | Slice-attributed outputs: `slice` field through diagnostics/SHACL/logic provenance + SARIF/RDF render. |
| **S5** | Slice-as-compilation-unit: per-slice compile/reason/validate/project + content-addressed per-slice cache + incremental composition. |
| **S6** | Bidirectional write-back: computed reference graph, coverage, profile closures, cross-slice-derivation edges in the `gmeow:Slice` vocabulary; replace path-derived ownership lint; subsume the Python slice gate. |
| **S7** | Retire Python slice plumbing (`slices.py`, `gts_gen` slice rollup, `validate_all` module_specs path) as callers move to the Rust model. |

Ordering: S1 → S2 (S2 needs #819 C1) → S3/S4 parallel → S5 → S6 → S7 (gated on the
Python cutover progressing).

## Critical files / anchors

- Slice meta-ontology & gates: `slices/vocabulary.ttl`,
  `shapes/slice-manifest-shapes.ttl`.
- Python plumbing to replace: `src/gmeow_tools/slices.py`, `gts_gen.py`,
  `gts_producer.py`, `crates/validate/src/validate_all.rs` (module_specs path).
- Carriage surfaces: `crates/rdf/src/lookaside.rs` (`RdfSegmentRecord`, `RdfLookaside`),
  `crates/rdf/src/diagnostic.rs` (`RdfLocation`), `crates/rdf/src/gts.rs`,
  `crates/rdf/src/gts_write.rs`.
- Output models: `crates/diagnostics/src/model.rs` (`Location`/`Finding`/`Report`),
  `crates/diagnostics/src/render.rs`, `crates/shacl/src/report.rs` (`ValidationResult`),
  `crates/logic/src/provenance.rs`.
- Ingestion: `crates/validate/src/store.rs`, `crates/validate/src/lint.rs`
  (`slice_ownership_lint`), `crates/slicetest/src/stores.rs`.

## Verification

1. **Provenance-completeness gate**: every quad in the merged id-graph resolves to ≥1
   `SliceId`; no orphan triples (hard-fail).
2. **Round-trip**: slice payload → GTS → reload preserves the full manifest payload and
   slice partitioning (extend `crates/rdf/tests/proptest_roundtrip.rs`).
3. **Declared-vs-computed dependencies**: the computed cross-slice reference graph (S6)
   reconciles with declared `gmeow:sliceDependsOn`, replacing the Python slice gate with a
   native check that catches IRI/path mismatches the current path-derived lint cannot.
4. **Incremental parity**: a per-slice incremental rebuild equals a full rebuild (golden
   parity); the logic derivation-graph goldens are unchanged.
5. Stay green: `make check`, `make test`.

## Doctrines honored

Maximal information flow (carry every slice product through the pipeline);
bundle-is-the-useful-surface (self-describing, repo-free bundle); greenfield /
no-backcompat (manifest-driven ownership replaces path-derivation, no fallback);
no-optionality / hard-fail (orphan triple = hard fail); one-PR-at-a-time (decomposed; lands
slice by slice).
