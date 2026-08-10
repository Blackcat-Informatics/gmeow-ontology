<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# norms

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/norms` · **tier: core**

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

`Condition` (core-owned by the observations slice, Principle 16 — reused by reference here) is
prose-canonical (`conditionText` mandatory). Composition via
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
**do-not-touch list**: without the graft, core rights is
byte-identical and behaves exactly as before.

## Deferred to the compiler-arc window

Alignments (ODRL 2.2 via EDOAL — `odrl:Constraint ↔ Condition` is
structural; LegalRuleML `<Override>` — the only target where precedence
survives round-trip; LKIF-Core, DPV, SUMO normative attributes, UFO-L) and
projections (ODRL JSON-LD, OPA/Rego, Cedar, XACML, LegalRuleML XML, each
with declared-loss manifests — enforcement flattens the issuer index, said
loudly). Target list fixed in the norms slice; the `wip-aboutness-349` mapping-set
precedent applies.

## The rubrics facility

`Rubric`, `Criterion`, `Assessment`, `Condition`, and `EvaluationVerdict` are the
foundational evaluative-primitive vocabulary — genuinely needed by non-norms
consumers (e.g. the preference slice), so they are **core-owned by the
observations slice** (Principle 16) and reused by reference here. The one
bridge axiom this module still authors directly is `Rubric ⊑ Norm`: a rubric
**is** a norm for judging, so `normIssuer` (no anonymous evaluation
standards), `overrides`, `AuthorityLevel`, and `PrecedenceTenure` arrive free.
That bridge lives in this slice because one slice carries the deontic family —
a single canonical owner (Principle 4) — and the evaluative primitives live in
observations rather than here so that consumers which need them but not the
deontic apparatus can reach them without taking a norms edge.

- **Content reified, application solver-layer** (P12): `Criterion` (core) with
  *named* poles (`CriterionPole` — "Power from the Bottom" vs "Passive
  Victimhood" — stays norms-resident since it is domain-specific rubric
  vocabulary), `ScoreScale` (min/max/step; arithmetic is solver work),
  `ScoreAnchor` (range × meaning × exemplars; interpolation is solver work).
- **`Exemplar ⊑ CitationAct`** — CiTO alignment free; pins by Selector span
  AND/OR `exemplarSubject` (the entity-pattern case: a character's conduct
  across a whole work — design context's 823 corpus links; at least one pin,
  SHACL). Closed polarity trichotomy (positive / negative / cautionary) with
  `exemplarRedirect` for the anti-pattern's correct-criterion correction.
  The kernel aboutness axis carries the **phase-gate**: a span that
  *describes* trust is source material; one that *enacts* it is embodiment —
  same selector, different `hasAboutness`, different anchored range.
- **`Assessment ⊑ Observation`** (core) — the judge is a vantage; an LLM judge is
  just a vantage, and two models disagreeing are two coexisting cells, no
  winner (P9). `assessmentCriterion`/`assessmentRubric` (also core) play the
  `observationMethod` role **without** the subproperty axiom (functional
  QualityValue range vs Entity values — the `claimModality` pattern);
  `assessmentScoreValue` (also core) is the datatype twin of `observationResult`.
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
- **The voice payload is byte-perfect**: `StyleGuide` + `voiceExemplifiedBy` →
  content-digested `Document`s carrying `hasAboutness aboutnessEnacts` —
  the document does not describe the voice, it *is* the voice; an
  undigested exemplar is a SHACL violation (silent drift).
- **Deferred to the compiler-arc window**: system-prompt assembly
  (Persona × Norms × StyleGuide — the projection that replaces
  principia.yaml's Jinja2 role), AI character-card JSON, LexInfo/OLiA/DUL
  alignment rows. Target list fixed in the alignment ledger.

## Terms

### gmeow:Norm · gmeow:NormativeSystem

`Norm` is a `gufo:Category` at Entity level — a prescription existing by social
convention that records an issuer prescribes some conduct and asserts nothing about
the world. `NormativeSystem` is a body of norms with a shared identity (constitution,
legal code, code of conduct, principia, rubric set); member norms attach via `partOf`,
and competing systems coexist without adjudication.

### gmeow:DeonticModality · gmeow:deonticModality

The open deontic-force vocabulary — obligation, prohibition, permission, recommendation
(individuals, never subclasses), seeded so supererogation and exemption can join without
schema change. `deonticModality` is the functional property fixing a norm's single force;
a modality-bearing norm must also carry an issuer.

### gmeow:normIssuer · gmeow:systemIssuer · gmeow:normBearer · gmeow:prescribedConduct

`normIssuer` names the agent or standpoint according to which a norm holds — the keystone
turning an ought into an ought-according-to; domain-free, non-functional (co-issued norms
carry several), documented as `⊑ accordingTo`. `systemIssuer ⊑ normIssuer` issues a whole
`NormativeSystem`. `normBearer` names a bound agent (absent = everyone in the issuer's
scope). `prescribedConduct` points at the governed conduct — an event type, a `Goal`
(core teleology), a situation, or a rights action — with an intentionally open range.

### gmeow:AuthorityLevel · gmeow:strongerThan · gmeow:hasAuthorityLevel

The ordered authority-grade vocabulary (absolute ≻ high ≻ medium ≻ conditional), the
kernel `GranularityLevel` pattern. `strongerThan` is transitive on the levels only;
`hasAuthorityLevel` records the grade an issuing system claims for a norm (non-functional —
sources may grade differently and coexist). What the order does to conflicts is solver work.

### gmeow:overrides · gmeow:PrecedenceTenure · gmeow:precedenceHigher · gmeow:precedenceLower · gmeow:precedenceScope

`overrides` is pairwise defeasible precedence — deliberately not transitive, SHACL-irreflexive;
chains and cycles are solver work over the recorded pairs. `PrecedenceTenure` is the reified,
time-scoped form ("Tier 2 overrode X until v3.5") carrying its `precedenceHigher` /
`precedenceLower` norms (functional, distinct) and a mandatory `precedenceScope` (a
`NormativeSystem` — precedence is always scoped), withdrawn by suppression only.

### gmeow:Condition (core, observations) · gmeow:conditionText · gmeow:normCondition

`Condition` — core-owned by observations, reused by reference here (Principle 16) — is a
describable circumstance whose canonical form is prose — the trigger of a
conditional norm, a persona's activation context, a causal antecedent. `conditionText` is the
mandatory natural-language statement that formalizations approximate. `normCondition` names the
condition(s) under which a norm applies (several are an implicit conjunction).

### gmeow:ConditionGroup · gmeow:GroupOperator · gmeow:groupOperator · gmeow:groupMember

A composite condition combining members with explicit logic. `GroupOperator` is the closed
trichotomy — all (and), any (or), none (not); richer trees nest groups, never add operators.
`groupOperator` is the functional, mandatory operator; `groupMember` attaches the members
(at least two by SHACL). A group is itself a `Condition`.

### gmeow:ConditionExpression · gmeow:expressionText · gmeow:ExpressionLanguage · gmeow:expressionLanguage · gmeow:formalizedAs

A `ConditionExpression` is a machine formalization in a named language — stored, never executed
(Principle 12): no CI step, test, or tool evaluates it against the graph. `expressionText` carries
the verbatim source; `ExpressionLanguage` is the open language vocabulary (prose, SPARQL ASK, CEL,
Rego, Cedar, XACML, SHACL) declared via `expressionLanguage`. `formalizedAs` attaches a formalization
as a challengeable claim of equivalence to the prose.

### gmeow:ConditionParameter · gmeow:conditionParameter · gmeow:parameterName · gmeow:parameterValue · gmeow:parameterEntity

A named binding that instantiates a condition template so one `Condition` serves many norms.
`conditionParameter` attaches bindings; `parameterName` is the mandatory key; a parameter carries
exactly one of `parameterValue` (literal) or `parameterEntity` (IRI) by SHACL.

### gmeow:ConditionEvaluation · gmeow:evaluatedCondition · gmeow:EvaluationVerdict (core, observations) · gmeow:evaluationVerdict

Whether a condition held is a `ConditionEvaluation ⊑ Observation`, vantage = the evaluator — never
a graph entailment; two evaluators disagreeing are two coexisting cells. `evaluatedCondition`
(`⊑ observedFeature`) names the reported condition. `EvaluationVerdict` — core-owned by
observations, reused by reference here (Principle 16) — is the closed trichotomy
held / not-held / undetermined, carried by the functional `evaluationVerdict`.

### gmeow:violates · gmeow:complies · gmeow:ComplianceAssessment · gmeow:assessedEvent · gmeow:assessedNorm · gmeow:complianceVerdict

`violates` / `complies` are the flat shortcuts (Event → Norm), indexed to whoever asserts them and
never entailed. Promote to `ComplianceAssessment ⊑ Observation` when the assessor, evidence, or
confidence must be first-class: `assessedEvent` (`⊑ observedFeature`) and `assessedNorm` name the
pair, `complianceVerdict` reuses the verdict vocabulary (held = compliant, not-held = violative).

### gmeow:Rubric (core, observations) · gmeow:Criterion (core, observations) · gmeow:hasCriterion

`Rubric` and `Criterion` are core-owned by observations (Principle 16) and reused by reference
here. This module adds the one bridge axiom `Rubric ⊑ Norm`: a rubric **is** a norm for
judging — a reified evaluation framework that names its issuer and may be overridden; applying it
is solver work that returns vantage-indexed `Assessment` cells. `Criterion` is one evaluative axis
with named poles; `hasCriterion ⊑ hasPart` attaches the axes (rubrics are multi-axis by design).

### gmeow:CriterionPole · gmeow:rewardPole · gmeow:penaltyPole · gmeow:CriterionDomain · gmeow:criterionDomain

`CriterionPole` is a named extreme of a criterion — a small information object with its own label
and definition ("Power from the Bottom"), not a bare number. `rewardPole` and `penaltyPole` are the
functional, mandatory, mutually distinct poles. `CriterionDomain` is the open subject-domain
vocabulary (relational, factual, aesthetic, safety, stylistic) carried by `criterionDomain`.

### gmeow:ScoreScale · gmeow:usesScale · gmeow:scaleMin · gmeow:scaleMax · gmeow:scaleStep

`ScoreScale` is a numeric scale — minimum, maximum, optional step; scale arithmetic is solver work.
`usesScale` attaches it at the rubric level (default) or criterion level (override). `scaleMin` /
`scaleMax` are mandatory bounds (min < max by SHACL); `scaleStep` is the optional discrete step
(absent = continuous).

### gmeow:ScoreAnchor · gmeow:hasScoreAnchor · gmeow:anchorRangeMin · gmeow:anchorRangeMax · gmeow:anchorMeaning · gmeow:anchorExemplar

A `ScoreAnchor` pins a score range to its meaning and exemplars — the rubric's calibration content,
with interpolation left to the solver. `hasScoreAnchor` attaches anchors to a criterion (high / medium /
low coexist). `anchorRangeMin` / `anchorRangeMax` bound the range; the mandatory `anchorMeaning` carries
the calibration prose (one per language tag); `anchorExemplar` pins exemplars to the range.

### gmeow:Exemplar · gmeow:ExemplarPolarity · gmeow:exemplarPolarity · gmeow:exemplarSubject · gmeow:exemplarRedirect · gmeow:exemplarRationale

`Exemplar ⊑ CitationAct` holds something up as an example — pinned by citation Selector AND/OR an
`exemplarSubject` (a character's conduct across a work; at least one by SHACL). `ExemplarPolarity` is
the closed trichotomy positive / negative / cautionary, carried by the functional, mandatory
`exemplarPolarity`; `exemplarRedirect` sends a cautionary case to the criterion it actually evidences;
`exemplarRationale` is the localizable judgement prose.

### gmeow:Assessment · gmeow:assessmentTarget · gmeow:assessmentCriterion · gmeow:assessmentRubric · gmeow:assessmentScoreValue (all core, observations)

`Assessment ⊑ Observation` — core-owned by observations, reused by reference here (Principle 16) —
scores a target against a criterion or whole rubric — vantage = the judge;
an LLM judge is just a vantage and disagreeing models are coexisting cells. `assessmentTarget`
(`⊑ observedFeature`) names what is scored; `assessmentCriterion` / `assessmentRubric` play the
`observationMethod` role without the subproperty axiom (claimModality pattern); `assessmentScoreValue`
is the mandatory numeric twin of `observationResult` — zeros are scores, never absences. The whole
property cluster is core-owned: this slice consumes it on `ComplianceAssessment` and constrains it
(cardinality restrictions on the core `Assessment`), which is the legal extension → core direction.

### gmeow:Persona · gmeow:personaBearer · gmeow:personaRegister · gmeow:activatedIn · gmeow:expressesNorm

`Persona` is a reified expression policy of one agent (a relator, not a `gufo:Role`): PRIMARY and
PRIVATE are two co-equal personas, withdrawn by suppression, never ranked. `personaBearer` is the
functional, mandatory bearer; `personaRegister` draws ≥1 register from the names-core spine;
`activatedIn` names the activation `Condition` or situation; `expressesNorm` makes the same-norms
invariant queryable rather than a shape (divergence is legal).

### gmeow:StyleGuide · gmeow:styleGuideFor · gmeow:voiceExemplifiedBy

`StyleGuide` is the voice payload of a persona or register — prose whose register IS the content.
`styleGuideFor` names what it voices (a `Persona` and/or `Register`). `voiceExemplifiedBy` attaches
byte-perfect content-digested `Document`s that carry `aboutnessEnacts` — the document does not
describe the voice, it is the voice; an undigested exemplar is a SHACL violation.
