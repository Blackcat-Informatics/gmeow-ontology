<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Documentation — the projection that documents itself

**A documentation surface that cannot be queried, grounded, or audited as data is
just prose that drifts from the ontology it claims to describe.** This slice owns
the vocabulary that stops that drift: the `gmeow:doc*` TBox behind the self-hosting
documentation projection, so the docs surface is itself first-class RDF folded into
`gmeow.gts` beside the ontology it documents (Principle 4).

## What rots without this slice

The documentation projection (`crates/docs/src/rdf.rs::to_gmeow_rdf`) dogfoods the
doc model into the `gmeow:graph/documentation` named graph — one record per
documented term, slice, concern, and mapping set, plus a uniform, grounded
`gmeow:DocEvidence` node per unit of proof-carrying documentation. That projection
emitted a dozen-plus `gmeow:doc*` IRIs with **no TBox declaration anywhere**: the
documentation graph named terms that the ontology did not own. Ownerless emitted
terms are exactly the drift this project refuses — a term with no `rdfs:isDefinedBy`
slice is unowned (a `make validate` error), and a term the projection emits but no
slice declares is an orphan the reader cannot resolve. This slice closes the gap:
every `gmeow:doc*` term the projection materializes is declared here, and an
orphan-zero gate (`crates/docs/tests/vocabulary_ownership.rs`) hard-fails if the
emitter ever grows a term this TBox does not cover.

## The load-bearing distinctions

- **The record is not the term.** A `gmeow:DocumentedTerm` is a downstream
  information object standing for a term's *page*; it `gmeow:documents` the real
  term, which keeps its own IRI in its owning slice. Conflating the two would make
  the documentation graph claim ownership it does not have.
- **Evidence is a claim WITH its grounds.** Every `gmeow:DocEvidence` node carries
  at least one mandatory `gmeow:docGroundedBy` truthmaker. An ungrounded evidence
  node is the doc-layer analogue of a DARK finding — forbidden by construction (the
  SHACL grounding shape and the live Rust invariant together; not a TBox cardinality
  restriction, since `gmeow:docGroundedBy` carries mixed IRI/literal objects and is an
  `owl:AnnotationProperty`).
- **Kind is a value, not a predicate family.** The five evidence kinds (competency,
  diagnostics, fixture, loss, provenance) are `gmeow:DocEvidenceKind` individuals
  referenced through one `gmeow:docEvidenceKind` classifier, so a sixth evidence
  source enters as a new *kind*, never a new parallel predicate family.
- **Reuse, don't redeclare.** The per-instance provenance predicates the projection
  reuses (`gmeow:addedInVersion`, `gmeow:definitionDigest`, `gmeow:hasChangelogEntry`,
  `gmeow:entryVersion`, `gmeow:entryNote`, `gmeow:ChangelogEntry`) stay owned by
  `slices/core/versions`; this slice declares only the genuinely-orphaned `gmeow:doc*`
  terms.

## Realized state

| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| [`manifest.ttl`](./manifest.ttl) | contract | realized | slice identity, core tier, dependencies (kernel, logic, versions), consumer |
| [`module.ttl`](./module.ttl) | vocabulary | realized | the 5 record classes, `gmeow:DocEvidence`, the `gmeow:DocEvidenceKind` value vocabulary + 5 seed individuals, and the 17 `gmeow:doc*` projection predicates, each with its full annotation coat |
| [`shapes.ttl`](./shapes.ttl) | enforcement | realized | closed-world data shapes: a documented-term shape and the fail-fast `gmeow:DocEvidence` grounding shape |
| [`examples/documented-term.ttl`](./examples/documented-term.ttl) | scene | realized | a self-describing worked scene — the projection dogfoods on its own `gmeow:DocEvidence` term, with a grounded competency node and a grounded provenance node |
| [`tests/structural.ttl`](./tests/structural.ttl) | enforcement | realized | MUST cells over the module graph: class/property typing + box roles, and the module+examples grounding invariant |
| [`tests/competency.ttl`](./tests/competency.ttl) | enforcement | realized | the pinned questions the slice answers — which slice owns a documented term, and is every evidence node grounded |
| [`queries/competency/`](./queries/competency/) | queries | realized | the SPARQL bodies for the competency cells |

The orphan-zero contract is enforced in Rust rather than as a slice cell because the
live `gmeow:graph/documentation` projection is produced downstream of `stage-validate`
and never seen by `make validate`; the gate re-derives the emitted term set from the
projection output itself, so a newly-emitted undeclared term reds the gate.
