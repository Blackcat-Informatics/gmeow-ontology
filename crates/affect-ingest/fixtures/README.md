<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Classifier capture fixtures

Each `*-sample.json` is a **genuine captured run** of a real Hugging Face
classifier — the input to the `gmeow-affect-ingest` producer's put leg
(`produce`) and the anchor of the whole-ontology SHACL conformance gate
(`crates/validate/tests/conformance_cases/conformance_affect_producer.rs`). Every fixture is a
`ClassifierRunCapture` envelope over three shared target texts (a grateful, a
joyful, and a fearful sentence), so each exercises the mapped-emotion claim path,
the sentiment/social no-claim path, and the below-threshold "concluded" path.

**These are real inference outputs, not representative numbers.** The scores were
produced by loading each model at the pinned revision below and running it over
the three texts **off-gate** (the repository itself runs no on-gate inference —
determinism / no-network). The capture is deterministic and **byte-reproducible**:
the maintainer artifact
[`maintenance/affect-classifier-capture/capture_fixtures.py`](../../../maintenance/affect-classifier-capture/capture_fixtures.py)
regenerates these exact files (see "Reproducing these fixtures" below). That script
is unattached and unmaintained — wired into no Makefile, gate, or code — kept only
so the fixtures can be refreshed.

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

## Reproducing these fixtures

These fixtures are **frozen, committed inputs** — the authoritative capture of the
three target texts run through each affect classifier. The one-shot Python capture
harness that produced them (a torch/transformers script) was retired with the
repo-wide Python purge; a native re-capture path would have to be reimplemented if
the models or targets ever change. Until then the committed fixtures below are the
source of truth and the `crates/affect-ingest` tests consume them directly.

**The three target texts** (verbatim), run through every model, one classified
target per `(model, text)` at IRI `…/gmeow/examples/affect/<model-dir>/<slug>`:

| slug | text |
|---|---|
| `gratitude` | `Thank you so much, this genuinely made my whole day — I really appreciate it!` |
| `joy` | `I am absolutely thrilled and overjoyed — this is the happiest I have felt in years!` |
| `fear` | `This is terrifying and I am deeply afraid of what happens next.` |

**Pipeline configuration.** Fixed-label models use `transformers`
`text-classification` with `top_k=None` (every label scored). GoEmotions applies
`function_to_apply="sigmoid"` (multi-label); SST-2 / CardiffNLP / j-hartmann apply
`function_to_apply="softmax"` (single-label). Zero-shot uses
`zero-shot-classification` with `multi_label=True` and the hypothesis template
`This text expresses {}.` over the candidate set
`["joy", "anger", "fear", "sadness", "surprise", "disgust"]` (prompt order
preserved; `label_set_revision` is `candidates:` + the sorted set). Every fixture
pins `tokenizer_revision = model_revision`.

**External-label → registry-local maps** (the lossless registry-identity step —
the fixture `label` is always the registry local, and the raw model surface string
is preserved as that label's `rdfs:label` in `slices/core/affect/module.ttl`):

| adapter | raw model label → registry local |
|---|---|
| GoEmotions | identity — the 28 GoEmotions labels are their own registry locals (lowercase) |
| SST-2 | `POSITIVE`→`sst2Positive`, `NEGATIVE`→`sst2Negative` |
| CardiffNLP | `negative`→`cardiffNegative`, `neutral`→`cardiffNeutral`, `positive`→`cardiffPositive` |
| j-hartmann | `anger`→`ekmanAnger`, `disgust`→`ekmanDisgust`, `fear`→`ekmanFear`, `joy`→`ekmanJoy`, `neutral`→`ekmanNeutral`, `sadness`→`ekmanSadness`, `surprise`→`ekmanSurprise` |
| zero-shot | identity — the candidate surfaces are run-scoped and minted in-graph |

Within each target the `scores` are sorted by label. To refresh after a model is
re-pinned, update `REV` in the artifact **and** the pinned revision in the table
above, then re-run.
