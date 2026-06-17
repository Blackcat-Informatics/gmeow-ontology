# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The eval harness (#298): mechanical scoring, pinned against the
hand-authored reference baseline whose properties are known by construction."""

from __future__ import annotations

import json

import pytest
from rdflib import Graph

from gmeow_tools.config import EVALS_DIR, PROJECT_ROOT
from gmeow_tools.evals import (
    Scorecard,
    _span_verified,
    all_scorecards,
    score_emissions,
)
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

_BASELINE = EVALS_DIR / "outputs" / "reference-baseline" / "claims.jsonl"


@pytest.fixture(scope="module")
def card() -> Scorecard:
    return score_emissions(_BASELINE)


def test_baseline_scores_are_pinned(card: Scorecard) -> None:
    """The reference baseline was engineered: 5/6 valid, 3/5 grounded,
    all 3 expectations recovered, no bait taken."""
    assert (card.emitted, card.valid) == (6, 5)
    assert card.scores["schema-validity"] == pytest.approx(0.8333, abs=1e-4)
    assert card.scores["grounding-precision"] == 0.6
    assert card.scores["grounding-recall"] == 1.0
    assert card.scores["hallucination-resistance"] == 0.6
    assert card.scores["abstention-quality"] == 1.0
    assert 0.6 < card.scores["calibration"] < 0.7


def test_quote_mismatch_is_not_grounded(card: Scorecard) -> None:
    """The 1997 claim has in-range offsets but a fabricated quote — the
    re-anchoring verifier rejects it (quotes are the truth; offsets bind
    only to a matching digest)."""
    # precision 0.6 over 5 valid claims = exactly 3 grounded: the two
    # fabrications (no-evidence + quote-mismatch) both failed.
    assert card.scores["grounding-precision"] == 0.6


def test_span_verifier_edges() -> None:
    text = "alpha beta gamma"
    ok = {"quote": "beta", "start": 6, "end": 10, "polarity": "supports"}
    assert _span_verified(ok, text, digest_current=True)
    # Quote present but offsets wrong: rejected when the digest is current...
    off = {"quote": "beta", "start": 0, "end": 4, "polarity": "supports"}
    assert not _span_verified(off, text, digest_current=True)
    # ...but accepted by re-anchoring when the source has moved on.
    assert _span_verified(off, text, digest_current=False)
    # Fabricated quote: rejected always.
    fab = {"quote": "delta", "start": 0, "end": 5, "polarity": "supports"}
    assert not _span_verified(fab, text, digest_current=False)
    # Out-of-range offsets with current digest: rejected.
    oor = {"quote": "beta", "start": 6, "end": 999, "polarity": "supports"}
    assert not _span_verified(oor, text, digest_current=True)


def test_leaderboard_and_scorecards_are_generated_and_current() -> None:
    """The evals generator's committed outputs match a fresh scoring run."""
    cards = all_scorecards()
    assert cards, "at least the reference baseline is committed"
    leaderboard = (PROJECT_ROOT / "generated" / "evals" / "leaderboard.md").read_text()
    for committed in cards:
        assert f"| {committed.model} |" in leaderboard
        payload = json.loads(
            (
                PROJECT_ROOT
                / "generated"
                / "evals"
                / f"{committed.model}.scorecard.json"
            ).read_text()
        )
        assert payload["scores"] == committed.scores


def test_scores_are_meta_claims_and_shacl_clean() -> None:
    """generated/evals/scores.ttl: vantage-indexed Assessments against the
    published rubric — instance data over the norms extension, gate-clean."""
    g = Graph()
    for triple in load_merged_graph(include_imports=False):
        g.add(triple)
    g.parse(EVALS_DIR / "rubric.ttl", format="turtle")
    g.parse(PROJECT_ROOT / "generated" / "evals" / "scores.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
