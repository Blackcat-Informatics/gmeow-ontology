<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Typed Compositional Meta-Semantics (the #766 capstone)

> The capstone of META-EPIC #766. It records the one move that organizes the
> reasoning layer — apply GMEOW's own projection doctrine *inside* the reasoning
> framework — and catalogs the orthogonal canonical axes it introduced against the
> simplified surfaces generated from them. It is a member of the GMEOW Logic design
> set (see [`LOGIC.md`](LOGIC.md)); each axis below is specified in full by the
> design-set document named in its row.

## The thesis #766 settled

The risk to a growing reasoning layer is **not** insufficient expressivity. It is that
independently sophisticated features — stable-model, probabilistic, transactional,
paraconsistent, counterfactual, world-indexed — acquire *incompatible meanings when
combined*, because they are mostly **orthogonal semantic dimensions, not competing
settings on one axis**. Collapsing them onto a single knob (a `semantic_profile`
field, a five-value modality, a four-rung ladder) forces a false choice and hides the
combinations that have no defined meaning.

The move is the projection doctrine turned inward: make the canonical reasoning
representation **orthogonal and explicit**, and **generate** the convenient simplified
surface from it (Principle 17 / Principle 4). The same discipline the ontology applies
to OWL/SHACL/Datalog as lossy projections of `logic:`, applied to profiles, claims,
worlds, results, goals, cognition, and parthood.

The unifying invariant: **canonical axes are authored directly; the simplified/legacy
surfaces are generated and regenerate losslessly; and any combination the engine has
no defined semantics for surfaces as an explicit `unsupported`/disclosure — never a
silent approximation to a nearby semantics.**

## The orthogonal axes ↔ generated surfaces

Each row names a canonical orthogonal representation, the simplified surface generated
from it, the child issue that landed it, and the design-set document that specifies it.

| Axis (canonical form) | Generated lossy surface | Child | Specified in |
|---|---|---|---|
| `logic:ReasoningContract` — 11 composable facets selected independently | the six named presets (`logic:ReasoningPreset`), expanded via `logic:expandsToFacet` | ME1 #767 | [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md) |
| typed `logic:ReasoningResult` — input × evaluation × completeness × preservation × information (Belnap four-valued + two non-results) | a single coarse status / boolean answer | ME2 #768 | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| claim modality factored into six axes (polarity, modal force, credence, assertoric force, truth-directedness, support status) | the five-value `StandpointModality` (`unequivocal`/`probable`/`conceivable`/`refuted`/`bullshit`), each carrying its `gmeow:decomposesToAxis` bundle | ME3 #769 | [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md) |
| separated `Proposition` / `ClaimToken` / `DoxasticState` / `ClaimEvaluation` | the `gmeow:StandpointClaim` (a `gmeow:Observation` relator) as the projected union view | ME4 #770 | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| typed context algebra — `PossibleWorld`/`EpistemicContext`/`Standpoint`/`Scenario`/`State`/`History-Path`/`ReferenceFrame`/`NarrativeFrame` + typed accessibility | unindexed → `UnspecifiedStandpoint` (the retired unindexed→universal default) | ME5 #771 | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| typed formalization governance — `FormalizationCandidate` lifecycle, per-category coverage, executable `NonEntailmentObligation` | a global coverage percentage | ME6 #772 | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |
| full-FOL typed IR — object/meta formulas, constraints, rules, queries, transaction programs, action schemas, validation shapes; explicit equality/UNA/Skolem/stratification | each lowering's preservation judgment (exact / under / over / validation-only / unsupported) | ME7 #773 | [`LOGIC-IR.md`](LOGIC-IR.md) |
| argumentation layer + epistemic standards — `Argument`/attack-kinds + `knowsThatIn` local factivity, reconciled defeater vocabulary | the flat `defeatedBy` / `hasDefeater` surfaces | ME8 #774 | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| typed/contextual mereology — overlap/disjointness, `MereologyProfile`-scoped supplementation, contextual `HolonicPosition`, structured `EmergenceAssessment` | the unary `Holon` and generic `properPartOf` | ME9 #775 | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| structured `GoalExpression` (atomic/conj/disj/achievement/maintenance/avoidance/optimization/conditional/deadline) + reified `GoalEvaluation` + action theory | the binary `satisfiedBy` | ME10 #776 | [`LOGIC-TELEOLOGY.md`](LOGIC-TELEOLOGY.md) |
| transaction-path execution consuming the action theory; State/History/Path vs Intention/Commitment vs causal account kept linked but never identified | the DAG profile of the canonical process model | ME11 #777 | [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md) |
| reified multidimensional `CognitiveAssessment` (eight dimensions) | the four-rung `isAwareOf … hasMastered` ladder; `hasSkill→knowsAbout` as defeasible evidence | ME12 #778 | [`LOGIC-COGNITION.md`](LOGIC-COGNITION.md) |
| semantics-aware validator — eight-way finding taxonomy incl. *permitted* epistemic conflict; scoped, signed coherence certificate | "self-consistent under its own axioms" as a single bit | ME13 #779 | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |

## Why this is the close-out witness

Every child of #766 is landed, and each axis above is authored canonically with its
surface generated, not substituted. The doctrine has a CI-checkable edge precisely
where the meta-epic's risk lives — the `ReasoningContract` compatibility matrix: a new
facet value cannot quietly acquire an undefined combination, because an exhaustive
oracle sweep checks the whole facet cross-product against an independent statement of
the forbidden combinations, a completeness guard forces coverage to track the rule
table, and the verdict is a hard `unsupported`, never a silent approximation (see
[`LOGIC-CONTRACT.md` § Feature-model completeness & coverage](LOGIC-CONTRACT.md)). That
is the meta-semantics thesis made enforceable rather than merely asserted.
