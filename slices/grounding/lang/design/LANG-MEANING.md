<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — Sense, Reference, and Denotation

> The **meaning charter** of the GMEOW Language design set: the Frege discipline, the reified
> denotation record, compositional denotation into `logic:`, interpretation as a vantage-held act,
> and the honest representation of ambiguity, deixis, and speech acts. It makes precise the
> manifesto's ([`LANG.md`](LANG.md)) central rule — *form ≠ sense ≠ reference* — and specifies the
> one-way bridge by which meaning bottoms out in the `logic:` layer. The forms that carry these
> meanings are defined in [`LANG-FORMS.md`](LANG-FORMS.md); the meaning-preserving maps between
> forms are in [`LANG-TRANSLATION.md`](LANG-TRANSLATION.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by canonical OWL/`logic:` authorities in `module.ttl`,
> their generated validation projections, competency queries, and the projection loss ledger —
> not a claim that any implementation already realizes X except as those gates demonstrate.

## Purpose

The meaning layer answers *what a form says*, without ever letting the answer collapse into the
form itself. Three regions factor the layer, each with a single load-bearing commitment.

- **The Frege triangle** — *form, sense, and reference are disjoint kinds.*
- **The denotation record** — *meaning assignment is reified, contexted, and kind-typed.*
- **Interpretation** — *assigning meaning is an act with a vantage; ambiguity is data.*

## The Frege triangle

Core classes: `lang:Sense`, `lang:Denotation`, `lang:DenotationKind`, `lang:Reading`,
`lang:InterpretationAct`, `lang:IndexicalAnchor`, and `lang:CommunicativeAct`.

Core properties: `lang:senseOf`, `lang:denotedForm`, `lang:denotationTarget`,
`lang:denotationKind`, `lang:denotationContext`, `lang:viaSense`, `lang:readingOf`,
`lang:denotation`, `lang:interpretedForm`, `lang:producedReading`, `lang:anchorsIndexical`,
`lang:communicativeForce`, and `lang:aboutReading` (the reading-correctness claim's subject, a
sub-property of `gmeow:observedFeature`).

Two structural markers back the well-formedness gates that make the rules below decidable.
`lang:isIndexical` (a `xsd:boolean` on a denotation) *declares* — rather than infers — that a
denotation's referent varies with a `lang:IndexicalAnchor`, so an indexical denotation whose act
names no anchor is ill-formed. `lang:resolvedReading` names the single reading an interpretation
act selects as canonical; selecting a winner is lawful only when a vantage-held observation
(`lang:aboutReading` with `gmeow:vantage`) grounds it — an act that resolves with no such
observation has silently disambiguated. Both are declared like the analysis level, never guessed.

The triangle's three corners have three different identity criteria, which is why they are three
disjoint kinds and not one node with three labels:

- **Form** (`lang:Form`, from the form charter) — identified by structure in a sign system.
  *The morning star* and *the evening star* are two forms.
- **Sense** (`lang:Sense`) — a lexicalized or constructed *way of meaning*, attached to a lexeme or
  a composed form by `lang:senseOf`. The two forms above have two senses. Senses are what WordNet
  synsets, dictionary sense numbers, and OntoLex `LexicalSense`s approximate, and what translation
  tries to preserve.
- **Reference** — the thing meant, which is *not a `lang:` object at all*: it is whatever GMEOW
  entity, `logic:` object, or described individual the denotation record targets. Venus. Both
  forms, via both senses, reach one referent.

The disjointness is the layer's signature hard rule, the semiotic peer of `math:`'s
probability-is-not-confidence: **string identity never implies form identity; form identity never
implies sense identity; sense identity never implies co-reference — and none of the arrows
reverses.** Every conflation is a typed conformance failure
([`LANG-CONFORMANCE.md`](LANG-CONFORMANCE.md)), because every conflation fabricates information:
treating one spelling as one meaning fabricates disambiguation; treating synonymy as co-reference
fabricates an identity claim.

## The denotation record

A meaning assignment is a **reified record**, never a bare triple from form to target. The record
is the Peircean triad made structural — sign, object, and the interpreting context that connects
them — and reification is what makes disagreement, revision, and ambiguity representable.

```ttl
ex:denot1
    a lang:Denotation ;
    lang:denotedForm ex:sentCatsChaseMice ;          # the form (LANG-FORMS.md worked example)
    lang:denotationKind lang:denotesLogicFormula ;
    lang:denotationTarget ex:formulaCatsChaseMice ;  # a logic: Formula IR object
    lang:viaSense ex:senseChase1 ;                   # chase = pursue, not chase = engrave
    lang:denotationContext ex:ctxPlainAssertion .
```

The mandatory fields, each load-bearing:

- **`lang:denotedForm`** — the form, never a surface form. Meaning attaches above the byte level;
  a denotation hung on a `lang:SurfaceForm` is ill-formed.
- **`lang:denotationKind`** — a `lang:DenotationKind` individual declaring *what kind of thing* is
  meant: `lang:denotesEntity` (a GMEOW individual), `lang:denotesClass`,
  `lang:denotesLogicFormula`, `lang:denotesLogicTerm`, `lang:denotesLogicType` (the `logic:`
  bridge kinds), and `lang:denotesByDescription` (an intensional target held as structure when no
  referent is identified). The kind vocabulary is closed per release and extended by design, not
  ad hoc.
- **`lang:denotationTarget`** — the referent or lowered object, typed consistently with the kind.
- **`lang:denotationContext`** — the context under which the assignment holds: sign-system stage,
  theory, register, discourse. The same form denotes differently in different contexts, and a
  context-free denotation is ill-formed, not conveniently universal.

`lang:viaSense` is required where the denoted form's head lexeme has more than one recorded sense —
routing through the triangle is exactly what prevents the record from silently resolving an
ambiguity it never examined.

### The one-way bridge into `logic:`

Formal meaning bottoms out in `logic:` objects, through the denotation record, in exactly the
architecture the `math:` layer uses for its expression lowering:

- A **declarative sentence** form denotes a `logic:` formula (`lang:denotesLogicFormula`); the
  formula is full-FOL IR, so nothing about natural language forces a decidable fragment.
- A **referring expression** denotes a `logic:` term or a GMEOW entity.
- A **common noun or predicate** denotes a `logic:` type or predicate.
- The lowering is **compositional where analysis reaches**: a composed form's denotation is
  constructed from its constituents' denotations by declared composition rules (the Montagovian
  program, scoped to the fragment GMEOW needs), and each compositional lowering carries a
  preservation judgment like every other lowering in the project. Where analysis does not reach,
  the form's surface stays honest `lang:UnanalyzedProse` — the unliftable remainder is marked,
  never approximated.

The bridge runs **one way**: `lang:` → `logic:`. `logic:` never depends back on `lang:`; its own
prose fields and labels remain ordinary annotations until and unless a `lang:` analysis lifts
them. This is the denotation seam registered in the grounding contract
([`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)).

Nothing here re-implements reasoning. Whether *cats chase mice* entails *some mouse is chased* is
`logic:`'s question about the lowered formulas; `lang:` owns only the audited path from the
sentence to the formula.

## Interpretation — acts, readings, and honest ambiguity

Assigning meaning is an **act**. A `lang:InterpretationAct` is a `gmeow:Activity`: it has an agent
or method (a person, an NLP engine, a compositional rule run), inputs (the form, the context, an
`lang:IndexicalAnchor` where deixis needs resolving), and provenance. Its *results* are
`lang:Reading` objects — candidate denotations — and the *claim* that a reading is correct is a
`gmeow:Observation` held from a vantage. This is the observation spine's
process/result/claim separation, applied to meaning:

- the **act** (`lang:InterpretationAct`) is the process — never typed as an observation;
- the **reading** (`lang:Reading`, carrying a `lang:Denotation`) is the structured result object;
- the **claim** that this reading is the right one is an observation with `gmeow:vantage`,
  confidence, and method — and confidence here is `logic:`'s confidence, *never* a probability
  unless a declared `math:` probability frame is actually present (the discipline inherited
  unchanged from the siblings).

**Ambiguity is co-resident readings.** An ambiguous form holds multiple `lang:Reading`s, each with
its own denotation record and its own vantage-held support. *I saw her duck* keeps both readings —
`duck` as noun via one sense, as verb via another, with different composed-form analyses — as
first-class data. Silent disambiguation is the meaning-layer form of fabricating certainty and
fails the gate; a downstream `logic:` reasoning request over an ambiguous form must select a
reading explicitly, acknowledge the multiplicity in its contract, or report itself not-evaluated.
This is the epistemic-shape-preservation principle: the data's ambiguity *is* its shape, and
imposing a single reading the source does not license is fabrication.

**Deixis is anchored, not resolved away.** Indexical forms — *I*, *here*, *yesterday*, *this
term* — denote only relative to a `lang:IndexicalAnchor` (speaker, time, place, discourse state).
The anchor is a declared object on the interpretation act, so the same form lawfully denotes
differently under different anchors, and an indexical denotation without an anchor is ill-formed.

**Speech acts carry force.** A `lang:CommunicativeAct` records what a use of a form *does* —
assert, ask, order, define, promise — via `lang:communicativeForce` individuals. Force is not
content: *the door is closed* and *is the door closed?* share propositional content and differ in
force, and only the assertion lowers to an asserted `logic:` formula. GMEOW's own competency
questions are communicative acts with interrogative force whose content denotes a query — which is
what makes flagship 2 (GMEOW reading its own prose) more than label-matching.

## Worked example — one form, two readings

```ttl
ex:formSawHerDuck a lang:ComposedForm ; lang:inSignSystem lang:english .   # slots elided; see LANG-FORMS.md

ex:actParse1
    a lang:InterpretationAct , gmeow:Activity ;
    lang:interpretedForm ex:formSawHerDuck ;
    lang:producedReading ex:readingBird , ex:readingDodge .

ex:readingBird
    a lang:Reading ;
    lang:readingOf ex:formSawHerDuck ;
    lang:denotation ex:denotBird .        # duck = waterfowl she owns

ex:readingDodge
    a lang:Reading ;
    lang:readingOf ex:formSawHerDuck ;
    lang:denotation ex:denotDodge .       # duck = the act of ducking

ex:obsPreferDodge
    a gmeow:Observation ;
    gmeow:vantage ex:annotatorVantage ;
    lang:aboutReading ex:readingDodge .   # held preference, with confidence and method — not erasure of ex:readingBird
```

Both readings persist. The annotator's preference is a vantage-held observation *about* a reading,
and a second annotator may hold the other — co-resident, queryable, and never flattened by the
data model itself.

## What this layer refuses to model

Three refusals keep the layer honest and small:

- **No universal semantic-role inventory.** Role and frame inventories (PropBank, FrameNet,
  SemAF) are alignment and projection targets ([`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md)), not
  the canonical meaning representation — meaning bottoms out in `logic:`, which does not need a
  fixed role list.
- **No probability without a frame.** Reading preference weights are confidences unless an actual
  `math:` probability model is declared — the rule is stated once in the siblings and inherited
  verbatim.
- **No world model.** What referents *are* is the business of the domain slices and the `logic:`
  foundation. The meaning layer stops at the audited arrow from form to referent.
