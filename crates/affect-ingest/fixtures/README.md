<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# GoEmotions capture fixtures

## `goemotions-sample.json`

A captured `SamLowe/roberta-base-go_emotions` classifier run — the input to the
`gmeow-affect-ingest` producer's put leg (`produce`) and the anchor of the
whole-ontology SHACL conformance gate
(`crates/validate/tests/conformance_affect_producer.rs`).

**Provenance — what is real:**

- `model_identifier` — the real Hugging Face model repository id.
- `model_revision` / `tokenizer_revision` —
  `d75048347613a25d77de8cf6412eaae9fa7b26be`, the **real pinned commit** of the
  model's `main` at capture time (fetched from
  `https://huggingface.co/api/models/SamLowe/roberta-base-go_emotions`). A model
  name without a pinned revision is a hard fail (rule 7); this SHA is genuine.
- The **28-label set** — the real GoEmotions label vocabulary the model emits
  over, each already registered as a `gmeow:AffectClassifierLabel` in
  `slices/core/affect/module.ttl`.
- `function_to_apply` / `score_semantics` / `threshold_policy` — the model card's
  documented multi-label sigmoid + 0.5-default configuration.

**Provenance — what is representative:** the per-label **score values** are a
representative multi-label-sigmoid profile, not a captured live inference. The
Hugging Face free inference API is now token-gated, and this repository runs **no
on-gate model inference** (determinism / no-network), so a live capture is not
reproduced here. The scores are chosen to be internally consistent for a warmly
positive comment: `gratitude` (0.90), `joy` (0.82), and `surprise` (0.55) cross
the 0.5 threshold, while everything else stays below it.

This exercises the full producer contract independent of the exact numbers:

- **Lossless ingest** — all 28 labels become `gmeow:AffectClassifierOutput`
  records (the `recover ∘ produce = id` round-trip proves it).
- **Claim routing** — `joy` and `surprise` cross threshold **and** carry an
  authored `skos:closeMatch` to a `gmeow:EmotionType`, so each supports an
  `AffectiveClaim`; `gratitude` crosses threshold but is a social/evaluative
  label with no emotion closeMatch, so it correctly supports **no** expresses-claim
  (evidence survives as an output+score either way).

To refresh with a genuine live capture, run the model once off-gate (an
authenticated `transformers` `text-classification` pipeline with
`function_to_apply="sigmoid"`, `top_k=None`) and replace the `scores` array;
the pinned `model_revision` must match the run.
