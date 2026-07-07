<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Classifier capture fixtures

Each `*-sample.json` is a **genuine captured run** of a real Hugging Face
classifier — the input to the `gmeow-affect-ingest` producer's put leg
(`produce`) and the anchor of the whole-ontology SHACL conformance gate
(`crates/validate/tests/conformance_affect_producer.rs`). Every fixture is a
`ClassifierRunCapture` envelope over three shared target texts (a grateful, a
joyful, and a fearful sentence), so each exercises the mapped-emotion claim path,
the sentiment/social no-claim path, and the below-threshold "concluded" path.

**These are real inference outputs, not representative numbers.** The scores were
produced by loading each model at the pinned revision below and running it over
the three texts **off-gate** (the repository itself runs no on-gate inference —
determinism / no-network). The one-time capture procedure is
`gmeow affect ingest`'s upstream: an authenticated `transformers`
`text-classification` (or `zero-shot-classification`) pipeline with
`top_k=None` / `return_all_scores`, pinned to the model's commit; the resulting
`scores` array is checked in verbatim. To refresh a fixture, re-run the pinned
model and replace its `scores` (the `model_revision` must match the run).

| fixture | model | pinned revision | semantics |
|---|---|---|---|
| `goemotions-sample.json` | `SamLowe/roberta-base-go_emotions` | `d75048347613a25d77de8cf6412eaae9fa7b26be` | sigmoid, multi-label (28) |
| `sst2-sample.json` | `distilbert-base-uncased-finetuned-sst-2-english` | `714eb0fa89d2f80546fda750413ed43d93601a13` | softmax (POSITIVE/NEGATIVE) |
| `cardiff-sample.json` | `cardiffnlp/twitter-roberta-base-sentiment-latest` | `3216a57f2a0d9c45a2e6c20157c20c49fb4bf9c7` | softmax (Negative/Neutral/Positive) |
| `ekman7-sample.json` | `j-hartmann/emotion-english-distilroberta-base` | `0e1cd914e3d46199ed785853e12b57304e04178b` | softmax, Ekman-7 |
| `zeroshot-sample.json` | `facebook/bart-large-mnli` | `d7645e127eaf1aefc7862fd59a17a5aa8558b8ce` | NLI entailment, run-scoped candidates |

## What every fixture proves

- **Lossless ingest** — every emitted `(target, label)` becomes a
  `gmeow:AffectClassifierOutput` carrying the raw score + score semantics +
  applied threshold; the blind `recover ∘ produce = id` round-trip proves it.
- **The external-label → registry-identity step** — each fixture's `label`
  strings are the GMEOW registry locals (e.g. the model's `POSITIVE` is captured
  as `sst2Positive`, registered under `gmeow-registry/hf/`); the raw model
  surface string is preserved as that registered label's `rdfs:label`.
- **Claim routing (evidence, never entailment)** — an above-threshold label
  supports a `gmeow:AffectiveClaim` **only** where the ontology authors a reviewed
  `skos:closeMatch` to a `gmeow:EmotionType`. GoEmotions and Ekman-7 emotion
  labels route claims; SST-2 / CardiffNLP sentiment labels (a `relatedMatch` to
  the valence axis, never an emotion) and every `neutral` label emit an
  output+score but **no** expresses-claim.

## The zero-shot fixture

`facebook/bart-large-mnli` is NLI-entailment, not a fixed-label classifier — its
candidate set is prompt-supplied and is part of the run's identity (per the
canonical affect design). The fixture therefore carries the run-scoped
`candidate_labels`, the `hypothesis_template`, and a `label_set_revision`
pinning the candidate set, rather than pointing at a static
`gmeow:AffectLabelSet`. Its scores are per-candidate entailment probabilities
(`multi_label` sigmoid over entailment-vs-contradiction). It emits attributed
evidence (one output per candidate) but routes no auto-claim: a run-scoped prompt
candidate has no pre-reviewed closeMatch, so the claim/evidence boundary holds.
