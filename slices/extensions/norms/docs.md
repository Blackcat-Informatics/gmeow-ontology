<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# norms

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/norms` · **tier: extension**

Generalized deontics with indexed authority (#351) plus the rights graft
(#352) — the constitutional keystone of the normative stack (EPIC #348).

**Grounding note:** `Norm` is a `gufo:Category` at Entity level (the
`IntentionalMoment` precedent), not a Kind under `SocialObject` — the graft
places `Rule ⊑ gufo:Relator` beneath it, plain norms are object-like, and
`gufo:Object ⟂ gufo:Aspect` forbids committing the umbrella to either realm;
a Kind would stack identities under the rights trio (the MixIden gate caught
exactly this during development). The social-convention reading lives on the
concrete classes: `NormativeSystem ⊑ SocialObject`, the trio `⊑ Relator`.

## There is no ought, only ought-according-to

Every modality-bearing `Norm` names its `normIssuer` (SHACL-enforced — an
anonymous ought is the deontic analogue of a global truth axiom). Issuers are
agents or standpoints; `normIssuer ⊑ accordingTo` is **documented, not
axiomatised** (`accordingTo` is an AnnotationProperty — the `vantage`
precedent). Two contradictory normative systems coexist without
inconsistency: GMEOW records what each prescribes and never adjudicates.
This is the deception module's held/projected move, applied to oughts.

## Precedence is data, not semantics

- `AuthorityLevel` — ordered vocab (absolute ≻ high ≻ medium ≻ conditional),
  the kernel `GranularityLevel` pattern; `strongerThan` is transitive **on
  levels only**.
- `overrides` — pairwise, **deliberately not transitive**, SHACL-irreflexive.
  Lex-superior chains, specificity, and cycle handling are solver work over
  recorded pairs (P12).
- `PrecedenceTenure` — the StandpointTenure idiom: higher × lower × scope
  (a `NormativeSystem`, mandatory — precedence is always scoped) ×
  `duringInterval`. "Tier 2 overrode X until v3.5," withdrawable by
  suppression only (P10).

## Conditions: stored, composed, formalized, evaluated — never executed

`Condition` is prose-canonical (`conditionText` mandatory). Composition via
`ConditionGroup` (all/any/none, ≥2 members, nest for trees). Formalizations
(`ConditionExpression` + `expressionLanguage`: prose, SPARQL ASK, CEL, Rego,
Cedar, XACML, SHACL) attach via `formalizedAs`, and **each formalization is
a claim of equivalence to the prose** — challengeable, confidence-bearing.
Parameters (`ConditionParameter`) let one condition template serve many
norms.

**The Principle 12 boundary, operationally:** no CI step, test, or tool may
evaluate a `ConditionExpression` against the graph as part of validation —
including SHACL-language expressions, which are carried as data and never
loaded into the harness. Whether a condition held is a
`ConditionEvaluation ⊑ Observation` (vantage = the evaluator; verdict vocab
held / not-held / undetermined via the `claimModality` axiom pattern). Two
evaluators disagreeing are two coexisting cells.

Compliance follows the same shape: flat `violates`/`complies` shortcuts,
promoted to `ComplianceAssessment ⊑ Observation` when the assessor matters.
Violation is always somebody's judgement; it is never entailed.

## The rights graft (#352) — zero core churn

Asserted in this module, never in core: `Rule ⊑ Norm`,
`ruleAssignee ⊑ normBearer`, `ruleAction ⊑ prescribedConduct`, plus SHACL
modality invariants (a `Permission` carrying a `deonticModality` other than
`deonticPermission` is a violation — likewise Prohibition/Duty; carrying
none is fine, the subkind fixes the effective modality). The trio stays a
**rigid subkind partition** under the open modality axis — not an inferior
duplicate (each subkind has a fixed modality; the axis carries the open
remainder: recommendation, supererogation, …). `prescribedConduct`'s range
is intentionally open (the `tenurePosition` precedent) precisely so the
graft needs no dual-typing of the `RightsAction` vocabulary and the
generated schema surface keeps a clean named domain.

The blast-radius survey's ~150 mechanical sites and 5 class-dependent
conflicts (core disjointness axiom, shape targets, ODRL projection
templates, competency query, grounding tests) are this slice's
**do-not-touch list**: with the extension absent, core rights is
byte-identical and behaves exactly as before.

## Deferred to the compiler-arc window

Alignments (ODRL 2.2 via EDOAL — `odrl:Constraint ↔ Condition` is
structural; LegalRuleML `<Override>` — the only target where precedence
survives round-trip; LKIF-Core, DPV, SUMO normative attributes, UFO-L) and
projections (ODRL JSON-LD, OPA/Rego, Cedar, XACML, LegalRuleML XML, each
with declared-loss manifests — enforcement flattens the issuer index, said
loudly). Target list fixed in #351; the `wip-aboutness-349` mapping-set
precedent applies.
