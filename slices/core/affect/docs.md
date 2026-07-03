<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# affect

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/affect` · **tier: core**

Emotions and appraisals — a **core** slice. An agent's felt mental life is part of
the grounded-agent-memory flagship (Principle 14), so affect joins the kernel
`gmeow:MentalMoment` family alongside cognition, epistemics, and teleology.

## The model

Core is comprehensive by design. This slice is the kernel of the affect model;
the fuller high-dimensional landscape, the affective-experience and evidence
layers, and the external bridges are specified canonically in
[`design/AFFECT-DESIGN.md`](./design/AFFECT-DESIGN.md). The current vocabulary
is a kernel, not a ceiling: `gmeow:Emotion` (an intrinsic affective
mode inhering in one agent, grafted under `gmeow:AffectiveMoment ⊑ gmeow:MentalMoment`),
an open Plutchik-seeded `EmotionType`, `Appraisal ⊑ Observation` with the PAD
dimensions and an open `AestheticQuality` vocabulary, and the emotion's
`affectiveTarget` (aboutness) separated from its `affectiveElicitor` (cause).

## Scope of the current module

The kernel models emotions and appraisals; it does not yet carry a distinct
affective-experience or mood/tenure class, a dimensional landscape, or an
evidence spine — see `design/AFFECT-DESIGN.md` for the comprehensive design of
those:

- **Episodic scope** rides `validFrom`/`validUntil` on the statement; there is
  no separate felt-episode or mood-tenure surface in the kernel.
- **`appraisalValue`** is a plain decimal; it does not yet reference an
  `AffectScaleProfile` for the open two-family axis basis, scale profiles,
  vector observations, and composition that the fuller model specifies.
- **Evidence** (expression, classifier outputs, telemetry as attributed
  evidence) is not yet modelled as a distinct spine.

Permanent stances (true regardless of how the model grows): **no emotion or
aesthetic hierarchies** — open value vocabularies, contested by design (P9);
and **attributed-vs-self-report is the vantage axis** (self-report is top
authority for the subject's own standpoint, the `facetVantage` precedent), not
new machinery.

## Alignments

MFOEM rows (linkage-only — BFO lineage), EmotionML vocabulary IRIs,
WordNet-Affect closeMatch rows; the W3C EmotionML projection with declared
loss (vantage collapses to EmotionML's single-annotator model — flagged
loudly). Target list fixed in the alignment ledger.

## Terms

### gmeow:Emotion · gmeow:emotionBearer

An emotion is an intrinsic mode inhering in one agent (the Desire/Intention
grounding) — episodic scope rides `validFrom`/`validUntil` on the statement, no
tenure class. `emotionBearer` is functional and mandatory: an intrinsic mode has
exactly one bearer.

### gmeow:EmotionType · gmeow:emotionType

The kind of an emotion as an OPEN vocabulary seeded with Plutchik's primary eight
(the EmotionML standard set) — registry-independent and contestable, never a tree
(P9). `emotionType` is non-functional: blended emotions carry several types and
classifications from different traditions coexist; at least one (SHACL).

### gmeow:Appraisal · gmeow:appraisalOf

An affective or aesthetic reading of something, as an observation whose vantage is
the appraiser — dimensional or qualitative (at least one of the two forms). Two
critics disagreeing are two coexisting cells. `appraisalOf` (⊑ `observedFeature`)
is functional: one appraisal, one subject.

### gmeow:AppraisalDimension · gmeow:appraisalDimension · gmeow:appraisalValue

The dimensional form: an OPEN axis vocabulary seeded with the PAD triad —
valence, arousal, dominance. `appraisalDimension` reads at most one axis per
appraisal (a PAD triple is three Appraisals sharing a vantage); `appraisalValue`
carries the reading on whatever scale the tradition declares (a rubric's
ScoreScale when loaded, plain decimals otherwise — soft reference).

### gmeow:AestheticQuality · gmeow:appraisalQuality

The qualitative form: an OPEN vocabulary seeded with elegance, sublimity, kitsch —
no hierarchy exists or ever will (P9). `appraisalQuality` is non-functional: one
cell may attribute several coexisting qualities.
