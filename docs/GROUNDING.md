<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The Grounding Contract — logic:, lang:, math: as one foundation

> The cross-slice contract for the three grounding slices. It is canonical for
> any work touching `slices/grounding/` or the seams between the grounding
> layers and the rest of the ontology, in the same way
> [`PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) is canonical for the regeneration
> pipeline. Each slice's own design set (`slices/grounding/*/design/`) remains
> canonical for that slice's interior; this document governs what the three
> must share, how they may reference one another, and what every other slice
> owes them.

## The grounding kernel

The three grounding slices — `logic:` (reasoning), `lang:` (semiosis), and
`math:` (structure and quantity) — are **co-foundational peers** (Principle
19). They form one interlocked foundation, not an internal dependency ladder:
signs denote logical content, mathematical content is rendered as signs, and
measured magnitudes on signs are mathematical quantities. Forcing these three
into an acyclic order would amputate one of them — quantity is mathematics,
rendering is language, denotation is logic — and any ordering that makes a
grounding concept flee its home slice is a tier error.

The peerage is declared, not implicit: each grounding manifest carries
`gmeow:sliceCoFoundationalWith` naming the other two. `gmeow:sliceDependsOn`
remains acyclic and keeps its existing meaning everywhere; peerage is a
different relation, restricted to the grounding triple. Downstream slices see
the kernel as one foundation and depend on its members individually.

Two hard rules govern the kernel:

1. **The seam rule.** Every cross-slice term reference between peered slices
   must land on a seam registered in this document. Peerage is a registry of
   sanctioned mutual seams, never a license for unregistered coupling. A new
   seam requires a registration here — name, direction, carrying terms, and
   the design doc that owns it — before the first referencing triple is
   authored.
2. **The tier rule.** A grounding slice never depends on a non-grounding
   slice for a grounding concept. Where a grounding concept is found split
   across a grounding and a non-grounding slice, the reconciliation direction
   is fixed: the grounding slice owns the concept and the non-grounding slice
   consumes it. Quantity is the standing applied example: `math:Quantity` and
   `math:quantityValue` are the sole class/property authorities, carrying the
   dimension and concrete magnitude. The observations slice consumes that
   object and adds unit, frame, determinacy, uncertainty, and provenance
   qualifiers without minting observation-local aliases.

## External grounding ownership

All semantic grounding to an external formalism is authored in the grounding
kernel, never in a downstream domain slice. The owner is selected by the kind
of meaning being grounded:

| External surface | Owning slice | Canonical direction |
|---|---|---|
| Linguistic, lexical, semiotic, and serialization formalisms | `lang:` | `lang:` → external view |
| Mathematical structures, quantities, and mathematical interchange languages | `math:` | `math:` → external view |
| Upper ontologies, logical formalisms, rule languages, and validation dialects | `logic:` | `logic:` → external view |

The external term is a **target endpoint**, not a term from which a domain
slice derives its semantics. Non-grounding slices consume the grounding term
and its correspondence; they do not author a second alignment. RDF/OWL
declaration syntax and generated validation syntax are serialization/compiler
boundaries, not exceptions that grant external vocabularies semantic ownership.

Each row of that table is declared as a `gmeow:GroundingDomain` individual in
the owning slice's manifest, carrying the owner in `gmeow:groundingDomainOwner`.
A term whose subject matter falls in one of the three domains is a **grounding
concept**, and says so on itself: `gmeow:groundingConceptDomain` names its
domain. That marker is what the tier rule is machine-checked against — a
grounding slice referencing a marked term that a non-grounding slice owns is a
violation, and the fix is always the fixed reconciliation direction (re-point
the term's `rdfs:isDefinedBy` and move its block to the domain's owner; the IRI
never changes, because ownership is by `rdfs:isDefinedBy`, not by namespace).
The judgment is authored rather than inferred: subject matter is not visible in
graph shape. The qualifier stays load-bearing in the other direction too — a
grounding slice consuming ordinary domain vocabulary by reference is sanctioned,
and formalizing a domain term in a `logic:Formula` does not make that term a
grounding concept.

Grounding correspondences are different from presentation-only projections.
They compile to content-addressed `logic:Correspondence` records and ship in
the `graph/correspondence-laws` named graph of `gmeow.gts`, while remaining
meta-level and outside object-level closure. SSSOM and other alignment formats
are generated views of those records.

The shipped grounding surface is split by semantic owner:

- `logic:` carries 140 core correspondences for gUFO, BFO, OBO/RO, SUMO,
  OWL/RDFS, and SHACL Core/AF in
  [`grounding-bridges.ttl`](../slices/grounding/logic/mappings/grounding-bridges.ttl),
  plus 25 commitment-shifting DUL, IAO, OBI, PATO, YAMATO, and OpenCyc rows in
  [`foundation-bridges.ttl`](../slices/grounding/logic/mappings/foundation-bridges.ttl).
  OBI's catalog is `logic:`-owned because its backbone is a planned-process
  upper ontology: the prescription and enactment bridges
  (`logic:Plan` → OBI protocol, `logic:Enactment` → OBI planned process) are
  authored there once, and every downstream `obi:` surface is a generated
  projection of them.
- `math:` carries its identity catalog, six direct quantity/value bridges to
  SOSA, OM 1.8, IVOA ObsCore, LOINC, and QUDT in
  [`quantity-bridges.ttl`](../slices/grounding/math/mappings/quantity-bridges.ttl),
  plus 13 validation-only Data Cube, STATO, OBCS, SIO, and OBI rows in
  [`statistical-bridges.ttl`](../slices/grounding/math/mappings/statistical-bridges.ttl).
  The single OBI row there (`math:DataTransformation` → OBI data transformation)
  predates the ownership split and sits off the owner boundary, so it is an
  explicitly ceilinged residue of exactly one (`gmeow:pcc-math-obi`) that may
  only fall — never a second OBI authoring home.
- `lang:` carries its identity catalog plus 21 validation-only OntoLex-Lemon,
  LexInfo, Global WordNet schema, NIF, and Web Annotation rows in
  [`lexical-bridges.ttl`](../slices/grounding/lang/mappings/lexical-bridges.ttl).

Every grounding row is oriented from its grounding namespace and states its
morphism class, morphism kind, and preservation kind. Commitment-shifting
views are never promoted to equivalence. This division is also the migration
rule for domain slices: use the `logic:`, `math:`, or `lang:` term and its one
owned correspondence instead of asserting an external vocabulary term again.
The foundation policy and coverage ledger is
[`foundational-bridging.md`](./foundational-bridging.md).

## The seam registry

The closed set of sanctioned seams among the grounding slices — directions are
reference directions (who names whose terms) — is **canonical governance
data**, not hand-maintained prose. Each seam is a `gmeow:Seam` individual
(`gmeow:seamDirection`, `gmeow:seamCarryingTerm`, `gmeow:seamOwningDoc`)
authored in a grounding slice's `manifest.ttl` (today, `logic:`'s registers
all eight, as the neutral registrar); the vocabulary is defined in
`slices/vocabulary.ttl`. A new seam requires a new `gmeow:Seam` individual
here before the first referencing triple is authored — never a new row typed
into this document. The registry's size is itself gated: the shipped-graph
non-vacuity check in `crates/pipeline/src/stages/carrier.rs` pins the closed
count, so a seam is added or dropped deliberately, never discovered by drift.

That data reaches two projections, both derived from the manifests and never
hand-maintained:

- **The shipped graph.** `gmeow.gts` carries the whole registry as its own
  queryable named graph,
  `https://blackcatinformatics.ca/gmeow/graph/grounding-seams` — one
  `gmeow:Seam` per authored seam with its `rdfs:label`, its
  `gmeow:seamDirection` legs (`gmeow:seamFromSlice` / `gmeow:seamToSlice`),
  every `gmeow:seamCarryingTerm`, and every `gmeow:seamOwningDoc`. It is
  lossless, so a consumer holding only the bundle can reconstruct the registry
  without the repository. Like `graph/correspondence-laws`, it is governance
  data and stays outside object-level closure.
- **The rendered table.** The always-current human table (seam, direction,
  carrying terms, owning design doc) is the generated seam-registry page
  `ontology-docs/seams/index.md`, rendered on demand by
  `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` (`ontology-docs/` is a git-ignored local
  product, so the page exists only after that render). It is a direct
  projection of the same data, and a validator gate keeps it from drifting out
  of sync with the manifests.

`logic:` reaches into a peer exactly once, and the registry says where: the
**quantity-boundary seam** carries the single term `math:Quantity`, because
`logic:` owns the SUMO boundary while the tier rule fixes `math:Quantity` as
the sole authority for dimensioned magnitude, so the honest bridge row has a
peer-owned source endpoint (`LOGIC-CORRESPONDENCE.md`, "The quantity
boundary"). Everything else `logic:` might be tempted to say about a peer's
term belongs in the peer: a `logic:PropertyCharacteristicAssertion` about a
`lang:`-owned property is a fact about `lang:`'s own term and is authored in
`lang:`'s `module.ttl`, exactly as it is for a non-grounding slice. Prose in
`logic:` definitions does not name `lang:`/`math:` terms; illustrative
examples belong in the peer's own documents.

## Shared disciplines

The disciplines every grounding slice satisfies identically. Where a slice
pioneered the discipline, its form is normative for the other two.

- **Preservation vocabulary — reused verbatim, never shadowed.**
  `logic:preservationKind` and the loss-ledger vocabulary are the single
  preservation surface. The lang: precedent is normative: domain-specific
  preservation *predicates* (`lang:renderingPreservation`) may point at the
  shared kinds, but no slice mints a parallel preservation enum or a shadow
  loss ledger.
- **Content-addressed identity — one discipline, one arena.** Structural
  content keys (surfaces, encodings, and renderings excluded) identify forms,
  formulas, and expressions across all three slices. The realization target
  is a single shared structured-term store serving all three (the
  hash-consed, alpha-normalized term arena of
  [`LOGIC-PERFORMANCE.md`](../slices/grounding/logic/design/LOGIC-PERFORMANCE.md)
  § Grounding-layer computability); slice-local key predicates are staging
  surfaces for that unification, not permanent forks.
- **The solver seam — logic-owned, observation-carried.** Budgeted evaluation
  and its incomplete-never-wrong contract live in `logic:`. When `lang:` or
  `math:` need heavy computation (parsers, provers, samplers, decompositions),
  the work crosses the solver seam and re-enters as vantage-held
  `gmeow:Observation`s from a named engine — never as bare asserted fact and
  never as a private budget vocabulary.
- **The document set template.** Each slice keeps a manifesto with a
  document-set table, a `*-CONFORMANCE.md` gate matrix, and a
  `*-REFERENCES.md` staging appendix. The conformance stance is shared and
  literal: **a rule with no negative fixture is not enforced.**
- **Naming-collision ledger.** Known benign collisions, recorded so greps
  don't mislead: `math:preservationLaw` is the algebraic
  structure-preservation law of a homomorphism, unrelated to the
  `logic:preservationKind` loss ledger; `math:LossFunction` is the machine
  learning training objective, unrelated to `logic:` projection loss. New
  terms avoid overloading `preservation*` and `Loss*` outside these two
  established senses.

## The acceptance bar

The flagship acceptance-manifest contract is **the** grounding-slice depth
bar, at its strongest realized form: five reified flagship scenarios per
slice, each binding a worked example, a pinned competency question, a guarding
counter-example, a named conformance-failure class, **and a named native
producer**, discharged on three static surfaces (SHACL shape, structural ASK,
native cross-check) **plus execution** — a harness that runs the
counter-example and asserts exactly the declared failure class fires, runs the
worked example and asserts nothing fires, and runs the producer and asserts
its folded output. Existence-only manifests and slices with no manifest are
below the bar. The flagship vocabulary is shared, not copy-forked per slice.
Each scenario's guarding counter-example is today discharged by a
structural / SHACL well-formedness proxy; the depth target is a reasoner-driven
counter-example whose malformed input the native solver runs to observe the
missing entailment directly, and each scenario records its discharge honestly so
the gap is surfaced (see
[`LOGIC-CONFORMANCE.md`](../slices/grounding/logic/design/LOGIC-CONFORMANCE.md)).

## The coverage duty

Coverage is audited in both directions:

- **Design-promise direction:** every surface a design doc promises is
  realized, explicitly gated as design-only in the doc itself, or carried by
  a wired issue. Silent omission from a slice's own status table is a defect.
- **Built-beyond-docs direction:** what is built often exceeds the documents;
  when it does, the documents follow the built form. A stale "queued" or
  "deferred" marker for work that has landed is a defect of the same weight
  as an unrealized promise.

The design-promise direction is now **gated, not audited by vigilance.** The per-doc
realized-state column of a slice's `docs.md` design-set table (every artifact marked
design-only / partial / built) is scored by the `gmeow:dimRealizedState` coverage dimension
(`slices/core/documentation/module.ttl`; the standard is [`SLICE_GUIDE.md`](SLICE_GUIDE.md)
§ 6.8). A design-set-table entry with **no** realized-state marker misses the dimension, so
the silent omission this duty warns against is a scored, gating defect — the missing marker
drops the slice below the tier it claims and the `asserted ⊄ earned` maturity gate reds the
build. Because `gmeow:dimRealizedState` sits at the FULL floor, a grounding slice asserting
`≥ FULL` cannot quietly drop a status-table marker; the gate bites before review does.

Downstream, MAXIMAL GROUNDING (`.goals`) is a duty owed *to* the kernel:
every slice grounds in `logic:`; every slice with a textual, nominal, or
notational surface grounds it in `lang:`; every slice with quantities,
magnitudes, rates, scores, or statistical claims grounds them in `math:`
(`math:Quantity`, dimensions, and the statistics vocabulary) rather than
minting bare numerics. A quantitative slice with no `math:` reference is
carrying an ungrounded number.
