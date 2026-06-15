<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# norms

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/norms` · **tier: extension**

Generalized deontics with indexed authority plus the rights graft
(rights graft) — the constitutional keystone of the normative stack.

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

## The rights graft — zero core churn

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
loudly). Target list fixed in norms extension; the `wip-aboutness-349` mapping-set
precedent applies.

## The rubrics facility

A rubric **is** a norm for judging: `Rubric ⊑ Norm`, so `normIssuer` (no
anonymous evaluation standards), `overrides`, `AuthorityLevel`, and
`PrecedenceTenure` arrive free. It lives in this slice because the P16 DAG
rule bars extension→extension dependencies — one slice carries the deontic
family.

- **Content reified, application solver-layer** (P12): `Criterion` with
  *named* poles (`CriterionPole` — "Power from the Bottom" vs "Passive
  Victimhood"), `ScoreScale` (min/max/step; arithmetic is solver work),
  `ScoreAnchor` (range × meaning × exemplars; interpolation is solver work).
- **`Exemplar ⊑ CitationAct`** — CiTO alignment free; pins by Selector span
  AND/OR `exemplarSubject` (the entity-pattern case: a character's conduct
  across a whole work — design context's 823 corpus links; at least one pin,
  SHACL). Closed polarity trichotomy (positive / negative / cautionary) with
  `exemplarRedirect` for the anti-pattern's correct-criterion correction.
  The kernel aboutness axis carries the **phase-gate**: a span that
  *describes* trust is source material; one that *enacts* it is embodiment —
  same selector, different `hasAboutness`, different anchored range.
- **`Assessment ⊑ Observation`** — the judge is a vantage; an LLM judge is
  just a vantage, and two models disagreeing are two coexisting cells, no
  winner (P9). `assessmentCriterion`/`assessmentRubric` play the
  `observationMethod` role **without** the subproperty axiom (functional
  QualityValue range vs Entity values — the `claimModality` pattern);
  `assessmentScoreValue` is the datatype twin of `observationResult`.
  Zeros are scores, never absences. Scoring-density guidance by target
  granularity reuses kernel `hasGranularity`.
- **Deferred to the compiler-arc window**: EARL (EDOAL — Assertion ↔
  Assessment is structural), DQV, schema.org `Rating`/`Review`, 1EdTech
  CASE, and the lm-eval task-YAML projection; judge outputs ingest back as
  vantage-indexed Assessments. Target list fixed in the alignment ledger.

## The registers & personas facility

Same agent, same norms, different expression by context — and
register-switching is **not** deception (the held/projected divergence is
the design's territory; documented boundary, no axiom coupling).

- **Grounding decision (deviation from the register/persona sketch, recorded):**
  `Persona` is a **relator** (the NameUsage idiom), not a `gufo:Role` class —
  roles classify, they don't reify, and a persona needs its own identity for
  registers, style guides, activation conditions, and suppressible tenure.
  The DUL Role/Description/Situation grounding maps onto relator + Condition.
- **The register spine lives in names core**: `gmeow:Register` (open
  umbrella) with `NameRegister ⊑ Register` — address and expression draw
  from one vocabulary, and the dependency direction requires the umbrella
  below its consumers. This facility mints the persona-facing seeds
  (public, private, ceremonial, clinical, brand voice).
- **Expression machinery**: `personaBearer` (one agent, many co-equal
  personas — no `primaryPersona`, ever), `personaRegister` (≥1, open vocab),
  `activatedIn` (→ `Condition` or situation type; blend-vs-compete is solver
  work over recorded precedence), `expressesNorm` (→ `Norm`). Persona
  precedence needs **no new machinery**: `overrides`/`PrecedenceTenure` on
  the activation norms.
- **The same-norms invariant is a query, not a shape**
  (`registers-norm-divergence.rq`): divergence is legal (P9) — the query
  makes it visible. Test-proven in both directions.
- **The voice payload is byte-perfect**: `StyleGuide` + `exemplifiedBy` →
  content-digested `Document`s carrying `hasAboutness aboutnessEnacts` —
  the document does not describe the voice, it *is* the voice; an
  undigested exemplar is a SHACL violation (silent drift).
- **Deferred to the compiler-arc window**: system-prompt assembly
  (Persona × Norms × StyleGuide — the projection that replaces
  principia.yaml's Jinja2 role), AI character-card JSON, LexInfo/OLiA/DUL
  alignment rows. Target list fixed in the alignment ledger.
