# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the real-data acceptance harness (#450).

Gate logic is pinned on small synthetic graphs (fast, deterministic); the
end-to-end pipeline is smoke-tested over one verbatim ``external/`` snapshot —
the honest corpus the harness exists to score. Per the issue the harness is a
*progress meter, red until done*, so these tests pin the harness's MACHINERY
(does each gate compute the right verdict), never a coverage threshold that would
re-introduce the gamed metric.
"""

from __future__ import annotations

from rdflib import RDF, Graph, Literal, URIRef

from gmeow_tools.acceptance import (
    _gate_external_validator,
    _gate_pure_gmeow,
    _gate_round_trip,
    _gate_size_invariant,
    default_corpus,
    render_report,
    run_acceptance,
)
from gmeow_tools.config import NAMESPACE

GM = NAMESPACE
FOAF = "http://xmlns.com/foaf/0.1/"


def test_pure_gmeow_gate_passes_on_gmeow_only_draft() -> None:
    """Gate 1 passes when every predicate/type is GMEOW (+ structural)."""
    draft = Graph()
    s = URIRef("https://ex.org/a")
    draft.add((s, URIRef(GM + "name"), Literal("Ada")))
    draft.add((s, RDF.type, URIRef(GM + "Person")))
    result = _gate_pure_gmeow(draft)
    assert result.passed
    assert result.hard


def test_pure_gmeow_gate_fails_on_consumer_residue() -> None:
    """Gate 1 fails (hard) when a consumer-vocab term survives into the draft."""
    draft = Graph()
    s = URIRef("https://ex.org/a")
    draft.add((s, URIRef(GM + "name"), Literal("Ada")))
    draft.add((s, URIRef(FOAF + "knows"), URIRef("https://ex.org/b")))
    result = _gate_pure_gmeow(draft)
    assert not result.passed
    assert result.metrics["residue"] == 1.0


def test_round_trip_gate_is_a_scoreboard_never_blocks() -> None:
    """Gate 2 reports per-vocab recall and is non-hard (a scoreboard)."""
    source = Graph()
    a, b = URIRef("https://ex.org/a"), URIRef("https://ex.org/b")
    source.add((a, URIRef(FOAF + "knows"), b))
    source.add((a, URIRef(FOAF + "name"), Literal("Ada")))
    output = Graph()
    output.add((a, URIRef(FOAF + "knows"), b))  # only one of the two recovered
    result = _gate_round_trip(source, output)
    assert not result.hard  # never blocks
    assert result.metrics["recall_pct"] == 50.0


def test_round_trip_keeps_distinct_languages_distinct() -> None:
    """A wrong-language round trip is a MISS, not a false recovery.

    Normalization folds an internal ``x-gmeow-*`` tag to its public BCP-47 form
    (the lossless retag) but must NOT collapse *different* languages: a source
    ``@en`` name that comes back ``@fr`` is a real failure the gate must surface.
    """
    a = URIRef("https://ex.org/a")
    source = Graph()
    source.add((a, URIRef(FOAF + "name"), Literal("Ada", lang="en")))
    # output carries the same text under a DIFFERENT language — not a recovery
    wrong_lang = Graph()
    wrong_lang.add((a, URIRef(FOAF + "name"), Literal("Ada", lang="fr")))
    assert _gate_round_trip(source, wrong_lang).metrics["recall_pct"] == 0.0
    # the matching public tag IS a recovery
    right_lang = Graph()
    right_lang.add((a, URIRef(FOAF + "name"), Literal("Ada", lang="en")))
    assert _gate_round_trip(source, right_lang).metrics["recall_pct"] == 100.0
    # an internal x-gmeow-english tag folds to en and still matches
    internal = Graph()
    internal.add((a, URIRef(FOAF + "name"), Literal("Ada", lang="x-gmeow-english")))
    assert _gate_round_trip(source, internal).metrics["recall_pct"] == 100.0


def test_round_trip_excludes_external_linkage_from_headline() -> None:
    """owl:sameAs to the outside world is external linkage — out of the headline."""
    source = Graph()
    a = URIRef("https://ex.org/a")
    from rdflib import OWL

    source.add((a, OWL.sameAs, URIRef("https://www.wikidata.org/entity/Q1")))
    source.add((a, URIRef(FOAF + "name"), Literal("Ada")))
    output = Graph()
    output.add((a, URIRef(FOAF + "name"), Literal("Ada")))
    result = _gate_round_trip(source, output)
    # foaf:name recovered (1/1) → 100%; the owl:sameAs miss does not count
    assert result.metrics["recall_pct"] == 100.0
    assert any("external linkage" in line for line in result.detail)


def test_size_invariant_gate() -> None:
    """Gate 3 passes iff the output strictly out-sizes the source."""
    source = Graph()
    source.add((URIRef("https://ex.org/a"), URIRef(GM + "name"), Literal("Ada")))
    bigger = Graph()
    for i in range(3):
        bigger.add((URIRef("https://ex.org/a"), URIRef(GM + f"p{i}"), Literal(str(i))))
    assert _gate_size_invariant(source, bigger).passed
    assert not _gate_size_invariant(source, Graph()).passed


def test_external_validator_x_gmeow_leak_is_hard_fail() -> None:
    """Gate 4 hard-fails when an internal x-gmeow-* tag leaks to the consumer."""
    leaky = Graph()
    leaky.add(
        (
            URIRef("https://ex.org/a"),
            URIRef(FOAF + "name"),
            Literal("Ada", lang="x-gmeow-english"),
        )
    )
    result = _gate_external_validator(leaky)
    assert result.hard
    assert not result.passed
    assert result.metrics["x_gmeow_leak"] == 1.0


def test_external_validator_passes_clean_consumer_output() -> None:
    """Public BCP-47 tags pass the hard x-gmeow check (report-only checks aside)."""
    clean = Graph()
    clean.add(
        (URIRef("https://ex.org/a"), URIRef(FOAF + "name"), Literal("Ada", lang="en"))
    )
    result = _gate_external_validator(clean)
    assert result.passed
    assert result.metrics["x_gmeow_leak"] == 0.0


def test_run_acceptance_over_a_real_snapshot() -> None:
    """End-to-end smoke: the harness scores a verbatim external snapshot, the hard
    gates pass, and the round-trip scoreboard reports a real (sub-100%) number."""
    corpus = default_corpus()
    assert corpus, "expected vendored external/ snapshots"
    # the smallest snapshot keeps the smoke test quick
    smallest = min(corpus, key=lambda p: p.stat().st_size)
    fa = run_acceptance(smallest)
    names = {g.name for g in fa.gates}
    assert names == {
        "pure-gmeow-intermediate",
        "round-trip-superset",
        "size-invariant",
        "external-validator",
        "honest-coverage",
    }
    # hard gates hold on real data; the scoreboard is honest (not faked to 100%)
    assert fa.passed
    rt = next(g for g in fa.gates if g.name == "round-trip-superset")
    assert 0.0 <= rt.metrics["recall_pct"] <= 100.0
    assert "PASS" in render_report([fa])
