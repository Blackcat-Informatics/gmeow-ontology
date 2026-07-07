#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
#
# ── UNATTACHED, UNMAINTAINED, OFF-GATE MAINTAINER ARTIFACT ──────────────────────
# This script is NOT part of the ontology, NOT wired into the Makefile, NOT
# collected by pytest, NOT imported by any code, and NOT covered by any gate. It is
# a one-shot maintainer tool kept only so the affect classifier capture fixtures can
# be REGENERATED if we ever need to. The repository itself runs NO on-gate model
# inference (determinism / no-network); this tool is run by a maintainer OFF-GATE.
#
# It regenerates, byte-for-byte, the checked-in fixtures under
#   crates/affect-ingest/fixtures/{goemotions,sst2,cardiff,ekman7,zeroshot}-sample.json
# by loading each Hugging Face model at its PINNED commit and running it over three
# fixed target texts. The scores it writes are genuine model output.
#
# Run (ephemeral env — nothing is added to the repo's own environment):
#
#   uv run --no-project --python 3.12 \
#       --with 'torch==2.*' --with 'transformers==4.*' --with 'numpy<3' \
#       python maintenance/affect-classifier-capture/capture_fixtures.py \
#       --out crates/affect-ingest/fixtures
#
# Then confirm the working tree is unchanged (a genuine, deterministic capture):
#
#   git diff --stat crates/affect-ingest/fixtures
#
# If a model's `main` is re-pinned to a new commit, update REV below AND the pinned
# revision in crates/affect-ingest/fixtures/README.md, then re-run.

import argparse
import json
import os
from pathlib import Path

os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

from transformers import pipeline  # noqa: E402  (imported after env setup)

# ── Fixed inputs — the exact provenance the fixtures encode ─────────────────────

# Three real target texts (a grateful, a joyful, and a fearful sentence), run
# through every classifier so each fixture exercises the mapped-emotion claim path,
# the sentiment/social no-claim path, and the below-threshold "concluded" path.
TEXTS = [
    ("gratitude", "Thank you so much, this genuinely made my whole day — I really appreciate it!"),
    ("joy", "I am absolutely thrilled and overjoyed — this is the happiest I have felt in years!"),
    ("fear", "This is terrifying and I am deeply afraid of what happens next."),
]

# The target-IRI scheme: one classified span per (model, text-slug).
IRI_BASE = "https://blackcatinformatics.ca/gmeow/examples/affect"

# Pinned model commit revisions — the exact SHAs the fixtures assert.
REV = {
    "goemotions": "d75048347613a25d77de8cf6412eaae9fa7b26be",
    "sst2": "714eb0fa89d2f80546fda750413ed43d93601a13",
    "cardiff": "3216a57f2a0d9c45a2e6c20157c20c49fb4bf9c7",
    "ekman7": "0e1cd914e3d46199ed785853e12b57304e04178b",
    "zeroshot": "d7645e127eaf1aefc7862fd59a17a5aa8558b8ce",
}

# raw model label → GMEOW registry-local (the lossless external-label→registry-identity
# step). GoEmotions labels ARE their registry locals (lowercase).
GOEMOTIONS_LABELS = [
    "admiration", "amusement", "anger", "annoyance", "approval", "caring", "confusion",
    "curiosity", "desire", "disappointment", "disapproval", "disgust", "embarrassment",
    "excitement", "fear", "gratitude", "grief", "joy", "love", "nervousness", "neutral",
    "optimism", "pride", "realization", "relief", "remorse", "sadness", "surprise",
]
LABEL_MAP = {
    "goemotions": {label: label for label in GOEMOTIONS_LABELS},
    "sst2": {"POSITIVE": "sst2Positive", "NEGATIVE": "sst2Negative"},
    "cardiff": {"negative": "cardiffNegative", "neutral": "cardiffNeutral", "positive": "cardiffPositive"},
    "ekman7": {
        "anger": "ekmanAnger", "disgust": "ekmanDisgust", "fear": "ekmanFear",
        "joy": "ekmanJoy", "neutral": "ekmanNeutral", "sadness": "ekmanSadness",
        "surprise": "ekmanSurprise",
    },
}

# Zero-shot (NLI entailment) run-scoped candidate set + hypothesis template. These
# are part of the run identity (not a static gmeow:AffectLabelSet), preserved in the
# original prompt order.
ZS_CANDIDATES = ["joy", "anger", "fear", "sadness", "surprise", "disgust"]
ZS_TEMPLATE = "This text expresses {}."


def write_fixture(out_dir: Path, name: str, obj: dict) -> None:
    """Write a fixture with the exact on-disk shape (indent 2, trailing newline)."""
    path = out_dir / f"{name}-sample.json"
    path.write_text(json.dumps(obj, indent=2) + "\n")
    print(f"WROTE {path} ({len(obj['targets'][0]['scores'])} labels/target)", flush=True)


def text_classification_targets(repo: str, function: str, rev: str, label_map: dict) -> list:
    """Run a text-classification pipeline over each TEXT; one target per text."""
    clf = pipeline(
        "text-classification",
        model=repo,
        revision=rev,
        top_k=None,
        function_to_apply=function,
    )
    targets = []
    for slug, text in TEXTS:
        raw = clf(text)
        rows = raw[0] if raw and isinstance(raw[0], list) else raw
        scores = [{"label": label_map[r["label"]], "score": float(r["score"])} for r in rows]
        scores.sort(key=lambda s: s["label"])
        targets.append({"target_iri": f"{IRI_BASE}/{_slug_dir(repo)}/{slug}", "scores": scores})
    return targets


def _slug_dir(repo: str) -> str:
    return {
        "SamLowe/roberta-base-go_emotions": "goemotions",
        "distilbert-base-uncased-finetuned-sst-2-english": "sst2",
        "cardiffnlp/twitter-roberta-base-sentiment-latest": "cardiff",
        "j-hartmann/emotion-english-distilroberta-base": "ekman",
        "facebook/bart-large-mnli": "zeroshot",
    }[repo]


def capture_goemotions(out_dir: Path) -> None:
    targets = text_classification_targets(
        "SamLowe/roberta-base-go_emotions", "sigmoid", REV["goemotions"], LABEL_MAP["goemotions"]
    )
    write_fixture(out_dir, "goemotions", {
        "model_identifier": "SamLowe/roberta-base-go_emotions",
        "model_revision": REV["goemotions"],
        "model_framework": "transformers",
        "model_task": "text-classification",
        "function_to_apply": "sigmoid",
        "return_all_scores": True,
        "label_set_id": "GoEmotions",
        "score_semantics": "sigmoid",
        "tokenizer_revision": REV["goemotions"],
        "threshold_policy": {"kind": "global", "value": 0.5},
        "targets": targets,
    })


def _softmax_fixture(out_dir: Path, name: str, repo: str, label_set_id: str) -> None:
    targets = text_classification_targets(repo, "softmax", REV[name], LABEL_MAP[name])
    write_fixture(out_dir, name, {
        "model_identifier": repo,
        "model_revision": REV[name],
        "model_framework": "transformers",
        "model_task": "text-classification",
        "function_to_apply": "softmax",
        "return_all_scores": True,
        "label_set_id": label_set_id,
        "score_semantics": "softmax",
        "tokenizer_revision": REV[name],
        "threshold_policy": {"kind": "global", "value": 0.5},
        "targets": targets,
    })


def capture_zeroshot(out_dir: Path) -> None:
    repo = "facebook/bart-large-mnli"
    zs = pipeline("zero-shot-classification", model=repo, revision=REV["zeroshot"])
    targets = []
    for slug, text in TEXTS:
        res = zs(text, ZS_CANDIDATES, hypothesis_template=ZS_TEMPLATE, multi_label=True)
        scores = [{"label": lbl, "score": float(sc)} for lbl, sc in zip(res["labels"], res["scores"])]
        scores.sort(key=lambda s: s["label"])
        targets.append({"target_iri": f"{IRI_BASE}/zeroshot/{slug}", "scores": scores})
    write_fixture(out_dir, "zeroshot", {
        "model_identifier": repo,
        "model_revision": REV["zeroshot"],
        "model_framework": "transformers",
        "model_task": "zero-shot-classification",
        "function_to_apply": "entailment",
        "return_all_scores": True,
        "label_set_id": "ZeroShotEmotion6",
        "score_semantics": "entailment",
        "tokenizer_revision": REV["zeroshot"],
        "hypothesis_template": ZS_TEMPLATE,
        "candidate_labels": ZS_CANDIDATES,
        "label_set_revision": "candidates:" + ",".join(sorted(ZS_CANDIDATES)),
        "threshold_policy": {"kind": "global", "value": 0.5},
        "targets": targets,
    })


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="fixtures output directory")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    capture_goemotions(args.out)
    _softmax_fixture(args.out, "sst2", "distilbert-base-uncased-finetuned-sst-2-english", "SST2")
    _softmax_fixture(args.out, "cardiff", "cardiffnlp/twitter-roberta-base-sentiment-latest", "CardiffTweetEval")
    _softmax_fixture(args.out, "ekman7", "j-hartmann/emotion-english-distilroberta-base", "Ekman7")
    capture_zeroshot(args.out)
    print("CAPTURE DONE", flush=True)


if __name__ == "__main__":
    main()
