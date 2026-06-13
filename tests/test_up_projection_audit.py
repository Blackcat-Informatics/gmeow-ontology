# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the up-projection invertibility audit (#449)."""

from __future__ import annotations

from gmeow_tools.up_projection_audit import (
    classify_sssom,
    combined_class,
    render_markdown,
    run_audit,
)


def test_sssom_relation_buckets() -> None:
    """Relation type drives reversibility, orientation-correctly."""
    # symmetric → clean either orientation
    assert classify_sssom("gmeow:Person", "skos:exactMatch", "foaf:Person")[0] == (
        "clean-reversible"
    )
    assert classify_sssom("foaf:Agent", "skos:exactMatch", "gmeow:Agent")[0] == (
        "clean-reversible"
    )
    # closeMatch → lift with a claim
    assert classify_sssom("gmeow:noteContent", "skos:closeMatch", "schema:text")[0] == (
        "liftable-with-claim"
    )
    # gmeow broader than target → generalizing lift is faithful
    assert classify_sssom("gmeow:Appellation", "skos:broadMatch", "schema:name")[0] == (
        "liftable-generalizing"
    )
    # gmeow narrower than target → narrowing would fabricate specificity
    assert classify_sssom("gmeow:X", "skos:narrowMatch", "schema:Y")[0] == (
        "down-only-narrowing"
    )
    # relatedMatch is too weak to lift faithfully
    assert classify_sssom("gmeow:X", "skos:relatedMatch", "schema:Y")[0] == (
        "down-only-related"
    )
    # neither side gmeow → not an up-projection cell
    assert classify_sssom("foaf:a", "skos:exactMatch", "schema:b")[0] == (
        "both-or-neither-gmeow"
    )


def test_combined_prefers_best_layer() -> None:
    """A term takes the best path across SSSOM + structural layers."""
    assert combined_class("x", {"x": "clean-reversible"}, {}) == "clean"
    assert combined_class("x", {}, {"x": "simple-1to1"}) == "clean"
    assert combined_class(
        "x", {"x": "liftable-with-claim"}, {"x": "structural-mint"}
    ) == ("liftable-with-claim")
    assert combined_class("x", {}, {"x": "structural-mint"}) == "hard-mint"
    assert combined_class("x", {"x": "down-only-related"}, {}) == "down-only"
    assert combined_class("x", {}, {}) == "GAP"


def test_real_data_baseline_is_sane() -> None:
    """The audit runs on the vendored real snapshots and is internally consistent."""
    report = run_audit()
    assert {f.name for f in report.files} == {"bii", "paudley"}
    # The vendored snapshots make this a REPRODUCIBLE baseline contract — pin
    # the exact numbers so any regression in invertibility coverage is caught.
    # Update deliberately (with the docs/up-projection-audit.md regen) when
    # cells or GMEOW coverage genuinely change.
    by_name = {f.name: f for f in report.files}
    assert (by_name["bii"].liftable, by_name["bii"].total) == (198, 269)
    assert (by_name["paudley"].liftable, by_name["paudley"].total) == (251, 332)
    assert (report.liftable, report.total) == (449, 601)
    assert len(report.gaps) == 100
    # gaps are de-duplicated and sorted across the corpus
    assert report.gaps == sorted(set(report.gaps))
    # the markdown renders with the headline + a gap section
    md = render_markdown(report)
    assert "Headline:" in md and "Coverage gaps" in md


def test_no_term_double_counted_within_a_file() -> None:
    report = run_audit()
    for f in report.files:
        # per_vocab counts sum to the per_term total (no term lost or doubled)
        vocab_total = sum(sum(c.values()) for c in f.per_vocab.values())
        assert vocab_total == f.total
