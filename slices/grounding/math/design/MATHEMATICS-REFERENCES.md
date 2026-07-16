<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — External References and Subsumption Posture

> The **references appendix** of the GMEOW Mathematics design set: an enumerated, classified survey
> of the external mathematical, statistical, and metrological standards, ontologies, formalisms,
> classification schemes, and engines the mathematics grounding layer subsumes, projects to, links,
> or cites. It is the mathematical peer of `slices/grounding/logic/design/LOGIC-REFERENCES.md`, and its
> records are staged for the `metadata/references.ttl` ledger. It is the evidentiary base for the
> subsumption plan and for every alignment recorded as a `logic:Correspondence`
> (`slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md`).
>
> **Reading this appendix.** Each record is `Name — relationship | license | status — note`. Survey
> data was web-verified July 2026; where a license or maintenance state could not be confirmed it is
> marked *unverified*. This is a landscape map, not an endorsement; inclusion records that a system
> exists and where it sits relative to the grounding layer.

## The classification axes

Every external system is classified on three axes, and the **relationship** is the resultant of all
three — not expressivity alone.

**Relationship (what GMEOW does with it):**

- **Subsume (S)** — lift its content into the canonical `math:`/`logic:` model as a fragment; GMEOW
  becomes the richer local source of truth. Requires permissive licensing *and* liftable structure.
- **Project (P)** — generate a lossy lowering *out* to it, carrying a `logic:preservationKind` in
  the loss ledger. GMEOW is canonical; the target is a consumer surface.
- **Link (L)** — align by reference (`gmeow:TermEquivalence` → `logic:Correspondence`) or carry a
  bare external identifier/QID. No content is imported; identity is anchored.
- **Reference (R)** — cite as theory, engine, prior art, or definitional authority. Not aligned as
  data; often forced by a restrictive license or the absence of a machine-readable form.

**License gate:** permissive (CC0/CC-BY/Apache/BSD/MIT) → *may* be subsumed/folded; share-alike or
**NC** (CC-BY-SA, CC-BY-NC-SA) → align/link only, never folded into the permissive bundle;
closed/subscription/restricted-prose → reference/link only.

**Kind gate:** a *concept scheme / formal vocabulary* can be aligned concept-to-concept; a *bare
identifier scheme* (zbMATH IDs, DLMF equation IDs, OEIS A-numbers) is a locator, link-only; a
*prover library* holds content but not RDF, so it is referenced, not projected.

## Primary anchors

Three systems carry most of the load and should be treated as the spine of the reference layer:

- **Lean mathlib** (Apache-2.0, ~210k theorems, active) — the richest permissive reference for
  algebra depth (Lie algebras, root systems, the typeclass hierarchy) **and** proof-as-graph (the
  declaration dependency DAG). Not RDF-projectable, so **L/R**: mirror its structure, align concept
  identity, cite proofs.
- **Wikidata** (CC0) — the identity hub. QIDs already carry the bridge properties (P3285→MSC,
  P829→OEIS, P1556→zbMATH), so one QID reconciles MSC codes, OEIS numbers, zbMATH IDs, and DLMF
  references to a single `math:` concept. **L**, and the pivot for every other link.
- **QUDT + DCC/D-SI** — QUDT (CC-BY-4.0) for units/quantity-kinds/dimensions; the PTB **DCC**
  (LGPL-v3) + embedded **D-SI** (LGPL) for the calibration-certificate + traceability + unit-
  uncertainty structure. **P/L** for units, **P** for the metrology projection.

A private 95-topic mathematics snapshot was also used as a one-time coverage probe. Its source
identity, checkout path, and revision are intentionally not published and it is not an ontology
dependency. The anonymized decisions are recorded in
[`MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md`](MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md); all minted
identities are general `math:` terms with independent definitions and public authority links.

### Realized grounding disposition

The external survey is discharged by one of the following concrete treatments;
there is no unqualified deferral bucket.

| Treatment | Families |
|---|---|
| Shipped grounding correspondences | The existing `mappings/equivalences.ttl` identity/reference catalog (including Wikidata, mathlib, DLMF, OEIS, QUDT, D-SI, OM 2, and symbol-level OpenMath targets), the logic-owned SUMO catalog's broad validation-only bridge from `math:Quantity` to `sumo:Quantity`, the six math-owned SOSA / OM 1.8 / IVOA ObsCore / LOINC / QUDT quantity-and-value rows in `mappings/quantity-bridges.ttl`, plus the Data Cube, STATO, OBCS, SIO, and OBI rows in `mappings/statistical-bridges.ttl`; the OBI data-transformation row is intentionally only `skos:relatedMatch` because OBI names an executed process |
| Generated codec or consumer projection | DCC/D-SI certificate surfaces, RDF Data Cube emission, MathML, OpenMath content, tabular/statistical interchange, and other formats whose structure is a lossy output rather than a term identity |
| Citation, identifier, or registry linkage only | UCUM, UO, VIM/GUM, MSC, arXiv, zbMATH/MR, proprietary or restricted resources, and non-RDF prover libraries; identifiers may be carried, but their content is not imported |
| Native authorship | Mathematical structures and preservation laws for which no adequate external ontology exists; these remain `math:` terms rather than receiving a fabricated alignment |

The live rows are `logic:GroundingCorrespondence` records, oriented from
`math:` and shipped in `graph/correspondence-laws`. A codec projection does not
substitute for such a row, and a survey citation does not claim one exists.

## Cluster A — Units, quantities, dimensions

- **QUDT** — P/L | CC-BY-4.0 | active (v3.1.4+) — primary units spine; encodes conversions as
  data/SHACL, not reasoned theorems (GMEOW's differentiator).
- **OM 2 (Ontology of units of Measure)** — L | CC-BY-4.0 | active, single-maintainer — richest
  unit/quantity lattice + scale-vs-unit distinction; carries a stale pre-2019 kg; align, don't spine.
- **UCUM** — L | restrictive (no-derivative) | active — parse/emit unit codes only; license bars
  subsuming.
- **UO (Units Ontology, OBO)** — L | CC-BY-3.0 | dormant — map to OBO/PATO IRIs; shallow dimensional
  semantics.
- **SWEET** — P/L | CC0 | active — broad earth-science quantity/units modules; shallow, no
  dimensional theorems.
- **BIPM SI Digital Framework (D-SI PIDs)** — L | CC-BY-3.0-IGO | active — the *authoritative* SI
  anchor; cite its PIDs, do not re-mint.
- **NIST UnitsDB / UnitsML (+ SP 811)** — S/P | CC-BY-4.0 | active — subsume the unit table; re-export
  UnitsML for tooling.
- **MUO** — R | unlicensed/dead | dormant — historical UCUM-instance IRIs only.
- **cdt (UCUM literal datatypes)** — P | CC-BY-4.0 spec | dormant — lossy literal-encoding export.
- **OBOE + OM measurement pattern** — S/L | CC-BY-3.0 / CC-BY-4.0 | dormant/semi-active — subsume the
  measure lattice; align the observation pattern.

## Cluster B — Statistical data / dataset / cube / observation

- **RDF Data Cube (QB)** — P | W3C | stable-frozen (2014) — pragmatic, widely-tooled cube projection;
  structural constraints only, no DL.
- **SDMX 3.0 / SDMX-RDF** — L | free spec | active — align codelists/DSDs; GMEOW cannot own sponsor
  registries.
- **DDI-CDI (+ Codebook/Lifecycle)** — P/L | CC-BY-4.0 | active (v1.0, Jan 2025) — the *richest*
  structured-statistics projection target (wide/long tables, process/provenance).
- **DCAT 3 / DCAT-AP** — P | W3C | active — catalog-level projection.
- **CSVW** — P | W3C | stable-frozen — tabular annotation/transform; GMEOW subsumes and generates.
- **Frictionless Table Schema** — P | MIT | active — lightweight tabular publishing target.
- **Croissant** — P | CC-BY-ND spec / Apache code | active — the ML-ready-dataset projection.
- **schema.org Dataset** — P | CC-BY-SA | active — discovery markup; deliberately shallow.
- **VoID** — L | W3C | dormant — dataset/linkset descriptors.
- **PROV-O** — L (import/reuse) | W3C | active — provenance; reuse, do not re-invent.
- **SOSA/SSN** — L | W3C/OGC | active — the observation pattern; intentionally lightweight.
- **DDI-RDF Discovery (Disco)** — L | CC-BY-3.0 | low-activity — dataset/variable discovery metadata.
- **Wikidata statistical datasets (.tab)** — L/P | CC0 | active — link QIDs, ingest `.tab` as instance
  data.

## Cluster C — Classification / subject taxonomies

- **MSC2020 (+ MSC2020-SKOS)** — L | CC-BY-NC-SA | active — the field's lingua franca; **NC clause →
  align-to, never fold**.
- **arXiv math.\* taxonomy** — L | CC0 labels | active — coarse category tags.
- **ACM CCS (Mathematics of computing)** — L/S | ACM-free SKOS | active (2012) — cleaner license than
  MSC for the CS-adjacent axis.
- **zbMATH Open** — L | CC-BY-SA / CC0 bib | active — identifier + bibliographic authority; public
  API.
- **Mathematical Reviews / MathSciNet** — L | subscription | active — MR numbers / MR Author IDs only.
- **DDC (510) / UDC (51) / LCC (QA)** — L | proprietary / CC-BY-SA Summary / open (id.loc.gov) — coarse
  shelf-level crosswalks only.
- **Wikidata** — L | CC0 | active — the QID identity hub (see Primary anchors).
- **DBpedia** — L | CC-BY-SA | active — the sole native-RDF encyclopedic peer.
- **PhySH (Physics Subject Headings)** — S/P | CC0 | active (v2.8.0) — CC0 physics subject axis for a
  downstream physics slice.

## Cluster D — Content markup & theory interchange

- **OpenMath (+ Content Dictionaries)** — S | royalty-free | standard active, CDs dormant (2022) —
  subsume compositional expression/symbol semantics; grounding rows target canonical symbol IRIs
  declared by the dictionaries (`http://www.openmath.org/cd/{cd}#{symbol}`). A dictionary HTML page
  is only a `skos:relatedMatch` when a broad local class spans several symbols and no unique symbol
  target is honest.
- **Content MathML** — S | W3C | MathML 4 = draft (2026-06), MathML 3 = Rec — subsume into the AST
  (mirrors OpenMath trees).
- **Presentation MathML** — P | W3C | as above — the notation projection surface.
- **OMDoc** — L | CC-BY-SA-2.5 | dormant (→ MMT) — theory-graph structure reference.
- **MMT (MathHub)** — L | custom BSD-ish | active (v27, 2025) — foundation-independent theory graphs;
  align, not RDF.
- **sTeX** — R | LPPL | active — author-side semantic annotation over prose.
- **LaTeXML** — R | CC0/PD | active — a TeX→XML/MathML conversion *tool* (an ingest lifter reference).
- **SCSCP** — R | OpenMath license | dormant — CAS RPC protocol over OpenMath.

## Cluster E — Formal proof libraries / interactive theorem provers

- **Lean + mathlib** — L/R | Apache-2.0 | very active — **primary algebra/proof anchor** (see above).
- **Metamath (set.mm)** — S | CC0 | active — subsume the finest-grained proof-step + axiom→theorem
  DAG (CC0 makes it foldable).
- **Mizar / MML** — L/R | dual GPL/CC-BY-SA | active — FOL + soft types; align/cite.
- **Coq/Rocq stdlib + Mathematical Components** — L/R | LGPL-2.1 / CeCILL-B | active (now *Rocq* 9.0)
  — dependent-type library; reference.
- **Isabelle/HOL + AFP** — L/R | BSD-ish | active — HOL developments + author/date metadata.
- **HOL Light** — R | BSD-2 | active — LCF-kernel HOL.
- **Agda standard library** — R | MIT | active — proofs-as-programs.
- **Dedukti / Logipedia** — R | CeCILL-B / unverified | active — λΠ-modulo universal proof encoding.
- **OpenTheory** — R | MIT | active — versioned HOL proof packages.
- **TPTP / TSTP** — L/R (SZS status → S) | non-OSI custom | active (v9.1.0) — ATP corpus + the SZS
  status vocabulary (subsumable); problems link-only.
- **Formal Abstracts** — R | unverified | experimental — Lean-based math KG, no released library.

## Cluster F — Computer algebra (interchange / category systems)

- **SageMath category framework** — R | GPLv2+ | active — runtime parent/element/category lattice; no
  serialized ontology or URIs.
- **GAP type/filter system** — R | GPLv2+ | active — runtime method-selection type system.
- **Macaulay2** — R | GPL | active — generic JSON serialization; no formal category ontology.
- **SymPy assumptions** — R | BSD-3 | active — 3-valued predicate system; runtime only.
- *Note:* cross-CAS interchange in this ecosystem already flows through OpenMath/MMT — target those,
  not the CAS runtimes.

## Cluster G — Encyclopedic / reference knowledge bases

- **OEIS** — L | CC-BY-SA-4.0 (relicensed ~Feb 2023; the old CC-BY-NC is stale) | active — A-numbers.
- **NIST DLMF** — L/R | NIST restricted (no bulk redistribution) | active — equation IDs; special-
  function authority.
- **Wolfram MathWorld** — R | proprietary | active — reference/authority prose.
- **Wolfram Language Knowledgebase** — R | proprietary | active — closed typed entity-property KB.
- **nLab** — L/R | no formal license (attribution by convention) | active — category-theory/physics
  page URLs.
- **PlanetMath** — L | CC-BY-SA | low-activity — MSC-tagged encyclopedic entries.
- **ProofWiki** — L | CC-BY-SA-3.0 | active — structured proof/definition/theorem pages.

## Cluster H — Statistics & probability domain ontologies

- **STATO** — L/P | CC-BY-3.0 | active low-cadence — statistical tests/distributions/study-design;
  names methods, cannot carry distribution math.
- **OBCS** — L | CC-BY-4.0 | dormant (~2018-20) — statistics procedures vocabulary.
- **SIO** — L | CC-BY-4.0 | active — a few statistical-measure classes; broad-but-shallow.
- **IAO** — L | CC-BY-4.0 | active — "data item / measurement datum" scaffolding.
- **OBI** — L | CC-BY-4.0 | active — the one place **PCA is semantically named** (a data-transformation
  process); align.
- **OntoDT** — S | open (unclear) | dormant — ISO-11404 datatype + measurement-scale taxonomy
  (nominal/ordinal/interval/ratio); subsume the scale primitives.
- **EXPO** — R | unclear | dormant — SUMO-linked experiment methodology; link-only.

## Cluster I — Probability & distribution catalogs

- **ProbOnto** — S | article CC-BY-4.0 / ontology unclear | dormant (frozen v2.5, 2017) — ≥150
  distributions + reparameterizations; the rich math is in PowerLoom, the OWL only partially covers
  it — **subsume the content, cite, don't embed the file**.
- **Distributome** — S | open (unpinned) | dormant — inter-distribution relation graph (limiting /
  special-case); bespoke XML, no RDF identity.
- **UncertML** — R | no license | dormant (only ever an OGC Discussion Paper) — XSD uncertainty
  values.
- **W3C URW3 uncertainty ontology** — S | W3C | historical (2008) — thin taxonomy of uncertainty
  *kinds*; absorb outright.
- **Bayesian-network ontologies (PR-OWL / BayesOWL / MEBN / OntoBayes)** — R | varies | semi-dormant —
  research artifacts, mutually incompatible; heavyweight MEBN hard to project.

## Cluster J — Probabilistic / statistical model interchange (projection targets)

- **PMML** — P | DMG permissive | slow (v4.4.1) — trained-model scoring interchange.
- **ONNX** — P (metadata) | Apache-2.0 | active — tensor-op graphs; flat key-value metadata, no
  statistical semantics.
- **XMLBIF / BIF** — P | none stated | legacy (tool-supported) — discrete Bayesian networks, flat
  CPTs.
- **UAI file format** — P | public | active — anonymous-index factor tables for inference
  competitions.
- **Stan / BUGS** — R | BSD-3 / GPL | Stan active — modeling *languages* (an ingest-lifter reference,
  peer to the R-bridge).
- **ML-Schema (MLS)** — L/S | W3C-CG | dormant — ML experiment-metadata skeleton; thin enough to
  absorb.
- **MEX vocabulary** — S/R | open | dormant — PROV-O-based ML experiment metadata; superseded by MLS.
- **Model Cards / MLflow** — L | Apache / doc | mixed — governance/documentation layer.

## Cluster K — Metrology / calibration / uncertainty

- **VIM (JCGM 200:2012)** — R/L | JCGM restricted | stable (VIM4 drafting) — definitional authority
  for measurement terms; **cite, formalize natively**.
- **GUM (JCGM 100:2008)** — R/L | JCGM restricted | stable — Type A/B uncertainty + budget semantics;
  **author natively (neither QUDT nor OM models GUM budgets)**.
- **D-SI (Digital SI)** — P | LGPL | active (v2.2, PTB) — value±uncertainty+unit exchange; the native
  uncertainty-IR projection target.
- **DCC (Digital Calibration Certificate)** — P | LGPL-v3 | active — **the anchor for the miscalibrated-
  device case**: calibration state + traceability chain + unit-uncertainty (embeds D-SI).
- **DCC-terminology OWL (Sensors 2024)** — R | academic | research-stage — proof that GUM is OWL-
  formalizable; not a standard.

## Cluster L — Geometry / topology / manifolds (domain depth)

- **OntoMathPRO 2.0** — L | Apache-2.0 | maintained — bilingual math taxonomy; shallow label anchor.
- **OpenMath tensor1 CD** — P | open | static (2010) — tensor-notation symbols only (no
  manifolds/metrics).
- **MMT / OMDoc + LATIN2 (MitM)** — L | research | thin coverage — modular theory graphs; align at
  most.
- **Prover libraries (mathlib / AFP / Rocq)** — R | Apache/BSD | active — full C^∞ manifolds/bundles as
  *proofs*, not RDF.
- **Cohen-Steiner, Edelsbrunner & Harer (2007), "Stability of Persistence Diagrams"** — R |
  Springer (subscription) | definitional — Discrete & Computational Geometry 37(1), 103–120; the proof
  that the persistence-diagram map is 1-Lipschitz in the bottleneck distance. The external warrant for
  `math:bottleneckStabilityTheorem` (`math:cohenSteinerEdelsbrunnerHarer2007`); referenced as
  definitional authority, not projected.
- **Edelsbrunner & Harer, *Computational Topology: An Introduction* (AMS, 2010)** — R | book | current —
  the standard text for filtrations, persistence diagrams, and the stability line; the theory context
  `math:computationalTopologyTheory` names.
- **Persistent homology (Wikidata `wd:Q17099562`)** — L | CC0 | active — `math:Filtration` carries a
  `skos:closeMatch` alignment to this item (the family a persistence computation runs over, related
  to but not identical with the method the QID names — see
  [`mappings/equivalences.ttl`](../mappings/equivalences.ttl)). No dedicated Wikidata item exists for
  bottleneck distance specifically: the concept there is covered only by individual research-article
  items, not a definitional entry, so `math:bottleneckStabilityTheorem` carries no Wikidata alignment
  — its external warrant is the Cohen-Steiner–Edelsbrunner–Harer citation above instead.
- **Dedicated topology/manifold/chart/Lorentzian-metric ontology** — **none exists → GMEOW authors it.**

## Cluster M — Linear algebra / decompositions / PCA (domain depth)

- **OpenMath linalg1-6 + linalgeig2 CD** — P/L | open | maintained — matrices/vectors/determinant +
  `eigenvalue`/`eigenvector` symbols; **no SVD/PCA/covariance-operator**.
- **OBI** — L | CC-BY-4.0 | active — names PCA as a process (the only semantic anchor for PCA).
- **STATO** — P | CC-BY | active — coarse eigenvalue/variance-ratio labels.
- **PMML / ONNX** — P | permissive | active — MatMul/Gemm primitives; PCA lowers to derived fields.
- **SVD / PCA / covariance-operator / subspace / rank semantics** — **largely author natively.**

## Cluster N — Optimization (domain depth)

- **OSiL / OSrL / OSoL** — P | EPL-1.0 | dormant — LP/MILP/NLP/MINLP instance-serialization target.
- **OPTION ontology** — L | academic | low-activity — BFO-grounded benchmarking ontology (the only
  real OWL one).
- **MINLPLib / CUTEst / COCONUT / NEOS** — R | mixed | mixed — test corpora and services, not
  vocabularies.
- **Optimization problem/objective/constraint/solver structure** — **largely author natively.**

## Cluster O — Numerical analysis / floating-point (domain depth)

- **xsd:float / xsd:double** — S (use directly) | W3C | stable — the canonical RDF IEEE-754 FP
  datatypes.
- **IEEE 754-2019** — R | IEEE paywalled | current — the normative FP spec (not machine-readable).
- **xsd:precisionDecimal** — L | W3C Note | non-normative — optional decimal FP.
- **Discretization / convergence / stability / error-bound vocabulary** — **none exists → GMEOW authors
  it.**

## Cluster P — Information theory (domain depth)

- **QUDT quantitykind:InformationEntropy** — P/L | CC-BY-4.0 | active — entropy-as-a-unit
  (bit/nat/shannon) only.
- **Mutual information / KL divergence / cross-entropy / Fisher information geometry** — no
  structural ontology supplies the required frames, so GMEOW authors them; verified Wikidata QIDs
  provide identity anchors for the named measures.

## Cluster Q — ML geometry / KG embeddings / latent spaces (domain depth)

- **ML-Schema / MEX** — L | open | dormant — ML process/experiment layer.
- **Croissant** — P | Apache | active — ML dataset layer.
- **MCRO / Model Cards** — L | open | active-niche — model documentation.
- **KG-embedding methods (TransE, RDF2Vec, OWL2Vec\*, Box/EL)** — R | mixed OSS | active research —
  methods that *consume* ontologies to make vectors; none is an ontology *of* the latent space.
- **Latent-space / embedding-dimension / residual-meaning semantics** — **none exists → GMEOW authors it
  (novel; the KG-embedding-residual flagship).**

## Cluster R — Physics classification & spacetime (downstream physics slice)

- **PhySH** — S/P | CC0 | active — the physics subject axis.
- **SWEET** — L/P | CC0 | active — earth-science quantity/space/time modules.
- **QUDT quantitykind (physics)** — L | CC-BY-4.0 | active — deep classical/EM/thermo, sparse SR/GR.
- **IVOA STC** — L | IVOA open | stable (2007) — the real coordinate-frame/spacetime standard (XML).
- **EngMath (Gruber & Olsen 1994)** — R | historical | superseded — FOL scalar/vector/tensor
  quantities; prior art.
- **SR/GR spacetime ontology** — **none exists → a downstream physics slice authors it**, consuming the
  math-side manifolds/Lorentzian metrics.

## Where GMEOW authors from scratch — the original surface

The survey confirms these have **no external ontology** (only prose, prover libraries, or nothing).
Per `.goals`' super-ontology mandate, these are the grounding layer's original contribution, not
gaps to apologize for:

1. **Homomorphic-encryption / ring-homomorphism-under-encryption semantics** — flagship 2.
2. **KG-embedding latent-space / residual-meaning semantics** — flagship (KG/PCA), fully novel.
3. **Information geometry** — mutual information, KL/cross-entropy, Fisher metric.
4. **Numerical-analysis conceptual layer** — discretization, convergence, stability, error bounds.
5. **Optimization structure** — problems, objectives, constraints, solver handoffs (thin external
   anchors only).
6. **Differential geometry / manifolds / topology depth** — content locked in prover libraries;
   author the ontology, align to mathlib/OpenMath, cite Wikidata identity.
7. **SVD / PCA / covariance-operator / subspace / rank** — only *named* as coarse process labels
   externally.
8. **GUM Type A/B uncertainty budgets** — VIM/GUM are restricted prose; formalize natively, project to
   D-SI/DCC.
9. **Lie / root-system / Weyl-group depth** (E8) — align to mathlib; no ontology exists.
10. **The universal R → `math:` ⊕ `logic:` lifter** — no "R ontology" exists; the parser-compiler is
    authored.

## Subsumption-posture summary

| Relationship | Representative systems | License note |
|---|---|---|
| **Subsume (S)** | OpenMath/Content-MathML, Metamath (CC0), NIST UnitsDB, OntoDT scales, ProbOnto/Distributome content, URW3, PhySH, SZS status, `xsd:float/double` | permissive only |
| **Project (P)** | RDF Data Cube, DDI-CDI, Croissant, DCAT, CSVW, PMML, ONNX, XMLBIF/UAI, Presentation MathML, D-SI/DCC, QUDT | GMEOW canonical, target consumes |
| **Link (L)** | Wikidata (hub), MSC2020 (NC), zbMATH/MR/DLMF/OEIS IDs, arXiv, DBpedia, QUDT/OM units, STATO/OBI/SIO/IAO, PROV-O, SOSA/SSN, IVOA STC | includes all NC/identifier-only |
| **Reference (R)** | mathlib & the ITPs, TPTP, VIM/GUM prose, Stan/BUGS, CAS runtimes, nLab, MathWorld, EngMath | restricted or non-RDF |

Every alignment (S/L) is recorded as a `logic:Correspondence` with a `logic:preservationKind`; every
projection (P) carries its loss in the same ledger; every citation (R) is staged for
`metadata/references.ttl`. This appendix is the input to the subsumption plan.
