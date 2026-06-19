<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Diagnostics

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/diagnostics` · **tier: core**

The ontological face of GMEOW's first-class diagnostics system (#654). A validation violation, a
lint warning, a reasoning divergence, an external-tool failure — every diagnostic is modelled as a
**`gmeow:Finding`**, a specialization of `gmeow:Observation`, the universal claim construct. A
diagnostic is not a new kind of thing; it is the **observation pattern applied to tooling**: the
producing tool is the `gmeow:vantage` (who observed), the offending statement is the
`gmeow:observedFeature` (what — reached via `gmeow:findingLocation`), and the verdict is a
`gmeow:DiagnosticSeverity` (the `gmeow:observationResult`, reached via `gmeow:findingSeverity`).

## Projection, not source

The **canonical** diagnostics model is the PyO3-free Rust `gmeow_diagnostics::Report`. It must report
on a graph too broken to parse — a Turtle syntax error cannot be reported through an ontology that
will not load — so the canonical model deliberately is **not** RDF. This slice is one **projection**
of that report, a sibling of SARIF 2.1.0, flat JSON, static HTML, and the coloured CLI (Principle 4).
The projection is materialized as a `gmeow:graph/diagnostics` named graph embedded in the feedback
`.gts` bundle, so a validation report rides **with the data it describes** and is SPARQL-queryable —
"show me every error about a term in slice X" becomes a graph query, impossible against an opaque
SARIF blob.

## A finding IS an observation

Unlike the content-mode siblings elsewhere in GMEOW, `gmeow:Finding rdfs:subClassOf gmeow:Observation`
is a **real subsumption bridge**: a diagnostic genuinely is an observation, so it inherits the
vantage / observedFeature / observationResult roles and the EL mediation restrictions, and generic
"all observations about X" queries find diagnostics for free (Principle 9 — no diagnostic is
privileged; a finding is one tool's perspectival claim).

### gmeow:Finding

A single diagnostic as a reified `gmeow:Observation`. Project one `gmeow:Finding` per Rust
`gmeow_diagnostics::Finding`: bind `gmeow:findingSeverity`, `gmeow:findingCode`,
`gmeow:findingMessage`, the producing tool via `gmeow:vantage` (and `gmeow:findingTool` for the flat
name), and `gmeow:findingLocation` at the reified statement, hanging the GTS wire coordinates on the
location node. Findings are **regenerated, never authored** — editing one by hand would diverge from
the canonical Rust report. An EL `owl:someValuesFrom` restriction requires a finding to mediate at
least one `gmeow:DiagnosticSeverity`; the closed-world "exactly one" is `gmeow:FindingShape`.

## Severity is a value

`gmeow:DiagnosticSeverity` is an open value vocabulary (`gufo:AbstractIndividualType ⊑
gufo:QualityValue`) whose members are **individuals, never subclasses** (Principle 9), mirroring the
Rust `Severity` enum and the SARIF level.

| Value | Grade | SARIF level |
|---|---|---|
| `gmeow:severityError` | gate-failing | `error` |
| `gmeow:severityWarning` | surfaced, non-failing | `warning` |
| `gmeow:severityNote` | advisory | `note` |
| `gmeow:severityInfo` | informational | `note` |

### gmeow:DiagnosticSeverity · gmeow:severityError · gmeow:severityWarning · gmeow:severityNote · gmeow:severityInfo

The verdict-grade vocabulary and its four seeded individuals. Reference them from a `gmeow:Finding`
via `gmeow:findingSeverity`; the SARIF-level and CLI-colour mappings live in the projection layer,
not in an axiom. The vocabulary is open by convention: a deployment may add a grade as a new
individual without a schema change.

## Finding properties

### gmeow:findingSeverity · gmeow:findingLocation

`gmeow:findingSeverity` (`⊑ gmeow:observationResult`, range `gmeow:DiagnosticSeverity`) carries a
finding's verdict grade — a finding's severity **is** its observation result, so generic
observation-result consumers read it by inheritance. `gmeow:findingLocation`
(`⊑ gmeow:observedFeature`) anchors a finding to the statement it concerns — a finding's location
**is** what its observation is about. Its range is left **open** (like `gmeow:observedFeature`): the
value is typically a reified RDF 1.2 statement (`rdf:reifies <<( s p o )>>`) whose node carries the
wire coordinates, but per-kind narrowing is SHACL's job, not the core's.

### gmeow:findingCode · gmeow:findingMessage · gmeow:findingTool

The flat datatype surface, set verbatim from the Rust finding. `gmeow:findingCode` is the **stable
rule identifier** (the SARIF `ruleId`, the grouping/suppression key — e.g.
`"shacl.MinCountConstraintComponent"`); `gmeow:findingMessage` is the **human-readable** one-line
description for CLI/SARIF/HTML; `gmeow:findingTool` is the producing tool's **short name** (`"shacl"`,
`"validate"`, `"clippy"`). `gmeow:findingTool` is the cheap 80% provenance surface — the auditable
record is the `gmeow:ToolCall` the finding `gmeow:wasGeneratedBy`, whose `gmeow:usedTool` is the
validator agent (the agentic idiom, Principle 5, no forward output property); that same agent is the
finding's `gmeow:vantage`.

## GTS wire coordinates

A finding's location node carries the **wire coordinates** that pin its exact position inside a GTS
bundle — the same coordinates emitted as SARIF `logicalLocations` and recorded on the Rust
`Location`. They are datatype properties with an **open domain** (they decorate whatever node
`gmeow:findingLocation` points at) and a `xsd:nonNegativeInteger` range.

### gmeow:gtsTermId · gmeow:gtsQuadIndex · gmeow:gtsReifierId · gmeow:gtsFrameIndex · gmeow:gtsSegmentIndex

The five wire coordinates: the term-id, quad index, reifier-id, frame index, and segment index that
resolve a finding into the bundle's term/quad/reifier/frame/segment tables. Each mirrors a SARIF
logical-location kind (`gts:term`, `gts:quad`, `gts:reifier`, `gts:frame`, `gts:segment`) and the
corresponding Rust `Location.gts_*` field, so SARIF, this RDF projection, and the content-addressed
validation cache all anchor a diagnostic to the same position.

## SSSOM alignments (`mappings/equivalences.ttl`)

Authored once and compiled to `mappings/gmeow-diagnostics.sssom.tsv` by `gmeow compile-mappings`
(Principle 4); all by reference (Principle 5). The match to W3C **EARL** (Evaluation And Report
Language) is deliberately loose (`skos:closeMatch`, not `equivalentClass`).

| GMEOW | Predicate | Target | Note |
|---|---|---|---|
| `gmeow:Finding` | `skos:closeMatch` | `earl:Assertion` | EARL's `(assertedBy, subject, result)` tuple vs a reified `gmeow:Observation` that is itself a projection of the canonical report |
| `gmeow:findingSeverity` | `skos:closeMatch` | `earl:outcome` | EARL's `outcome` grades the **test** (passed/failed/cantTell); `gmeow:findingSeverity` grades the **finding** a test produced |

The internal `logic:violation` (the OntoUML-discipline diagnostic) and the native↔oracle
divergence-ledger entries are **restricted `gmeow:Finding`s** in the GMEOW namespace; because this
file aligns to external vocabularies only, that unification is documented here rather than mapped.

## Dependencies

| Slice | Why |
|---|---|
| `kernel` | `gmeow:SoftwareAgent` (the tool vantage) and the graph-box-role / box vocabulary |
| `observations` | the Observation spine a `gmeow:Finding` specializes — `gmeow:vantage`, `gmeow:observedFeature`, `gmeow:observationResult`, which `gmeow:findingLocation` / `gmeow:findingSeverity` refine |

## Verified by construction

`tests/test_diagnostics.py` pins the load-bearing shape of the slice:

- **Finding ⊑ Observation** — `gmeow:Finding` is an `owl:Class` (`gufo:Kind`) with a real
  `rdfs:subClassOf gmeow:Observation` bridge.
- **Role subproperties** — `gmeow:findingSeverity ⊑ gmeow:observationResult` (range
  `gmeow:DiagnosticSeverity`) and `gmeow:findingLocation ⊑ gmeow:observedFeature` (open range).
- **Severity value vocabulary** — `gmeow:DiagnosticSeverity` is a `gufo:QualityValue`; its four grades
  are individuals, never subclasses.
- **Wire coordinates** — the five `gmeow:gts*` properties are datatype properties ranging over
  `xsd:nonNegativeInteger`.
- **No truth/resolution bits** — none of `isTrue` / `isFalse` / `isResolved` / `findingOutcome`
  appears; the slice is a projection, not a verdict mint.
- **Annotation completeness** (Principle 8) — all 16 locally-declared terms carry an `rdfs:label`, a
  `skos:definition`, `rdfs:isDefinedBy` the diagnostics slice IRI, and a `gmeow:graphBoxRole`.
