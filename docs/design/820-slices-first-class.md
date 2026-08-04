<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# RFC: Slices as first-class, persistent, flowing compilation units

This RFC covers slices as first-class compilation units.
Parent: the reference RDF-1.2 stack (META-EPIC). **Coordinates with
the immutable value-interned `RdfDataset` RFC**: this RFC adds a *generic provenance sidecar* on that dataset —
it does **not** put slice semantics into the RDF kernel. Assumes the fully-Rust end-state.

This is a design RFC. No engine code lands with this document; the architecture is
realized through the staged children S0–S8 below. **S0 (a semantic RFC + crate-layering
gate) is a mandatory first child.**

> **Historical record.** The RDF-kernel crates this RFC reasons about — `gmeow-rdf-core`,
> `gmeow-rdf`, `gmeow-rdf-events` — were subsequently extracted into the sibling **`purrdf`**
> package and no longer exist in this repository; the kernel is consumed here as one exact-pinned
> dependency. The layering argument is unchanged, but the "core" it names is now an external
> boundary rather than an in-workspace crate. Current layout: [`crates/README.md`](../../crates/README.md).

## Context

A **slice** (`slices/<group>/<name>/`) is the authoring and policy unit, but current Rust
ingestion reduces slices to *paths* and merges their modules into one provenance-free
oxigraph store (`crates/validate/src/store.rs:71-87`, `crates/slicetest/src/stores.rs:47`).
Only `docs.md` survives the GTS fold; tier/deps/creators/title/profiles/consumers are
never embedded (`gts_gen.py:193-200`). `RdfSegmentRecord`
(`crates/rdf/src/lookaside.rs:199-207`) has no slice field; the slice-ownership lint
(`crates/validate/src/lint.rs:696-729`) derives the slice IRI from the *filesystem path*
and discards its per-slice store; and no `slice` channel exists in `RdfLocation`,
`Location`/`Finding`, `ValidationResult`, or logic provenance.

**The central architectural correction (from review):**

> A `SliceId` column is not, by itself, a provenance model.

The strongest form of this RFC is **not** "a `SliceId` attached to every RDF row." It is a
**content-addressed compilation-unit catalog** whose source artifacts, semantic
assertions, derivations, diagnostics, and bundle resources stay connected through the
Rust pipeline. Five concerns that a scalar column conflates must be kept distinct:

| Concern | Question it answers |
| --- | --- |
| **Source origin** | which physical unit/artifact/file asserted this occurrence? |
| **Semantic ownership** | which slice *defines* this vocabulary term (`rdfs:isDefinedBy`)? |
| **Artifact membership** | which packaged artifact (module/shape/mapping/query/…) is this? |
| **Evaluation scope** | over which graph was this result computed? |
| **Derivation support** | which rule applications / premises justify this derived fact? |

Conflating them makes diagnostics misleading and incremental deletion incorrect.

## Target architecture

### 1. Generic provenance in the RDF kernel; slice semantics layered above

`gmeow-rdf-core` is the generic RDF-1.2 narrow waist under the oxigraph adapter,
future stores, SHACL, validation, and logic. It must **not** gain a
GMEOW-specific `SliceId`. Instead it gains a neutral provenance vocabulary:

```rust
pub struct UnitId(u32);        // a compilation/source unit (opaque to the kernel)
pub struct ArtifactId(u32);    // a packaged artifact within a unit
pub struct OriginSetId(u32);   // interned set of origins (set-valued datasets)
```

The slice layer (`gmeow-slice`) interprets units:

```rust
enum UnitKind { Slice(SliceId), RootOntology, Import, Generated, RuntimeInput }
```

This is required because the real source set includes the **root ontology and imports**
beyond slice modules, and the bundle also contains statement, alignment, metadata, import,
and verification *products*. The provenance gate therefore becomes (replacing "every quad
has ≥1 SliceId"):

- Every **assertion occurrence** has exactly one `UnitId` and `ArtifactId`.
- Every **semantic quad** has at least one assertion occurrence *or* a derivation.
- Every **GMEOW vocabulary definition** has exactly one *validated* owner slice.
- No GMEOW build artifact may retain an `Unknown` origin (root/import/generated/runtime
  are explicitly representable as non-slice units).

### 2. Separate quad identity, assertion occurrence, definition ownership, derivation

The canonical `RdfDataset` is **set-valued**: the same quad authored by two slices
collapses to one `QuadId` (the GTS producer already sorts and dedups equal quads). So
identity cannot hold origin — *occurrences* must:

```text
QuadId
  ├── assertion occurrences → UnitId + ArtifactId + source Location
  └── derivations           → RuleApplication + premise FactIds
TermId
  ├── optional definition owner (a slice)
  └── usage occurrences (derived from quads)
```

```rust
struct AssertionOccurrence { quad: QuadId, unit: UnitId, artifact: ArtifactId, location: Option<LocationId> }
struct DefinitionRecord    { term: TermId, slice: SliceId, evidence: AssertionOccurrenceId }
struct RuleApplication     { rule: RuleId, premises: Box<[FactId]>, rule_unit: Option<UnitId> }
```

This encodes the facts a scalar column gets wrong: `rdfs:isDefinedBy` is an *authored
ownership declaration*, not trustworthy physical provenance (load origin is always
retained even when `isDefinedBy` is wrong); an IRI term is owned by one slice but *used* by
many; literals and external RDF terms generally have **no** owning slice; and one semantic
quad may have multiple physical files/locations (today's single-coordinate `RdfLocation`
cannot represent duplicate occurrences on its own).

### 3. The slice model is an artifact catalog, not a monolithic compiler object

`gmeow-slice` owns package structure but must **not** parse SHACL, test-DSL, mappings, and
SPARQL into one enormous `Slice` struct (that creates dependency cycles between
`gmeow-slice`, `gmeow-rdf`, `gmeow-shacl`, `gmeow-slicetest`, and the mapping compiler).
Prefer a catalog of typed-but-unparsed artifacts; consumer crates compile what they
understand:

```text
SliceCatalog → SliceRecord
    ├── typed manifest view
    ├── preserved manifest RDF graph (verbatim)
    └── ArtifactId[] { role, logical path, media type, raw digest, optional semantic digest, content ref }
```

Roles: `Manifest, Module, Shapes, Mapping, CompetencyQuery, VerifyQuery, TestDsl, Example,
CounterExample, Documentation, TranslationCatalog, Citation, Other(IRI)`. The open
`Other(IRI)` role is essential for third-party slices and forward compatibility.

### 4. Preserve the manifest as RDF, not only as known Rust fields

Today's Python loader extracts known fields into plain strings, discarding literal
language/datatype identity and all unknown triples. The Rust model retains **both** the
complete manifest graph (or canonical manifest bytes) **and** a validated typed projection.
Otherwise "full manifest payload" silently loses labels, definitions, identifiers
(the vocabulary uses `dcterms:identifier` for a DOI — absent from today's `Slice` struct),
custom publisher metadata, and future extension properties. The artifact inventory must
also include **translations** (`i18n/*.po`), which the current payload list omits.

**S0 contract discrepancy to resolve (do not freeze into Rust):** discovery accepts any
RDF literal and coerces to a plain string, while `shapes/slice-manifest-shapes.ttl`
declares fields such as `gmeow:sliceConsumer` to be `xsd:string` — yet existing manifests
use *language-tagged* consumer literals. S0 must define the authoritative manifest datatype
contract.

### 5. GTS segment alignment is a packaging optimization, not slice identity

A GTS segment is a transport/append-log scope; term IDs are segment-local, blank nodes are
segment-scoped, and the fold unions segments by RDF value. **Reject** the invariant
`one segment == one slice`. Use:

```text
segment ↔ zero or more compilation units
slice   ↔ one or more segments
```

"Slice-affine segments" can remain the canonical GMEOW packaging layout, but semantic
provenance must survive concatenation, compaction, resegmentation, a slice split across
segments, and global generated segments with no owning slice. Therefore
`RdfSegmentRecord.slice_iri` is too restrictive — use a metadata-backed `SegmentUnitMap`
(a *set*; note `gmeow:sliceProfile` is multi-valued, and the current record carries only
index/head/one-profile/streaming state).

### 6. A real bundle content store before promising full-payload round-trip

The lookaside resource model is close to the needed artifact index (kind, path, media
type, digest, graph name, metadata) but its **blob record carries no bytes** — so the
current RDF→GTS writer states blobs cannot be preserved. The shared model becomes:

```text
RdfBundle
├── dataset:    RdfDataset          // the hot graph
├── provenance: DatasetProvenance   // units, occurrences, definitions, derivations
├── units:      UnitCatalog
├── artifacts:  ArtifactIndex
└── blobs:      ContentStore        // actual bytes, content-addressed
```

An interned RDF graph alone cannot carry mappings, queries, Markdown, PO catalogs,
examples, or exact source manifests — so this is coordinated with the `RdfBundle`
(dataset + envelope) split. Replace the current project-wide **tar aggregates** (mappings,
queries, DSL cells, tests folded into shared tar blobs) with **individually indexed,
content-addressed artifacts** where possible — global archives make one changed test or
mapping invalidate and retransmit unrelated slices.

### 7. Fact provenance is an OR of rule applications, not a flattened slice set

A "contributing slice set" is only a *summary*. If an inferred fact has two proofs —
`{A,B}` and `{C}` — flattening to `{A,B,C}` cannot answer whether the fact survives
removal of A, B, or C. Exact incremental deletion needs the alternative derivations (a
truth-maintenance structure):

```text
Fact = OR( asserted-in-A, asserted-in-B, RuleApplication(AND premise₁, premise₂, …), … )
```

The displayed "contributing slices" is then a *lazily computed union of provenance leaves*.
This is a real engine capability beyond today's single immediate-antecedent
`ChaseProvenance` record, so reasoning is split: **S6a** = phase-level caching + incremental
parse/validate/project; **S6b** = exact incremental reasoning backed by complete
justification / truth maintenance. S6b's correctness must **not** depend on a flattened
set. Persistent **derivation identity stays independent of runtime IDs**: current
derivation IDs hash the rule IRI + premise reifier IRIs (`crates/logic/src/provenance.rs`);
numeric interner IDs must never enter those hashes.

### 8. Distinguish source units from execution units

A slice is an excellent *source and attribution* unit, but not always the correct isolated
*reasoner* unit: the manifest vocabulary says core slices reason as one union and
extensions reason as extension-plus-core, and competency questions deliberately run over
the full merged ontology. Use three levels:

```text
Source unit:  one slice                              (parse, lint, inventory, hash)
Link unit:    dependency strongly-connected component (reason mutually-dependent core together)
Product unit: dependency-closed profile / bundle      (validate full composition)
```

Build one reusable **core reasoning baseline**, evaluate each extension over core+extension,
validate both intrinsic slice composition and full profile composition — and **keep output
attribution at slice granularity even when execution occurs over an SCC**.

### 9. Structured attribution roles, not a scalar `slice` field

A single diagnostic can involve the slice owning the *shape*, the slice asserting the
*focus node*, the slice defining the *result path*, the slice owning the *rule*, several
slices *supporting a derivation*, and an external runtime-data unit that is not a slice.
So attribution is a structured relation, not a field:

```rust
struct Attribution { unit: UnitId, role: AttributionRole, evidence: Box<[LocationId]> }
enum AttributionRole {
    AssertionOrigin, DefinitionOwner, ShapeOwner, RuleOwner,
    FocusOrigin, ValueOrigin, DerivationSupport, EvaluationScope,
}
```

For SHACL, adding a `slice` to `ValidationResult` is inadequate: a result has focus node,
optional path/value, component, source shape — but no source-assertion or evaluation-scope
info, and a `minCount` violation has **no offending data quad at all** (its attribution
comes from evaluation scope + shape owner). At serialization: SARIF carries slice IRIs in
`properties` / logical locations; RDF links the **public slice IRI**, never the graph-local
numeric `SliceId`; use `gmeow:attributedToSlice`, `gmeow:shapeSlice`,
`gmeow:contributingSlice` rather than an ambiguous `gmeow:sliceId`. The SARIF **fingerprint
is versioned** to include canonical attribution roles and slice IRIs where they
distinguish otherwise-identical findings (today it uses only severity/code/primary
location/message).

### 10. Define the dependency algorithm before implementing write-back

The "actual cross-slice reference graph" needs a normative definition. Classify edges by
source artifact (`Ontology, Shape, Mapping, Query, Test, Example, Documentation,
Generated`). For RDF artifacts, scan **subject, predicate, object, datatype IRI, graph
name, nested triple-term components, and reifier/annotation rows**. **Parse** SPARQL and
mappings — do not text-search. Each computed edge retains evidence
(`from, to, artifact role, referenced term, source occurrence/location, dependency kind`).
Only edge kinds defined as *semantic* reconcile with `gmeow:sliceDependsOn` (a
documentation link must not silently become a build dependency). The native ownership
analyzer (replacing the path-derived lint) must: (1) discover the slice IRI **from the
manifest**, (2) attach manifest identity as load origin, (3) derive declared term ownership
from `rdfs:isDefinedBy`, (4) compare declared ownership against physical origin, (5) build
dependencies only from *validated* ownership data.

### 11. Keep computed write-back separate from authored declarations

Computed edges must **not** be written back as ordinary `gmeow:sliceDependsOn` (an authored
declaration) — that makes reconciliation self-satisfying and renders stale declarations
indistinguishable from observations. Write into a separate named graph
`gmeow:graph/slice-analysis` with distinct predicates:
`gmeow:computedSliceDependency`, `gmeow:dependencyStatus`, `gmeow:dependencyEvidence`,
`gmeow:computedProfileMembership`, `gmeow:termCoverage`. The analysis graph records the
source bundle content ID, toolchain/version, generation activity, evidence links, and
whether each edge is *matched / undeclared / stale / forbidden*. Follow the repo's existing
**two-pass attestation** pattern (build authored bundle → compute analysis against that
immutable input → attach analysis in a second pass) so it never attests itself. Persist
analysis automatically; editing `manifest.ttl` is a separate explicit
`gmeow slice fix-deps --apply` producing a reviewable patch.

### 12. Semantic, phase-specific, path-independent cache keys

The current cache hashes physical relative paths, file sizes, and raw bytes — which
conflicts with the doctrine that the group path carries no semantics (moving
`slices/core/x` to another group must not invalidate its semantic compilation). Use a
Merkle-style key over: raw artifact digest, semantic RDF digest, manifest digest,
dependency output digests, phase configuration, compiler/rule version, reasoning profile.
Different phases select different roots — reasoning: semantic module + dependency closure +
rules; SHACL: semantic module/data + shapes + config; tests: compiled graph + test/query
artifacts; packaging: raw bytes + metadata; docs: Markdown + translations + term index. A
comment-only change must not force a new reasoning closure (but *should* change the
source-complete bundle).

## S0 — Frozen Semantic Contract

**Status: FROZEN (this child, S0).** This section is the authoritative,
implementation-binding contract for this RFC. Children S1–S8 realize it; they may
add, but must not contradict, anything fixed here. Where the surrounding RFC
sketches the architecture, this section *decides* it.

### S0.1 The five separated concerns (frozen)

A scalar `SliceId` column is **not** a provenance model. These five concerns are
permanently distinct; no implementation may collapse any pair into one field:

| # | Concern | Question it answers | Carrier |
| - | --- | --- | --- |
| 1 | **Source origin** | which physical unit/artifact asserted this occurrence? | `AssertionOccurrence` (`UnitId` + `ArtifactId`) |
| 2 | **Semantic ownership** | which slice *defines* this vocabulary term (`rdfs:isDefinedBy`)? | `DefinitionRecord` (`TermId` → `SliceId`) |
| 3 | **Artifact membership** | which packaged artifact is this (module/shape/mapping/query/…)? | `ArtifactId` + artifact role |
| 4 | **Evaluation scope** | over which graph was this result computed? | `Attribution{ role: EvaluationScope }` |
| 5 | **Derivation support** | which rule applications / premises justify this derived fact? | `RuleApplication` (OR of supports) |

Ownership (2) is an *authored declaration* and is never trusted as physical
provenance (1): load origin is always retained even when `rdfs:isDefinedBy` is
wrong.

### S0.2 Identifier types (frozen)

```rust
pub struct UnitId(u32);        // a compilation/source unit
pub struct ArtifactId(u32);    // a packaged artifact within a unit
pub struct OriginSetId(u32);   // an interned set of origins (set-valued datasets)
```

All three are **opaque `u32` newtypes** owned by the generic RDF kernel
(`gmeow-rdf`); the kernel never learns what a slice is. The slice layer
interprets a unit's *kind*:

```rust
enum UnitKind { Slice(SliceId), RootOntology, Import, Generated, RuntimeInput }
```

`UnitKind` is **total over every build input**: the root ontology, OWL imports,
generated graphs, and runtime input are each an explicit non-slice unit. There
is no `Unknown` variant — an unattributable origin is a **hard failure**
(no-optionality / hard-fail), not a sixth enum case.

### S0.3 Record types (frozen)

```rust
struct AssertionOccurrence { quad: QuadId, unit: UnitId, artifact: ArtifactId, location: Option<LocationId> }
struct DefinitionRecord    { term: TermId, slice: SliceId, evidence: AssertionOccurrenceId }
struct RuleApplication     { rule: RuleId, premises: Box<[FactId]>, rule_unit: Option<UnitId> }
struct Attribution         { unit: UnitId, role: AttributionRole, evidence: Box<[LocationId]> }
enum   AttributionRole {
    AssertionOrigin, DefinitionOwner, ShapeOwner, RuleOwner,
    FocusOrigin, ValueOrigin, DerivationSupport, EvaluationScope,
}
```

The canonical dataset is **set-valued**: identical quads authored by two
slices collapse to one `QuadId`, so identity cannot hold origin — *occurrences*
do, and the **same quad keeps multiple `AssertionOccurrence`s** (one per
asserting unit). A fact's support is an **OR of `RuleApplication`s plus
assertion occurrences**, never a flattened contributing-slice set; the displayed
"contributing slices" is a lazily computed union of provenance leaves.

### S0.4 Authored-vs-generated classification (frozen)

Every `UnitKind::Slice` artifact is **authored**; every `RootOntology` /
`Import` / `Generated` / `RuntimeInput` unit (and any computed
`gmeow:graph/slice-analysis` output) is **generated**. The two are never
interchangeable: generated analysis facts must **never** become inputs to their
own dependency computation (two-pass attestation — build authored bundle first,
compute analysis against that immutable input, attach in a second pass).

### S0.5 Persistent-vs-runtime ID rule (frozen)

Numeric interner IDs (`UnitId`/`ArtifactId`/`OriginSetId`/`TermId`/`QuadId`/…)
are **runtime-only**. They MUST NOT enter any persistent derivation identity,
content-addressed cache key, serialized provenance, or RDF output. Persistent
derivation identity hashes the **rule IRI + premise reifier IRIs** (per
`crates/logic/src/provenance.rs`); persistent attribution serializes the
**public slice IRI**, via `gmeow:attributedToSlice` / `gmeow:shapeSlice` /
`gmeow:contributingSlice` — never a graph-local numeric `SliceId`. Cache keys
are **path-independent**: moving `slices/core/x` to another group changes no
semantic ID, dependency result, or cache key.

### S0.6 Required slice anatomy & artifact roles (frozen)

A slice is an artifact **catalog**, not a monolithic parsed object. The closed
role set, plus an **open `Other(IRI)`** escape for third-party / forward
compatibility:

```rust
enum ArtifactRole {
    Manifest, Module, Shapes, Mapping, CompetencyQuery, VerifyQuery,
    TestDsl, Example, CounterExample, Documentation,
    TranslationCatalog, Citation, Other(IRI),
}
```

The catalog preserves **both** the verbatim manifest RDF graph (every unknown
triple, every literal's language/datatype identity) **and** a validated typed
projection; the inventory includes translations (`i18n/*.po`). Consumer crates
compile only the roles they understand — the catalog parses no SHACL / test-DSL
/ SPARQL into one struct, so the layering cycle of §3 cannot form (enforced by
the S0 crate-layering gate, below).

### S0.7 `SegmentUnitMap` set semantics (frozen)

GTS segmentation is a packaging optimization, never slice identity. The mapping
is **set-valued in both directions**:

```text
segment ↔ zero or more compilation units
slice   ↔ one or more segments
```

The invariant `one segment == one slice` is **rejected**. Semantic provenance
must survive concatenation, compaction, resegmentation, a slice split across
segments, and global generated segments with no owning slice. `SegmentUnitMap`
is a metadata-backed *set*; `RdfSegmentRecord.slice_iri` (single-valued) is
insufficient and is replaced.

### S0.8 Manifest datatype contract (frozen — resolves §4's discrepancy)

The §4 discrepancy — discovery coerces any RDF literal to a plain string while
`shapes/slice-manifest-shapes.ttl` declared consumer literals `xsd:string`
though every committed manifest uses *language-tagged* literals — is resolved
here authoritatively, and encoded in `shapes/slice-manifest-shapes.ttl` by this
child. Manifest literals split into exactly two classes, by the lang-tag
discipline:

- **PROSE (human-facing, localizable) → `sh:nodeKind sh:Literal` +
  `sh:languageIn ( "x-gmeow-english" )`, NEVER `@en`.**
  Properties: `gmeow:sliceConsumer`, `dcterms:title`, `rdfs:label`. These are
  translatable sentences/labels and carry the `@x-gmeow-english` source tag
  (every committed manifest already does).
- **TOKEN / proper-name (programmatic or proper noun) → plain `xsd:string`,
  NEVER lang-tagged.**
  Properties: `gmeow:sliceProfile`, `gmeow:providesSubcommand`,
  `gmeow:builtAgainstCore`, and **`dcterms:creator`**. A `providesSubcommand`
  is a dispatch token; a `sliceProfile` names an OWL composition IRI + GTS tag;
  a `dcterms:creator` is a **legal-entity / personal proper name** projected
  verbatim into `CITATION.cff`. A proper name is not translatable prose, so
  tagging it `@x-gmeow-english` would be a category error **and** would
  hard-fail validation against the existing untagged manifests — therefore
  `dcterms:creator` is frozen as a **plain token literal**, deliberately set
  apart from the PROSE properties.

The Python loader (`src/gmeow_tools/slices.py::_strings`) keeps its
string-returning contract: it is a documented **lexical projection** that
accepts either class and drops the language/datatype identity. Preserving that
identity end-to-end is the native catalog's job (S1), not the structural Python
view's.

### S0.9 The crate-layering gate (frozen — delivered by this child)

The Rust-side twin of Principle 16, enforced now (not deferred):

- **RDF core purity** — `gmeow-rdf-core` may depend on the neutral
  `gmeow-rdf-events` protocol seam, but no slice/domain/adapter crate may leak
  into the core. `gmeow-rdf` is the oxigraph/PyO3 adapter that depends on and
  re-exports the core. A registry crate such as `gmeow-gts` (published, no
  `path`) is an external boundary, not an internal layering edge, and is
  excluded by construction.
- **Acyclic layering** — the first-party crate dependency graph is a **DAG**;
  any cycle is a hard error (the monolithic-compiler trap of §3).

Implementation: `crates/validate/src/crate_layering.rs::check_crate_layering`,
surfaced as `gmeow-dev crate-check` and `make crate-check`, wired into
`make check`, and registered in `governance/constitution.ttl`
(`meta:gate-crate-layering`) under Principle 16.

## Epic decomposition

| Child | Scope |
| ----- | ----- |
| **S0** | Semantic RFC + crate-layering gate (mandatory first): unit/origin/ownership/derivation semantics, authored-vs-generated data, persistent-vs-runtime IDs, required anatomy, authoritative manifest datatype contract. |
| **S1** | Native slice catalog + artifact inventory: manifest-based discovery, raw manifest preservation, typed view, normalized logical paths, content digests, unknown-artifact preservation. |
| **S2** | Generic provenance sidecar — `UnitId`/`ArtifactId`/`OriginSetId`, assertion occurrences, origin-set interning, definition-owner table — no GMEOW slice semantics in the base dataset. |
| **S3** | Self-describing bundle resource layer: content store, artifact index, manifest graph, GTS mapping. Slice-affine segments are an optimization, not the attribution source. |
| **S4** | Native ownership + dependency analyzer: evidence-bearing dependency graph, exact declared-vs-computed reconciliation, profile closure, path-independent ownership. *(Lands before cache composition — its closure defines invalidation.)* |
| **S5** | Structured attribution through SHACL, diagnostics, logic, SARIF, and RDF (`Attribution`/`AttributionRole`). |
| **S6a** | Phase-specific Merkle cache + SCC/profile composition. |
| **S6b** | Exact incremental reasoning: alternative derivations / truth maintenance; add/change/delete parity. |
| **S7** | Generated slice-analysis graph + explicit `gmeow slice fix-deps` manifest-fix command (two-pass attestation). |
| **S8** | Retire Python discovery, rollup, and `module_specs` plumbing. **✅ ownership plumbing done:** the native `gmeow_slice.OwnershipAnalyzer` is the sole authoritative `make validate` ownership source; the path-derived `slice_ownership_lint` (Rust engine + PyO3 binding + Python wrapper) and the `module_specs` `ValidateOptions` field/phase are deleted. Restored the per-term single-owner invariant for the 4 colliding terms first (merge `gtsSegmentIndex`; rename `etymonSource`/`voiceExemplifiedBy`/`roleNarratingVoice`). |

Ordering: S1 may proceed independently; S2 waits on the ID-addressed dataset; S3 and S5
proceed in parallel once S2's IDs + bundle boundary are fixed; S4 lands before cache
composition.

## Critical files / anchors

- Slice meta-ontology & gates: `slices/vocabulary.ttl`,
  `shapes/slice-manifest-shapes.ttl`.
- Python plumbing to replace: `src/gmeow_tools/slices.py`, `gts_gen.py`.
  (`gts_producer.py` is now the Rust `purrdf::gts_compose` core used by
  `crates/gmeow-dev-cli/src/feedback_bundle.rs` and `crates/pipeline`.)
  Ownership plumbing **retired**: `crates/validate/src/validate_all.rs` (`module_specs`)
  and `crates/validate/src/lint.rs` (`slice_ownership_lint`) are deleted; ownership is sourced
  from the native `gmeow_slice.OwnershipAnalyzer` and folded in `src/gmeow_tools/validate.py`
  via `native_ownership_errors()`.
- Carriage surfaces: `crates/rdf/src/lookaside.rs` (`RdfSegmentRecord`, blob records),
  `crates/rdf/src/diagnostic.rs` (`RdfLocation`), `crates/rdf/src/gts.rs`, `gts_write.rs`.
- Output models: `crates/diagnostics/src/model.rs`, `render.rs`,
  `crates/shacl/src/report.rs` (`ValidationResult`), `crates/logic/src/provenance.rs`
  (`ChaseProvenance`, derivation IDs).

## Verification — stronger acceptance gates

Provenance gate (replaces "every quad has ≥1 SliceId"):

- Every assertion occurrence has exactly one source unit and artifact.
- Every semantic quad has at least one assertion occurrence *or* derivation.
- Duplicate equal quads from two slices **retain both origins** after RDF-set dedup.
- Every GMEOW vocabulary definition has exactly one validated owner.
- Root ontology, imports, generated graphs, and runtime input are explicitly representable
  as non-slice units.

Tests:

- Renaming/moving a slice directory changes **no** semantic IDs, dependency results, or
  compilation cache keys.
- Unknown manifest triples and literal language/datatype identity survive bundle
  round-trip.
- Every authored slice artifact is recoverable **repo-free** by role, logical path, digest,
  and bytes.
- Resegmenting a GTS bundle leaves slice identity, artifacts, provenance, and RDF semantics
  unchanged.
- Removing one slice **preserves** a derived fact when an alternative proof remains.
- Incremental add / modify / delete equals a clean rebuild across cyclic core dependencies
  and extension-plus-core products.
- A cross-slice SHACL result records distinct **shape-owner** and **data-origin**
  attributions.
- An absence-based (`minCount`) SHACL result has a valid **evaluation-scope** attribution
  even without an offending quad.
- Generated analysis facts never become inputs to their own dependency computation.
- Bundle loading rejects digest mismatches, duplicate logical artifact paths, absolute
  paths, `..` traversal, and conflicting manifests for one slice IRI.
- Stay green: `make check`, `make test`, logic derivation-graph goldens.

## Doctrines honored

Maximal information flow; bundle-is-the-useful-surface (self-describing, repo-free,
per-artifact content-addressed); greenfield / no-backcompat (manifest-driven ownership
replaces path-derivation); no-optionality / hard-fail (unknown origin, digest mismatch, and
malformed structure all fail). The strongest form of this RFC is a **content-addressed
compilation-unit catalog**, not a `SliceId` on every RDF row.
