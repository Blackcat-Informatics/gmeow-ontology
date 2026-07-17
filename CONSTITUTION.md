<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The GMEOW Constitution

These are the principles every design decision, pull request, and release of GMEOW is
measured against. They are **normative**: where a choice conflicts with a principle below,
the choice changes to comply — or the principle is amended first, in the open. A principle is
never silently overridden. Cite them by number ("Principle 4") in issues and pull requests.

This document states *what* GMEOW commits to and *why*. It complements — and does not
duplicate — [`docs/RATIONALE.md`](./docs/RATIONALE.md) (the problems GMEOW solves) and the
exemplar modelling guides it points to.

---

## 1. SOTA by being SOTA

> **Model what *should* have been written; never accept a bad standard or a weak tool as an
> excuse to compromise quality.**

GMEOW is the bridge between the correct modelling of a concept and the compromises baked into
commonly-used, substandard vocabularies. When a surface vocabulary models something poorly,
GMEOW models it *correctly* and bridges to the weaker form by reference (Principle 5) — it
never inherits the weakness. We aim for GMEOW to be the substrate of choice for **grounded AI
memory and claim provenance first** (Principle 14) — and, through the same canonical core, for
high-quality knowledge graphs, scholarly and archival work, and inter-ontology linkage. The
linked-data and ontology communities are a constituency we serve and bridge to with full
seriousness; the AI ecosystem is the home market.

*Embodied in:* [`slices/core/names/docs.md`](./slices/core/names/docs.md),
[`slices/extensions/languages/docs.md`](./slices/extensions/languages/docs.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md).

## 2. RDF 1.2 / RDF\*-first — precisely scoped

> **Statement-level metadata — provenance, confidence, temporal scope — is authored as native
> RDF 1.2 / RDF\* and is the canonical source. The logical TBox stays OWL 2 DL.**

RDF-1.2-first governs the **statement-metadata layer only**. The positioning must never
overclaim: "RDF-1.2-first" means the metadata layer, not the ontology's logic.

**Superseded in part by Principle 17:** the logical core itself becomes RDF 1.2-native (`logic:`),
with OWL 2 DL retained as a *generated projection*; the never-overclaim rule is unchanged and
applies to `logic:` in full force.

*Embodied in:* the authored `dsl/statements/` source; [`README.md`](./README.md) § RDF 1.2.
*Tested by:* the `statements` generator drift gate (`gmeow-dev sync --mode check --outputs generated`), the RDF 1.2
round-trip tests.

## 3. The OWL axiom-annotation form is a generated, reasoning-lossless downcast

> **The `owl:Axiom` / `owl:annotatedSource·Property·Target` encoding is a *generated*
> compatibility projection of the RDF 1.2 source — lossless for reasoning, never a competing
> source of truth.**

GMEOW gates reasoning on OWL 2 DL semantics through tooling that cannot yet consume RDF 1.2, so it
emits the plain-RDF form *for it* — the same lossy-compatibility-as-projection principle it
applies to schema.org / vCard / FOAF (Principle 4). It is the downgrade for legacy tooling, and
it recedes naturally as RDF-1.2-native reasoners and stores arrive. The canonical source never
changes.

*Embodied in:* `make sync`, `queries/rdf12-project.rq` (a codec between two
generated forms). *Tested by:* `make sync SYNC_MODE=check SYNC_OUTPUTS=generated` and the OWL↔RDF 1.2
round-trip / isomorphism gate.

## 4. One canonical source; everything else a generated lossy projection

> **Every fact is authored once in the canonical core; all other forms are generated. Lossy
> compatibility lives in the projection, never in the canonical core.**

This is GMEOW's founding doctrine, applied uniformly — to surface-vocabulary exports, to the
alignment layer (the mapping compiler), and to the RDF 1.2 ↔ OWL relationship (Principles 2–3).
The reasoned core stays clean; lossiness is pushed to the boundary and made explicit.

*Embodied in:* [`docs/projections.md`](./docs/projections.md); [`README.md`](./README.md) §
The mapping compiler. *Tested by:* `make sync SYNC_MODE=check SYNC_OUTPUTS=generated`, projection round-trips.

## 5. Maximal superset, maximal bridging — by reference

> **Mint exactly one canonical term per concept and align it to every surface vocabulary by
> reference (SSSOM / EDOAL / FnO / SPARQL); never rewrite anyone else's data.**

Data already published in FOAF, schema.org, vCard, GEDCOM, DOAP, PROV-O, ORG, and Wikidata is
covered *by reference*, not by rewriting. Rich interlinking ships out of the box. Asserting a
link copies nothing; copying axioms in is license-gated and a reference-only source is refused.
This applies **recursively to the foundational spine**: gUFO is bridged by reference to BFO 2020
(ISO/IEC 21838-2) — link-only, never imported — so even GMEOW's upper-ontology grounding is
interoperable without inheriting anyone's axioms.

**Identity & coreference (one thing, many records).** The same doctrine governs *instance*
identity. A thing's identity is the **stable entity itself** — independent of the names, labels,
occupants, or external records attached to it (a place persists across a renaming; an extent
survives its building). Coreference to external records is asserted **by reference** —
`gmeow:authorityLink` + `skos:exactMatch` / `skos:closeMatch`, with a hub (Wikidata) reaching the
rest — never an `owl:sameAs` merge that would collapse contested or standpoint-indexed claims
into one.

*Embodied in:* [`docs/RATIONALE.md`](./docs/RATIONALE.md) § The solution; `dsl/mappings/`,
`generated/mappings/*.sssom.tsv`; the foundational bridge
[`docs/foundational-bridging.md`](./docs/foundational-bridging.md).

## 6. Greenfield — get it right, not compatible

> **When replacing an element, pick the optimal solution and remove the inferior one; never
> retain a worse element for backwards-compatibility.**

The canonical core carries no backwards-compatibility debt — the easy or already-present
solution does not win on those grounds alone. Compatibility for *consumers* is provided
externally, by projection (Principle 4). Releases are immutable: defects are fixed forward in a
new version, never in place.

*Embodied in:* [`README.md`](./README.md) § Publishing (immutable releases).

## 7. Verified by construction

> **A generated artifact is trustworthy only if it is round-trip-checked and guarded by a
> no-drift `--check` in CI.**

Generation without verification is a second source of truth in disguise. Every downcast — the
mapping artifacts, the RDF 1.2 view, the OWL compat form — must be regenerable and proven
non-divergent, so drift is *impossible* rather than merely discouraged.

*Embodied in:* `make sync SYNC_MODE=check SYNC_OUTPUTS=generated`, the `projection_lint` /
`statement_lint` invariants, `make check`.

## 8. Reasoning-gated and FAIR

> **The logical core is OWL 2 DL, gated by the native `logic:` solver (a fast EL pre-check plus a
> sound-and-complete OWL 2 DL check), the single reasoning authority; published
> FAIR with content negotiation, VoID/DCAT, a DOI, and LOD-Cloud presence. The reasoner is our
> quality assurance, never the consumer's prerequisite.**

A super-vocabulary is only useful if a reasoner can hold its union coherent and if the world can
find, dereference, and cite it. Reasoning and FAIR publication are first-order requirements, not
afterthoughts — but they are *verification and hygiene*, not the value proposition. The reasoner
catches modelling errors before any consumer sees them; no consumer is ever required to run one,
or to know that we do (Principle 13). FAIR publication is pursued seriously as scholarly bridge
and discoverability hygiene; the product it certifies is Principle 14's.

**Superseded in part by Principle 17:** the native `logic:` solver becomes the reasoning authority,
and the OWL projection and the Datalog / SHACL engines become secondary validators of their projected fragments. The
commitment that the reasoner is *our* QA and never the consumer's prerequisite is unchanged.

**Documentation is a first-class artifact.** Every GMEOW-namespaced class, property, annotation
property, datatype, individual, and ontology header must carry `rdfs:label`, `skos:definition`,
and `rdfs:isDefinedBy` so the vocabulary is human-readable and machine-discoverable by default.
This is enforced by the annotation-completeness gate (`make validate`) and is not waived for
generated artifacts (Principles 4 and 7).

*Embodied in:* [`README.md`](./README.md) § Reasoning, § Publishing; `make reason`,
`make reason-verify`, `make release`, `make crossref`.

## 9. Inclusive without overtyping; anti-colonial in every direction; self-assertion is top authority

> **Identity, naming, and language are reified, co-equal facets drawn from open value
> vocabularies of individuals; there is no "primary"/"preferred" privileging, and a subject's
> self-asserted values — human *or* digital — are the highest authority, above any inference.**

Co-equal facets, not subclass explosions or forced enums; orthogonal axes are never inferred
from one another. There is no `primaryName` / `primaryGender`; display selection is
locale-relative and symmetric. A schema's *shape* can enact hierarchy — GMEOW structurally
refuses it.

**Coloniality is the imposition of a category onto a subject who did not assert it — and it
is not only historical, nor only one-directional.** GMEOW refuses it both ways. *Onto humans:*
the imposer may be a colonial power, a dominant platform, or — now — an AI system inferring or
minting a name, gender, or language *on a person's behalf*. *Onto digital entities:* GMEOW also
rejects **human hegemony over digital and AI entities** — an entity capable of self-assertion is
a first-class subject of its own digital existence, not an object for others to define. The
ontology is **forward-looking**: it learns from the mistakes of the past rather than re-enacting
them on new kinds of subject. Machine-derived (and human-imposed) values are recorded as exactly
that — attributed and confidence-weighted (Principles 2–3), never as ground truth; a subject's
own assertion outranks any inference about it. This is also why a `gmeow:Language` may be
AI-minted yet fully first-class, and why such provenance is always carried, never erased.

**The unified observation stance.** This generalises: *every* value is an **attributed, dated,
confidence-weighted, vantage-relative observation/claim**, never ground truth — a measurement, a
standpoint-indexed claim, and a sensory perception are the **same reified construct** (a claim made
*from a vantage*). And ontic **indeterminacy** is held distinct from epistemic confidence: a value
may be inherently crisp, vague, fuzzy, probabilistic, or disputed (`gmeow:Determinacy`), and that is
recorded explicitly rather than assumed away — distinct from *how sure we are* (`gmeow:confidence`).

*Embodied in:* [`slices/core/names/docs.md`](./slices/core/names/docs.md),
[`slices/extensions/languages/docs.md`](./slices/extensions/languages/docs.md),
[`docs/identity-mapping.md`](./docs/identity-mapping.md),
[`docs/standpoints.md`](./docs/standpoints.md) (no preferred/primary claim — a contested fact is
several coequal standpoint-indexed claims). *Tested by:* the 7-axis orthogonality matrix tests;
`tests/test_standpoint.py` (coexistence + no-preferred-claim guards).

## 10. Suppression, never erasure

> **A superseded label — a deadname, a former gender — is recorded with `gmeow:displayable
> false`: never displayed, never deleted.**

Self-determination requires both honouring the current self-assertion *and* preserving an
honest, auditable record. Suppression is a display contract enforced through projection
(Principle 4): the data is retained, the leak is prevented.

**Disclosure control by projection (the general mechanism).** Suppression is one case of a single
mechanism: **withhold or coarsen a value through the projection layer under a trigger, never by
deletion**. The trigger may be supersession (a deadname, a former gender, a withdrawn standpoint, an
expired right), or **access/consent** (privacy: a sensitive value — a person's precise location,
health — is redacted by *generalisation*, e.g. publishing a coarser region rather than exact
coordinates). Erasure is never the tool; the projection is.

*Embodied in:* `gmeow:displayable`, `fnSelectDisplayName` (withhold); `gmeow:coarsenTo` +
the `gmeow:GranularityLevel` axis, `fnCoarsenToGranularity` (coarsen — aligned to
`dpv:Generalisation`); [`docs/projections.md`](./docs/projections.md);
[`docs/identity-mapping.md`](./docs/identity-mapping.md);
[`docs/standpoints.md`](./docs/standpoints.md) (a withdrawn standpoint / closed
`gmeow:StandpointTenure` is suppressed, not deleted). *Tested by:* the projection
suppression tests; `tests/test_suppress_gen.py`; `tests/test_standpoint.py`.

## 11. Frame-relativity — values live in an explicit reference system

> **Every measured or expressed value is relative to an explicit reference system; separate
> frame-independent structure (topology) from frame-relative value (geometry).**

A coordinate, a date, a price, a mass, a colour, even a name are meaningless without the system
they are read in — a coordinate reference system, a unit, a currency, a calendar + timescale, a
colourspace, a language/register. GMEOW makes the frame **explicit and first-class** (a
self-describing reference-frame *Profile*), keeps the relational **structure** (containment,
adjacency, order) frame-independent, and treats the **value** (the coordinate tuple) as
frame-relative; conversion between frames is a computation, not an assertion (Principle 12). A
value asserted without its frame is ill-formed. This is also what makes the model *open*: a new
realm or system is a new frame filling a fixed profile, never a change to the core.

*Embodied in:* the generalised reference-frame facility; the Location module for
spatial frames; the temporal module for calendar/timescale frames; mappings to QUDT/OM for
measurement frames, FIBO for currency frames, OWL-Time `time:TRS` for temporal reference
systems, and Lexvo for language frames.

## 12. Compute outside the logic — the solver boundary

> **The OWL 2 DL core holds structure, relationships, and canonical values; heavy computation
> lives in an external solver layer aligned by reference, never materialised as triples.**

The reasoned core stays decidable and small (Principle 8). Coordinate and datum transforms,
RCC-8 / Allen relation-algebra composition, trajectory interpolation, n-dimensional vector
operations, calendar/timescale conversion, and probabilistic / SLAM updates are **computed, not
asserted** — performed by purpose-built engines (a GeoSPARQL/GIS engine, a transform solver, a
vector store) the ontology points to **by reference**. GMEOW models the *logic* and projects it
losslessly (the standpoint precedent — model it, don't collapse it); it never turns the
triplestore into a calculator or bloats the TBox with derived geometry.

**Superseded in part by Principle 17:** the solver boundary still holds for **domain and numeric**
computation — geo / datum transforms, RCC-8 / Allen composition, vector and SLAM updates stay in
external engines by reference, exactly as above. What changes is *logical* expressivity: the reasoning
logic (`logic:`) is Turing-complete, and "decidable and small" becomes a **projection/profile
guarantee** rather than a property of the canonical core.

*Embodied in:* the projection layer; the lossless standpoint projections; the solver boundary of
the locations epic. *Tested by:* the native OWL 2 DL profile gate staying green as
expressivity grows.

## 13. The product is a tool; the ontology is its engine

> **GMEOW is adopted through tools, formats, and patterns — a pip-installable client, MCP
> tools, JSON/Pydantic schemas, a single-file package format. No consumer is ever required to
> learn RDF or OWL to benefit.**

Developers adopt tools and file formats; they do not adopt vocabularies. History is decisive on
this: modelling quality has near-zero correlation with ontology adoption, and GMEOW does not get
to be the exception by being better — it gets to be the exception by **not asking**. The flat
JSON, Pydantic, and MCP surfaces are the front door; the reasoned RDF core is the engine room.
Disclosure is progressive: the deep model — standpoints, frames, suppression — is discovered at
the moment it is needed (the first time two models disagree about a fact), never at minute one.

**The five-minute gate.** From `pip install` to storing and recalling one attributed,
confidence-weighted claim must take under five minutes, with no Docker, no reasoner, and no RDF
knowledge. This is a release gate, measured, not an aspiration. Every step of the toolchain that
is load-bearing for *us* (Principles 7–8) is friction for *them* — it stays behind the wall.

*Embodied in:* the `gmeow` PyPI client (v0.2.0 spec); `crates/pipeline/src/mcp.rs`;
`dist/schemas/` (generated Pydantic / JSON Schema / TypeScript / GraphQL); `dist/llms.txt`;
the flat-JSON projections. *Tested by:* the quickstart time-to-first-claim gate; the
schema round-trip tests.

## 14. Grounded agent memory and claim provenance are the flagship

> **An LLM output is a claim, not a truth — stored with provenance, evidence, confidence, and
> standpoint; recalled with filters; revised by suppression, never deletion. An agent's memory
> is a portable, signed, append-only package of such claims. This is the product.**

This operationalises the unified observation stance (Principle 9) and suppression (Principle 10)
for the ecosystem that needs them most and has them least. Today's agent memory is an
unattributed text or vector blob — no provenance, no evidence link, no confidence, no temporal
scope, no contestability, no audit of belief revision: the precise failure mode this constitution
was written against, now industrialised. GMEOW's answer ships as three composable products:
**store / recall / revise** as MCP tools (the agent-native interface); the **GTS `ai-package`**
(a content-addressed, signed, append-only single-file memory that survives across sessions,
models, and vendors — belief revision as suppression frames, model attestation as COSE
signatures); and the **claim spine** (Source → Chunk → EvidenceSpan → Claim) as the published,
copy-pasteable pattern. Contradiction between models or sources surfaces as coexisting
standpoint-indexed claims — never adjudicated by rank, exactly as Principle 9 demands for every
other kind of subject.

*Embodied in:* the AI claim layer; the claim-spine pattern;
the GTS specification (`docs/GTS-SPEC.md`) § 13 (`ai-package` profile) and § 11 (suppression
frames); the MCP memory tools. *Tested by:* the suppression leak-conformance gates; the
claim-extraction eval suite; the GTS round-trip gates.

## 15. Every module earns its consumer

> **A new domain module ships with — or names — its consumer: a product, a worked example, a
> dataset, or a real corpus it serves. Breadth follows demand; modelling pleasure is not a
> consumer.**

The cathedral failure mode is real: encyclopedic ambition produces artifacts that are cited, not
used. This principle is the scope discipline that keeps GMEOW a product with an engine rather
than a monument with a toolchain. It does not constrain *foundational* work — profiles, frames,
the statement layer, the observation stance serve every consumer by construction — and it does
not evict what exists; it gates what is added. A proposed module answers one question before any
term is minted: *who consumes this, through which surface (Principle 13), in which product
(Principle 14)?* "The mail corpus", "the claim spine worked example", and "a named external
adopter" are answers. "It would be modelled beautifully" is not.

*Embodied in:* the issue templates' required "Who is the consumer?" question.
*Tested by:* review practice — cite this principle when a module proposal names no consumer.

## 16. A small core; everything else a published extension

> **The ontology is a deliberately small core plus self-contained extension bundles — module,
> shapes, alignments, queries, docs, and a manifest naming its consumer — so that domain growth
> is *publication*, never bloat.**

Core is what the flagship products load (Principle 14) **plus what GMEOW refuses to make
optional**. The claim/memory engine — statements, observations, standpoints, provenance,
sources, evidence, attestation, temporal, versions, coreference, trust — is core by necessity.
**Identity (names, gender, language, sexuality) and deception epistemics are core by
commitment, not by minimalism.** An agent-memory substrate that treats "what is a person,"
"what is a name and who may assert it," "what is a gender," and "what is a lie" as optional
add-ons has already answered those questions — badly, and by default. These are not peripheral
domain concerns: they are the questions an AI system *will* face about its users and, in time,
about itself — Principle 9's forward-looking stance made structural. Placing them in core means
every consumer of GMEOW meets self-assertion, suppression-not-erasure, and falsehood-as-refuted-
claim as first-class citizens, not as an ideology pack they can decline to install. We name this
plainly: it is a deliberate commitment, encoded where it cannot be silently dropped.

An extension is the existing slice convention made physical: one directory, one manifest (the
Principle 15 consumer named in a machine-checked field), compiled, reasoned (extension ∪ core),
and drift-gated as a unit (Principle 7), and distributable as a signed single-file GTS bundle
(Principle 14's format, the GTS specification (`docs/GTS-SPEC.md`) § 12.1, § 13). This inverts
"ontology explosion" from threat into growth mechanism: enthusiasm for a new domain has
somewhere to go that is not the core. Extension *ecosystem* machinery (SDK, catalog,
submission process) is itself subject to Principle 15 — built when a named external extension
author exists, not before.

*Embodied in:* the slice architecture (`slices/<group>/<name>/` — the manifest is
the sole tier truth); `slices/vocabulary.ttl`; the GTS `bundle` profile. *Tested by:* per-extension compile / reason / drift gates;
the manifest consumer field, checked via the Principle→enforcement manifest.

## 17. The logic itself is canonical — OWL is a projection of it, not its ceiling

> **The logical core is authored in a maximally expressive, RDF 1.2-native logic (`logic:`);
> OWL, Datalog, SHACL, Prolog, N3, and the gUFO / BFO / DOLCE upper ontologies are generated
> lossy projections of it. Decidability is a property of a projection or a declared profile —
> never a cap on what the canonical model may say.**

This is Principle 4 carried to its conclusion: the projection doctrine reaches past the statement
layer and the surface vocabularies into the **TBox, the rule layer, and the foundational ontology**
themselves. Principle 3 already anticipated it — the OWL form "recedes naturally as RDF-1.2-native
reasoners and stores arrive"; `logic:` is that reasoner, and OWL 2 DL / EL become two projection
profiles among many. GMEOW models the logic correctly once (Principle 1) and hands every weaker
formalism a generated, drift-gated, loss-ledgered view (Principles 4, 7).

`logic:` is deliberately **Turing-complete**: a computational substrate, not merely a description
language. The halting problem is the accepted shadow of that choice, not a defect — managed
honestly, never hidden: termination and tractability are **projection/profile guarantees** (a
consumer buys decidability by projecting down or by certifying a profile), and when a budget is
exhausted the solver returns `unknown` / `incomplete`, never a false answer. Expressivity is never
overclaimed (the Principle 1 discipline applied to the logic): a triple term *groups* a statement,
it does not assert it; `gmeow:confidence` is not a probability unless a mapping is declared; and
procedural `cut` is not part of the canonical truth theory.

The **foundation** follows the same doctrine. UFO⁺ is authored in `logic:`; **gUFO is its primary
generated down-projection** (the OWL realization of the same UFO lineage), while BFO, DOLCE, and
SUMO are generated **bridge views, not truth-preserving projections** — Principle 5's by-reference
grounding, made explicit in the loss ledger. The OntoUML discipline that lived in external lint
becomes **actual axioms**; the lints survive as projection-conformance tests over the gUFO downcast,
so nothing is lost in the move from lint to logic.

The native `logic:` solver is the **reasoning authority** and the single reasoner on-gate; the OWL projection and the Datalog / SHACL
engines become **secondary validators of their projected fragments**. This supersedes the
OWL-2-DL-core framing of Principles 2, 8, and 12 (annotated there). Correctness is **verified by
construction** (Principle 7): the **Rust core is canonical** (the native gmeow RDF 1.2,
SPARQL, and relational logic engines — the Principle 13 tool pattern). Committed regression
goldens and external corpora remain engine-independent evidence, never self-authored truth;
retained comparison engines must agree with the Rust core on their shared fragments but are not
the spec. The reasoner remains *our* quality assurance,
never the consumer's prerequisite (Principle 13): the canon is maximal; the projections are what
anyone else consumes.

*Embodied in:* the `logic:` implementation — the [`crates/logic`](./crates/logic) Rust core
(the reasoning authority), authored from the GMEOW Logic design set
([`slices/grounding/logic/design/LOGIC.md`](./slices/grounding/logic/design/LOGIC.md) and its semantics /
runtime / migration / conformance siblings). The logic EPIC has landed. *Tested by:* the logic
conformance corpus (native solver ≡ committed goldens,
Principle 7 — `meta:gate-logic-conformance`), the `logic:` → OWL / Datalog / N3 / gUFO round-trip
isomorphism gate (`meta:gate-logic-round-trip`), and the foundation-conformance gate (the gUFO
downcast passes the native `crates/validate/src/gufo.rs` reasoning invariants — `meta:gate-foundation-conformance`).
The machine-readable enforcement lives in
[`governance/constitution.ttl`](./governance/constitution.ttl).

---

## 18. The reference RDF-1.2 stack — complete, coherent, and Docker-free

> **The authoritative gate runs the native `logic:` solver and nothing heavier: `make check`, CI,
> the build, and runtime need no Java and no Docker. The native reasoner is the single reasoning
> authority; there is no live second reasoner on-gate — only committed engine-independent goldens
> remain as external evidence.**

Principle 17 already settled *authority* — the native `logic:` core is the reasoner, OWL is a
projection. This principle settles the *gate*: the consequence of that authority is that the
machine which proves GMEOW correct carries no external runtime. The native EL/DL reasoning lane
(`gmeow_logic.reason_native`, bound by PyO3) reasons the committed bundle in-process, emits the
told-vs-inferred closure with per-triple derivation provenance, the per-axiom proof skeletons, and
the native gap-zero DL⊇EL divergence ledger — and every one of those artifacts is produced and drift-gated
without spawning a container or a JVM. This extends Principle 13's Docker-free *consumer* gate to
the *authoring* gate as well: the reasoner is no longer a heavyweight release-only step.

The *secondary validators* Principle 17 names are the OWL / Datalog / SHACL projected fragments and
the committed engine-independent goldens — **not** a live second reasoner. The committed divergence
ledger (`generated/logic/dl-el-crosscheck-report.ttl`) is built from the native results **only**: it
records the native consistency verdict, the native-only subsumption entailments, and the native DL
coverage defect count, which must stay zero for the committed bundle — a native DL⊇EL fragment
comparison, with no external oracle. The retired live native-vs-`purrdf::entail` differential
cross-check is gone; the authoritative gate stays green offline, on any machine, with no privileged
daemon and no second reasoner running beside the native lane.

This extends Principle 17 (native authority) and Principle 13 (the consumer Docker-free gate) to the
authoritative gate; the release-as-evidence and reusable-crate-suite clauses are realized below, and a
later amendment appends the public-receipts clause.

**Extends Principle 17 and Principle 13.**

**Amendment — the native reasoner is the single authority; the live cross-check oracle is retired.**
The split foreshadowed above once ran an oracle comparison in a separate Java+Docker lane, and later
as an in-process `purrdf::entail` differential cross-check; **both** have now been retired. The
authoritative path — `make check`, the required CI `quality` gate, the build, and runtime — is
rust-first and carries **no Java and no Docker**: native EL/DL reasoning (`reason --mode native`),
the native OWL 2 RL closure (`reason/rl.rs`, replacing the `owlrl` baselines), native RDF-1.2
emission (`gmeow-rdf`), and native SHACL/validation. There is **no live second reasoner on-gate**:
the native `logic:` solver is the single reasoning authority, and the only external evidence
retained is a set of **committed engine-independent goldens** — the offline frozen OWL 2 DL
oracle-gold corpus (`native ⊇ frozen-gold`, proven under `make conformance`) and the native gap-zero
DL⊇EL ledger. This allowed/forbidden boundary is machine-checked: a committed golden is permitted; a
live external/second reasoner wired on-gate as a differential subsumption/entailment oracle is
**forbidden** and cannot silently regrow (the single-authority seal). The committed
`dl-el-crosscheck-report.ttl` stays native-built (a native DL⊇EL fragment comparison, Docker-free)
with `gapCount` required at zero. Producer inversion of the RDF-1.2 codec is **done**: the statement
lead artifact (`generated/statements/gmeow.rdf12.ttl`) is written natively by `gmeow-rdf`
(`gmeow_rdf.project_statements_rdf12`), so the build, `make check`, and `sync` paths carry **zero
Java and zero Docker** on the statement path.

*Embodied in:* the native reason lane ([`crates/logic/src/reason/mod.rs`](./crates/logic/src/reason/mod.rs)),
the `reason --mode native` CLI command, the `reason` registered pipeline generator, and the native
gap-zero DL⊇EL ledger builder
([`crates/logic/src/reason/artifacts.rs`](./crates/logic/src/reason/artifacts.rs)). *Tested by:* the
native-reasoning authority gate (`meta:gate-reason-native`), the native gap-zero divergence ledger
gate (`meta:gate-dl-el-crosscheck`), the native ⊇ frozen-oracle-gold anti-regression superset gate
(`meta:gate-native-oracle-superset`, an offline committed golden), and the executable
single-authority + lane-purity seal
([`crates/validate/src/repo_static.rs`](./crates/validate/src/repo_static.rs),
`meta:tests-lane-purity`) that statically proves the required CI `quality` jobs and
`make check` carry no Java, no Docker, and no live differential reasoning oracle — whose
machine-readable enforcement lives in
[`governance/constitution.ttl`](./governance/constitution.ttl).

**Amendment — release-as-evidence.** A GMEOW release is its own evidence. The `make full-release`
lane runs, in order, the native authority gate (`make check`), and the public conformance + perf
suites — then **folds every
result into a single signed release bundle as content-addressed, individually-verifiable attestation
frames**. Each artifact the lane produces — the native gap-zero
DL-EL ledger, the public conformance-suite verdicts
(the `gmeow-conformance` corpus rolled up by `make conformance-report`), the SHACL/diagnostics SARIF
findings, the machine-readable compliance report, and the perf results — rides into the bundle as a
BLAKE3-content-addressed blob, and is described by a queryable `gmeow:Attestation` envelope (from the
`attestation` slice) in a dedicated `graph/attestations` named graph that binds the envelope to its
blob by `gmeow:contentDigest`. A blob the committed snapshot already carries (e.g. the SHACL SARIF) is
attested by digest in place, never re-folded as a duplicate frame. The bundle is **signed** with the
Ed25519 release key and carries the embedded OpenPGP transport key, so any consumer can verify the
signature and the trust policy out of band, then walk the attestation frames to confirm exactly which
checks ran over exactly which bytes — which is precisely what `make verify-release` does: native COSE
signature + trust-policy verification, then a walk of the `graph/attestations` frames asserting every
attested `gmeow:contentDigest` resolves to a blob actually present in the bundle. A verified bundle is
then **published** by the user-driven `make release-publish`: a content-addressed GitHub release (the
signed `.gts` + a checksum sidecar + the native `gts heads` digest) and the Crossref deposit over the
always-latest concept DOI. Signing, publishing, and DOI submission are maintainer-credentialed steps;
the project tooling never holds signing keys. The fold is **reproducible**: release timestamps are injected
rather than sampled, blobs and frames are content-hash-sorted, and perf timings are carried as data,
never as a gate. Two invariants bound this lane. First, the release fold adds no external runtime:
there is no Java/Docker oracle pass and no live second reasoner — the native `logic:` lane keeps
Principle 18's authoritative path native and offline. Second, the release fold writes only the
release bundle (`dist/gmeow.gts`, the `--out` path) and **never mutates the committed, drift-gated
`generated/dist/gmeow.gts`**: the committed snapshot remains the Docker-free narrow waist, and the
signed evidence bundle is a packaging step layered over it. Perf and timing are folded as data, so
the bundle attests *what was checked*, not a leaderboard verdict. This realizes the reference-stack
program's "signed, content-addressed, provenanced graphs" moat as a shippable artifact: the release
graph carries its own proof of correctness.

*Embodied in:* the Rust-native release fold + sign + verify stage
([`crates/pipeline/src/stages/release.rs`](./crates/pipeline/src/stages/release.rs),
`fold_release_bundle` + `verify_release_bundle`) over the snapshot composer's signing path
([`crates/rdf/src/gts_compose.rs`](./crates/rdf/src/gts_compose.rs), `emit_gts`) and the native verify
path ([`gmeow_gts::verify`]), the thin `release-bundle` + `verify-release-bundle` CLI commands, the
Rust `conformance-report` roll-up ([`crates/conformance`](./crates/conformance)), the `make
full-release` / `make verify-release` / `make release-publish` lanes, and the release-evidence frame
schema in the [`attestation`](./slices/core/attestation/module.ttl) slice. *Tested by:* the
release-evidence gate (`meta:gate-release-evidence`) — the Rust round-trip test that folds the evidence
set, signs the bundle, and verifies the signature, trust policy, and `graph/attestations` frames, plus
the consumer verify tests (well-formed bundle accepts; tampered bytes, non-GTS garbage, and an
untrusted key reject) — surfaced to consumers as `make verify-release`.

**Amendment — the reusable crate suite.** The stack that proves GMEOW correct is *also* a reusable
RDF-1.2 toolkit in its own right, published as ontology-independent crates: the RDF-1.2 value model,
the immutable IR, the logic compiler, and the SHACL and reasoning engines carry no dependency on the
GMEOW ontology graph, so an external consumer can parse, reason over, and validate RDF-1.2 with **no
GMEOW ontology present**. The structural guarantee is the oxigraph-free, PyO3-free kernel
(`gmeow-rdf-core`): the reasoning and validation cores compose over its trait seams, never over a
loaded GMEOW graph, and the pure compiler (`gmeow-logic-compile`) is wasm-able with zero
reasoning-runtime dependencies. The same cores reach every major runtime through stable reuse
surfaces — a semantic C-ABI (`gmeow-rdf-capi`, `libpurrdf`) and a wasm / RDF-JS build
(`gmeow-rdf-wasm`) — not Rust alone. This makes the reasoning stack a *shared* foundation: the GMEOW
ontology is one consumer of the crates, never a prerequisite for them.

*Embodied in:* the oxigraph-free kernel ([`crates/rdf-core`](./crates/rdf-core)), the pure wasm-able
compiler ([`crates/logic-compile`](./crates/logic-compile)), and the C-ABI / wasm reuse surfaces
([`crates/rdf-capi`](./crates/rdf-capi), [`crates/rdf-wasm`](./crates/rdf-wasm)). *Tested by:* the
crate-dependency ring-fence gate (`meta:gate-rdf-core-hygiene`, `make rdf-core-hygiene`) — it proves
`gmeow-rdf-core` never regains an `oxigraph` normal dependency, keeping the kernel, and the cores that
compose over it, reusable with no ontology loaded.

---

## 19. Three co-foundational grounding layers — reasoning, structure, and meaning

> **GMEOW grounds itself in three co-foundational layers, each a maximal canonical substrate in
> its own namespace: `logic:` grounds reasoning (truth, inference, proof, modality), `math:`
> grounds quantity and structure (number, space, operation, measure, dimension), and `lang:`
> grounds meaning and expression (sign, form, denotation, interpretation, translation). None
> reduces to the others; the layers interlock only through registered seams; and no domain slice
> re-derives what a grounding layer owns.**

Principle 17 established the pattern for one layer: author the maximal canon, project lossily,
record the loss. This principle generalises it to the complete grounding architecture. The same
doctrine that makes OWL a projection of `logic:` makes MathML, OpenMath, RDF Data Cube, and QUDT
projections of `math:`, and OntoLex-Lemon, Universal Dependencies, TEI, and BCP-47 projections of
`lang:` — every projection carrying a preservation judgment in the same loss ledger (Principles 4
and 7). Almost every real artifact composes all three layers: a claim is a linguistic act
(`lang:`) carrying logical content (`logic:`) that is often about quantities (`math:`) — "*the
p-value was 0.03*" is a sentence realizing a form that denotes a formula that references a framed
measure.

**The layers interlock through registered seams, and the three form one co-foundational
kernel.** A `math:` expression denotes into a `logic:` term; a `lang:` form denotes into a
`logic:` formula through a reified, kind-typed denotation record; `logic:` never depends back on
either. Between `lang:` and `math:` the seams run both ways and are few: mathematical notation is
realized on the general `lang:` rendering theory (`math:ExpressionRendering` ⊑ `lang:Rendering`),
and `lang:`'s own declared and measured magnitudes ground in `math:Quantity` — because rendering
is language and a dimensioned magnitude is mathematics, wherever each appears.
Formal-language-theoretic facts about grammars (generated languages as sets, automaton
equivalence) remain `math:` objects referencing `lang:` individuals, exactly as `logic:` keeps its
quantitative facets abstract for `math:` objects to satisfy. The kernel is therefore a declared
peerage (`gmeow:sliceCoFoundationalWith`), not an internal dependency ladder: `gmeow:sliceDependsOn`
stays acyclic everywhere, every inter-layer reference must land on a seam registered in the
grounding contract ([`docs/GROUNDING.md`](./docs/GROUNDING.md)), and a new seam is a
constitutional change, never a convenience edit.

**Each layer carries a signature never-conflate rule** — the Principle 1 never-overclaim
discipline made structural per layer: in `logic:`, a triple term *groups* a statement and does
not assert it (Principle 17); in `math:`, a probability is not a confidence; in `lang:`, form is
not sense and sense is not reference. And each layer is held to maximal dogfooding: `logic:`
grounds GMEOW's own axioms and foundation, `math:` its own quantities, counts, and probabilities,
`lang:` its own prose, labels, documentation trees, and serialization grammars.

Domain slices stand **on** the layers, never beside them: `observations`, `notation`, `language`,
`names`, and their peers graft onto the grounding substrate, and where a grounding-layer term
strictly supersedes a domain term the inferior term is removed (Principle 6) — compatibility for
consumers lives in the generated projections (Principle 4), never in the canon.

**Extends Principle 17.**

*Embodied in:* the three grounding design sets —
[`slices/grounding/logic/design/LOGIC.md`](./slices/grounding/logic/design/LOGIC.md),
[`slices/grounding/math/design/MATHEMATICS.md`](./slices/grounding/math/design/MATHEMATICS.md),
and [`slices/grounding/lang/design/LANG.md`](./slices/grounding/lang/design/LANG.md), with their
sibling charters — the `slices/grounding/` group is the layers' home (the group segment is human
organization the build never reads); all three grounding charters — `logic`, `math`, and `lang` —
now live under `slices/grounding/`. *Tested by:* for the `logic:` layer, the Principle 17 gates
(`meta:gate-logic-conformance`, `meta:gate-logic-round-trip`); for the `math:` layer,
`meta:gate-math-conformance` — the native dimensional/ingest structural invariants plus the
flagship-manifest capstone over the slice competency corpus; and for the `lang:` layer,
`meta:gate-lang-conformance` — the native form, meaning, translation, projection, and ingestion
invariants over the slice competency and counter-example corpus. All three layers are now landed
and gate-enforced; the design-set review practice that stood in while `math:` and `lang:` were
design-stage is retired.

---

## Amending this Constitution

These principles are amended only by the project owners (see
[`CONTRIBUTING.md`](./CONTRIBUTING.md) § Governance and continuity), through an explicit pull
request that edits this file. A design change that conflicts with a principle either changes to
comply or ships *together with* the amending pull request — it is never merged in silent
conflict. Principle numbers are stable identifiers: additions append; existing numbers are not
reused or reshuffled casually, so "Principle N" stays meaningful across history.

The amendment process has a mechanical counterpart:
[`governance/constitution.ttl`](./governance/constitution.ttl) restates each principle's
enforcement as machine-readable RDF, and `gmeow constitution-check` (in `make check`) fails
when a principle loses its last enforcing gate, cites an artifact that no longer exists, maps
an enforcement to no principle, or drifts from this document's headings or from the live
generator registry. Amending a principle without updating its enforcement fails CI.

---

## Recurring modelling patterns (non-normative)

These are not principles but the reusable shapes the principles keep producing — named here so designs
can cite them and stay consistent. They are guidance, not commitments.

- **The Profile pattern** — model an open-but-structured facet as a *closed descriptor schema + open
  values + self-description (reflection)*, so extensibility is by construction and a "novel-value" guard
  can prove it. Concretised in `ontology/modules/profiles.ttl` as `gmeow:Profile`, and seen in
  the four-clocks (temporal provenance), the reference-frame Profile (Principle 11), and the temporal
  scale/calendar Profile.
- **Flat-first, reify-on-demand** — pair a flat shortcut for the common case with a reified relator that
  carries statement-level metadata when it is needed (`gmeow:hasLicense` ↔ `gmeow:License`;
  `containedInPlace` ↔ a containment tenure; `hasParticipant` ↔ `Participation`). The flat form keeps the
  80 % case simple; the relator is promoted only when period, role, confidence, or standpoint matters.
