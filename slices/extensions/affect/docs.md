<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# affect

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/affect` · **tier: extension**

Emotions and appraisals (the affect design) — **the thinnest slice in the
repo, by commitment**.

## The thinness budget

This slice ships exactly: `Emotion ⊑ gufo:IntrinsicMode` (the
Desire/Intention pattern), an open Plutchik-seeded `EmotionType` vocabulary,
and `Appraisal ⊑ Observation` with the PAD dimensions and an open
`AestheticQuality` vocabulary. **Nothing else.** Principle 15 discipline:
growth requires a *new consumer*, not modeling pleasure. The bar for
expansion: a sensory-environment or narrative consumer demanding (e.g.) an
emotion-tenure class, appraisal provenance structure, or a cognitive
appraisal-theory layer. Until then, requests to grow this slice should be
declined with a pointer here.

## What deliberately does NOT exist

- **No emotion tenure class** — episodic scope rides `validFrom`/`validUntil`
  on the statement; promote to the StandpointTenure idiom only on consumer
  demand.
- **No attributed-vs-self-report machinery** — that's the vantage axis
  (self-report is top authority for the subject's own standpoint, the
  `facetVantage` precedent).
- **No emotion or aesthetic hierarchies** — open value vocabularies,
  contested by design (P9): traditions disagree about the inventory, and
  the disagreement is data.
- **No hard rubrics dependency** — `appraisalValue` is read against a
  rubric `ScoreScale` *when loaded*, plain decimals otherwise (soft
  reference).

## Deferred to the compiler-arc window

MFOEM rows (linkage-only — BFO lineage), EmotionML vocabulary IRIs,
WordNet-Affect closeMatch rows; the W3C EmotionML projection with declared
loss (vantage collapses to EmotionML's single-annotator model — flagged
loudly). Target list fixed in the alignment ledger.
