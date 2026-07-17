<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Diagnostics

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/diagnostics` · **tier: core**

The ontological face of GMEOW's first-class diagnostics system. A validation violation, a
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

## Category is an orthogonal axis

Severity answers "how loud?"; the `logic:FindingCategory` axis (owned by the logic grounding slice)
answers "what kind?" — because not all findings are failures. This slice wires each category's two
projections: `gmeow:categoryBlocking` (its gating contribution — `gmeow:blockingBlocking` for the
three failure kinds, `gmeow:blockingCoherent` for the rest) and `gmeow:categoryPolarity` (its Belnap
coherence stance, a `logic:InformationState` value). The closed **chatter** category
`logic:FindingTransientChatter` is the home for general-purpose logging — the ordinary note/info
stream a run emits to narrate its own progress. It projects to `gmeow:blockingCoherent` and
`logic:InfoNeither`: transient bookkeeping that never gates and takes no coherence stance.

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
`Location`. The four diagnostics-owned coordinates are datatype properties with an **open domain**
(they decorate whatever node `gmeow:findingLocation` points at) and a `xsd:nonNegativeInteger`
range. The fifth, `gmeow:gtsSegmentIndex`, is **owned by the gts slice**
(single-owner invariant) and merely *referenced* here as a coordinate.

### gmeow:gtsTermId · gmeow:gtsQuadIndex · gmeow:gtsReifierId · gmeow:gtsFrameIndex · gmeow:gtsSegmentIndex

The five wire coordinates: the term-id, quad index, reifier-id, frame index, and segment index that
resolve a finding into the bundle's term/quad/reifier/frame/segment tables. Four are declared in
this slice; `gmeow:gtsSegmentIndex` is the gts-owned segment position (the index that, over the
segment heads, IS a document's composite identity — spec §3.1), referenced here so a finding can
name the segment it concerns. Each mirrors a SARIF logical-location kind (`gts:term`, `gts:quad`,
`gts:reifier`, `gts:frame`, `gts:segment`) and the corresponding Rust `Location.gts_*` field, so
SARIF, this RDF projection, and the content-addressed validation cache all anchor a diagnostic to
the same position.

## Dev-gate producers (the feedback fold)

`gmeow-dev feedback` folds **every** GMEOW-owned `make check` surface into one canonical report
(a few surfaces are folded standalone rather than as literal `make check` targets — `box-roles` is
a `reason-native` sub-audit),
then writes `dist/gmeow-feedback.{json,sarif,html,gts}` — so the self-attesting bundle is the
complete picture of the developer gate, not just validation (Principle 5: maximal information
flow). Each surface owns its own `Severity`/`code` semantics via a `to_diagnostics_report()` function
(the facade stays surface-agnostic); the table-driven fold in `cli_dev._surface_reports()` adds a
surface in one row, and `tests/test_feedback_surfaces.py` pins the fold table against the documented
surface set so the registry of folded surfaces cannot drift unnoticed. The feedback process **exit
stays driven solely by the validation result** — per-surface hard gating lives in each surface's own
`make check` command; the bundle carries the rest as an artifact.

| Surface | Stable `code`s | Severity rule |
|---|---|---|
| `validate` (SHACL/syntax/lint) | `shacl.*`, `validate.*` | Rust-native; carries GTS wire coords |
| native `reason` / `verify` | `reason.*`, `verify.*` | folded as `gmeow_diagnostics::Report`s |
| `alignment` | `alignment.<check>` | maps `AlignmentFinding.severity` (keeps `info`, unlike the legacy gate) |
| `coverage` | `coverage.gap-class`, `coverage.gap-predicate` | `info` — gaps never fail the gate |
| `acceptance` | `acceptance.<gate-name>` | failing **hard** gate → `error`; failing **scoreboard** gate → `note` |
| `wikidata` | `wikidata.qid-syntax`, `wikidata.namespace-misuse` | `error` (misuse kind in `tags`) |
| `constitution` | `constitution.error`, `constitution.warning` | folds the gate's error/warning strings |
| `box-roles` | `box-roles.missing`, `box-roles.invalid` | `error` (term source in `path`) |
| `audit` | `audit.{ungrounded,contradicted,stale}-*`, `audit.shacl-{error,warning}` | heuristic flags → `warning`; SHACL errors → `error` |
| `generator` | `generator.{drift,orphan,problem}` | `error`; per-finding `tool` = generator name (covers statement + mapping drift) |

The mapping-compiler surface is now **landed**: native compiler failures surface as
`mapping-compile.dsl-error`, and the SSSOM validator + native `gmeow_slice.lint_projection` trio →
`mapping-compile.{sssom,fno-type,fno-ref,spec-drift}`; only the `overclaim`
leg remains.

The remaining GMEOW-owned `make check` surfaces (not silently dropped): the mapping-compiler `overclaim`
leg, statement-compiler (round-trip), and logic-compiler (`gmeow_logic` diagnostic
dicts) surfaces; external-tool failures (`ruff` / `mypy` / `clippy` / `pre-commit`) wrapped with raw
output; and granular per-check `constitution.*` codes (today the string-only constitution surface
uses `constitution.error` / `constitution.warning`).

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

- **Finding ⊑ Observation** — `gmeow:Finding` is an `owl:Class` (`gufo:SubKind`) with a real
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
