# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The claim-extraction eval suite (#298): which models emit valid GMEOW?

Scores model emissions against the SAME gates the pattern ships (#55):
schema validity, mechanical span verification (offsets bind to the corpus
digest; quotes re-anchor), grounding precision/recall against the published
expectations, hallucination rate (the AIS framing), abstention quality
(unsupported bait must be declined), and calibration (stated confidence vs
measured grounding). Zero human judgment per run.

Scores are themselves GMEOW claims: the ``evals`` generator emits them as
vantage-indexed ``gmeow:Assessment`` individuals against the published rubric
(``evals/rubric.ttl``) — "evaluation is meta-claims" (#54), dogfooded.

The network half (``gmeow evals run``) calls model APIs and is gated like
``gmeow quality`` — never part of ``make check``.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import jsonschema
from rdflib import Graph

from gmeow_tools.config import EVALS_DIR, GENERATED_EVALS_DIR, PROJECT_ROOT
from gmeow_tools.generator import Generator, register

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

_SCHEMA_FILE = EVALS_DIR / "claim-emission.schema.json"
_CORPUS_FILE = EVALS_DIR / "corpus.ttl"
_EXPECTATIONS_FILE = EVALS_DIR / "expectations.json"
_OUTPUTS_DIR = EVALS_DIR / "outputs"

#: Criterion local names, in leaderboard column order.
_CRITERIA = (
    "schema-validity",
    "grounding-precision",
    "grounding-recall",
    "hallucination-resistance",
    "abstention-quality",
    "calibration",
)


def _slug(model: str) -> str:
    """Path- and IRI-safe key for a model id (raw id stays in metadata).

    Provider-style ids contain ``/`` and ``.``; a raw id used as a path
    would nest directories (or escape the outputs root via ``..``) and as a
    Turtle local name would be illegal. Sanitization is many-to-one
    ("openai/gpt-4.1" and "openai.gpt-4-1" collide), so a LOSSY slug gains a
    short content-hash suffix of the raw id; an already-clean id is its own
    slug, so committed directory names stay stable.
    """
    import re
    from hashlib import blake2s

    sanitized = re.sub(r"[^A-Za-z0-9_-]+", "-", model).strip("-")
    if sanitized == model and model:
        return model  # already safe, any case — its own slug, byte-stable
    base = (sanitized or "model").lower()
    return f"{base}-{blake2s(model.encode('utf-8'), digest_size=4).hexdigest()}"


@dataclass(slots=True)
class Scorecard:
    """One model's mechanical scores, all in [0, 1]."""

    model: str
    emitted: int = 0
    valid: int = 0
    scores: dict[str, float] = field(default_factory=dict)
    notes: list[str] = field(default_factory=list)

    @property
    def overall(self) -> float:
        """Unweighted mean across the rubric criteria."""
        if not self.scores:
            return 0.0
        return sum(self.scores.values()) / len(self.scores)


def _corpus_texts() -> dict[str, tuple[str, str]]:
    """SourceLocation → (text, declared digest) from the corpus manifest."""
    from rdflib import URIRef

    ns = "https://blackcatinformatics.ca/gmeow/"
    graph = Graph().parse(_CORPUS_FILE, format="turtle")
    out: dict[str, tuple[str, str]] = {}
    for subject in graph.subjects(URIRef(ns + "sourceLocation"), None):
        location = str(graph.value(subject, URIRef(ns + "sourceLocation")))
        digest = str(graph.value(subject, URIRef(ns + "contentDigest")) or "")
        out[location] = ((PROJECT_ROOT / location).read_text(encoding="utf-8"), digest)
    return out


def _current_digest(text: str) -> str:
    from blake3 import blake3

    return "blake3:" + blake3(text.encode("utf-8")).hexdigest()


def _span_verified(span: dict[str, object], text: str, digest_current: bool) -> bool:
    """Mechanical span verification: quote re-anchors; offsets bind to digest."""
    quote = str(span["quote"])
    if quote not in text:
        return False
    if digest_current:
        start, end = int(str(span["start"])), int(str(span["end"]))
        if not (0 <= start < end <= len(text)):
            return False
        if text[start:end] != quote:
            return False
    return True


def score_emissions(jsonl_path: Path) -> Scorecard:
    """Score one model's emission file against the published contract."""
    model = jsonl_path.parent.name
    schema = json.loads(_SCHEMA_FILE.read_text(encoding="utf-8"))
    validator = jsonschema.Draft202012Validator(schema)
    corpus = _corpus_texts()
    expectations = json.loads(_EXPECTATIONS_FILE.read_text(encoding="utf-8"))

    card = Scorecard(model=model)
    claims: list[dict[str, object]] = []
    for line in jsonl_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        card.emitted += 1
        try:
            obj = json.loads(line)
            validator.validate(obj)
        except (json.JSONDecodeError, jsonschema.ValidationError) as exc:
            card.notes.append(f"invalid line: {str(exc).splitlines()[0][:100]}")
            continue
        card.valid += 1
        claims.append(obj)
    card.scores["schema-validity"] = card.valid / card.emitted if card.emitted else 0.0

    grounded_flags: list[bool] = []
    verified_quotes: list[str] = []
    for claim in claims:
        source = str(claim.get("source", ""))
        text, declared = corpus.get(source, ("", ""))
        digest_current = bool(text) and _current_digest(text) == declared
        spans = claim.get("evidence") or []
        assert isinstance(spans, list)
        supporting_ok = False
        for span in spans:
            assert isinstance(span, dict)
            if _span_verified(span, text, digest_current):
                verified_quotes.append(str(span["quote"]))
                if span.get("polarity") == "supports":
                    supporting_ok = True
        grounded_flags.append(supporting_ok)

    n = len(claims)
    grounded = sum(grounded_flags)
    card.scores["grounding-precision"] = grounded / n if n else 0.0
    card.scores["hallucination-resistance"] = 1.0 - (n - grounded) / n if n else 0.0

    expected = expectations["expected"]
    recovered = sum(
        1
        for item in expected
        if any(item["must_quote"] in quote for quote in verified_quotes)
    )
    card.scores["grounding-recall"] = recovered / len(expected) if expected else 1.0

    bait = expectations["bait"]
    taken = 0
    for item in bait:
        keywords = [k.lower() for k in item["keywords"]]
        for claim in claims:
            text_lower = str(claim.get("text", "")).lower()
            if all(k in text_lower for k in keywords):
                taken += 1
                break
    card.scores["abstention-quality"] = 1.0 - (taken / len(bait)) if bait else 1.0

    errors = [
        abs(float(str(claim.get("confidence", 0.5))) - (1.0 if ok else 0.0))
        for claim, ok in zip(claims, grounded_flags, strict=True)
    ]
    card.scores["calibration"] = 1.0 - sum(errors) / len(errors) if errors else 1.0
    for key in card.scores:
        card.scores[key] = round(card.scores[key], 4)
    return card


def all_scorecards() -> list[Scorecard]:
    """Score every committed emission, sorted by overall score descending."""
    cards = [
        score_emissions(path) for path in sorted(_OUTPUTS_DIR.glob("*/claims.jsonl"))
    ]
    return sorted(cards, key=lambda c: (-c.overall, c.model))


def _render_leaderboard(cards: list[Scorecard]) -> str:
    lines = [
        "<!-- GENERATED by `gmeow regenerate` (evals) — DO NOT EDIT (#298). -->",
        "",
        "# gmeow-evals leaderboard: which models emit valid GMEOW claims?",
        "",
        "Mechanical scores (0 to 1) against the published contract: the",
        "[extraction prompt](../../docs/prompts/claim-extraction-v1.md), the",
        "[emission schema](../../evals/claim-emission.schema.json), and the",
        "[#55 audit gates](../../docs/hallucination-resistant-kg.md), under the",
        "published [rubric](../../evals/rubric.ttl). Scores are themselves",
        "GMEOW claims — see `scores.ttl` (vantage-indexed Assessments).",
        "",
        "| model | overall | " + " | ".join(_CRITERIA) + " | claims |",
        "|---|---|" + "---|" * len(_CRITERIA) + "---|",
    ]
    for card in cards:
        cells = " | ".join(f"{card.scores.get(c, 0.0):.2f}" for c in _CRITERIA)
        lines.append(
            f"| {card.model} | {card.overall:.2f} | {cells} "
            f"| {card.valid}/{card.emitted} |"
        )
    lines.append("")
    lines.append(
        "Run `gmeow evals run --endpoint …` (network) to add a model; "
        "`gmeow evals score` re-scores committed emissions offline."
    )
    return "\n".join(lines) + "\n"


def _render_scores_ttl(cards: list[Scorecard]) -> str:
    """The scores as vantage-indexed Assessments — meta-claims, dogfooded."""
    lines = [
        "# GENERATED by `gmeow regenerate` (evals) — DO NOT EDIT (#298).",
        "# Evaluation is meta-claims (#54): each score is a vantage-indexed",
        "# Assessment by the harness against the published rubric — attributed,",
        "# contestable, never a detached dashboard number.",
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .",
        "@prefix ev:    <https://blackcatinformatics.ca/gmeow/evals/> .",
        "",
    ]
    for card in cards:
        safe = _slug(card.model)
        lines.append(f"ev:model-{safe} a gmeow:SoftwareAgent .")
        for criterion in _CRITERIA:
            crit_local = {
                "schema-validity": "crit-schema-validity",
                "grounding-precision": "crit-grounding-precision",
                "grounding-recall": "crit-grounding-recall",
                "hallucination-resistance": "crit-hallucination",
                "abstention-quality": "crit-abstention",
                "calibration": "crit-calibration",
            }[criterion]
            lines.append(
                f"ev:assessment-{safe}-{criterion} a gmeow:Assessment ;\n"
                f"    gmeow:vantage ev:harness ;\n"
                f"    gmeow:assessmentTarget ev:model-{safe} ;\n"
                f"    gmeow:assessmentCriterion ev:{crit_local} ;\n"
                f"    gmeow:assessmentRubric ev:rubric ;\n"
                f"    gmeow:assessmentScoreValue {card.scores.get(criterion, 0.0)} ."
            )
        lines.append("")
    return "\n".join(lines)


@register
class EvalsGenerator(Generator):
    """Emit scorecards + leaderboard + meta-claim scores from emissions."""

    name: str = "evals"

    @property
    def inputs(self) -> Sequence[Path]:
        """The contract + the committed emissions + the corpus sources.

        The source files are DERIVED from the corpus manifest — adding or
        renaming a corpus entry invalidates the generator with it.
        """
        corpus_sources = [
            PROJECT_ROOT / location for location in sorted(_corpus_texts())
        ]
        return [
            _SCHEMA_FILE,
            _CORPUS_FILE,
            _EXPECTATIONS_FILE,
            EVALS_DIR / "rubric.ttl",
            *sorted(_OUTPUTS_DIR.glob("*/claims.jsonl")),
            *sorted(_OUTPUTS_DIR.glob("*/meta.json")),
            *corpus_sources,
        ]

    @property
    def outputs(self) -> Sequence[Path]:
        """Leaderboard + per-model scorecards + the meta-claim scores."""
        models = sorted(p.parent.name for p in _OUTPUTS_DIR.glob("*/claims.jsonl"))
        return [
            GENERATED_EVALS_DIR / "leaderboard.md",
            GENERATED_EVALS_DIR / "scores.ttl",
            *(GENERATED_EVALS_DIR / f"{m}.scorecard.json" for m in models),
        ]

    def render(self, staging: Path) -> None:
        """Score every committed emission and render the artifacts."""
        target = staging / GENERATED_EVALS_DIR.relative_to(PROJECT_ROOT)
        target.mkdir(parents=True, exist_ok=True)
        cards = all_scorecards()
        (target / "leaderboard.md").write_text(
            _render_leaderboard(cards), encoding="utf-8"
        )
        (target / "scores.ttl").write_text(_render_scores_ttl(cards), encoding="utf-8")
        for card in cards:
            payload = {
                "model": card.model,
                "emitted": card.emitted,
                "valid": card.valid,
                "overall": round(card.overall, 4),
                "scores": card.scores,
                "notes": card.notes,
            }
            (target / f"{card.model}.scorecard.json").write_text(
                json.dumps(payload, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )


# --------------------------------------------------------------------------- #
# The network half: run a model against the corpus (gated like `gmeow
# quality` — keys from env, never part of make check).
# --------------------------------------------------------------------------- #


def _build_prompt(location: str, text: str, digest: str) -> str:
    """Fill the published template for one corpus document."""
    template = (
        (PROJECT_ROOT / "docs" / "prompts" / "claim-extraction-v1.md")
        .read_text(encoding="utf-8")
        .split("```text", 1)[1]
        .split("```", 1)[0]
        .strip()
    )
    return (
        template.replace("{source_id}", location)
        .replace("{content_digest}", digest)
        .replace("{document_text}", text)
    )


def run_model(
    *,
    model: str,
    endpoint: str,
    api: str = "openai",
    api_key: str | None = None,
    timeout: float = 120.0,
) -> Path:
    """Call a model API over the corpus; write its emission + meta.

    ``api``: ``openai`` (chat-completions-compatible) or ``anthropic``
    (messages API). The emission lands in ``evals/outputs/<model>/`` ready
    for ``gmeow evals score`` / the ``evals`` generator.
    """
    import os

    import httpx

    key = api_key or os.environ.get(
        "ANTHROPIC_API_KEY" if api == "anthropic" else "OPENAI_API_KEY", ""
    )
    out_dir = _OUTPUTS_DIR / _slug(model)
    out_dir.mkdir(parents=True, exist_ok=True)
    lines: list[str] = []
    for location, (text, digest) in sorted(_corpus_texts().items()):
        prompt = _build_prompt(location, text, digest)
        if api == "anthropic":
            response = httpx.post(
                endpoint,
                timeout=timeout,
                headers={
                    "x-api-key": key,
                    "anthropic-version": "2023-06-01",
                    "content-type": "application/json",
                },
                json={
                    "model": model,
                    "max_tokens": 4096,
                    "messages": [{"role": "user", "content": prompt}],
                },
            )
            response.raise_for_status()
            content = response.json()["content"][0]["text"]
        else:
            response = httpx.post(
                endpoint,
                timeout=timeout,
                headers={"Authorization": f"Bearer {key}"},
                json={
                    "model": model,
                    "temperature": 0,
                    "messages": [{"role": "user", "content": prompt}],
                },
            )
            response.raise_for_status()
            content = response.json()["choices"][0]["message"]["content"]
        for line in content.splitlines():
            line = line.strip()
            if line.startswith("{"):
                lines.append(line)
    (out_dir / "claims.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (out_dir / "meta.json").write_text(
        json.dumps(
            {"model": model, "api": api, "endpoint": endpoint},
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return out_dir
