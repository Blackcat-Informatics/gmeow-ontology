# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the gate-derived up-projection invertibility audit."""

from __future__ import annotations

from gmeow_tools.up_projection_audit import (
    classify_sssom,
    combined_class,
    gate_audit,
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


def test_gate_derived_baseline_is_sane() -> None:
    """The gate audit runs on the vendored real snapshots and is consistent.

    The headline is a correspondence-gate verdict ledger: ``proved`` (round-trip
    verified over two independently-sourced real projection rules) vs ``claimed``
    (liftable, asserted by the alignment relation, not proved by inversion). The
    vendored snapshots make this a REPRODUCIBLE baseline contract — pin the exact
    numbers so any regression is caught; update deliberately (with the
    docs/up-projection-audit.md regen) when cells or GMEOW coverage change.
    """
    report = gate_audit()
    # The four tiers strictly partition every audited term.
    proved = report["proved"]
    claimed = report["claimed"]
    excluded = report["red_excluded"]
    unsupported = report["unsupported"]
    total = report["total"]
    assert proved + claimed + excluded + unsupported == total
    # The liftable headline numerator is proved + claimed.
    assert report["liftable"] == proved + claimed
    # Pinned gate-derived baseline over the vendored corpus. The 479
    # historically-"liftable" terms are all CLAIMS (alignment-asserted): the
    # corpus authors no independent forward + reverse path pair, so zero terms
    # are structurally proved-invertible — the gate machinery is exercised (and
    # would exclude a non-inverting pair, see the Rust gate-audit tests).
    assert (proved, claimed, excluded, unsupported, total) == (0, 479, 0, 112, 591)
    assert report["liftable"] == 479
    assert len(report["gaps"]) == 72
    # gaps are de-duplicated and sorted across the corpus.
    assert report["gaps"] == sorted(set(report["gaps"]))
    # the markdown renders with the gate-derived headline + a gap section.
    md = report["markdown"]
    assert "Headline:" in md
    assert "Coverage gaps" in md
    assert "proved" in md
    assert "claimed" in md


def test_binding_surfaces_mirror_and_per_vocab_shape() -> None:
    """The PyO3 binding exposes the ledger consistently across its surfaces.

    This guards the wrapper contract (the convenience top-level figures mirror
    the nested ``totals`` dict, and every per-vocab cell carries the full tier
    shape) — NOT the partition arithmetic, which the native Rust gate-audit
    tests own.
    """
    report = gate_audit()
    tier_keys = {
        "proved",
        "claimed",
        "red_excluded",
        "unsupported",
        "liftable",
        "total",
    }
    # The convenience top-level figures mirror the nested totals dict.
    for key in tier_keys:
        assert report[key] == report["totals"][key], key
    # Every per-vocabulary cell carries the full tier shape.
    for vocab, counts in report["per_vocab"].items():
        assert set(counts) == tier_keys, vocab


def test_iri_matching_no_false_gaps_from_prefix_skew() -> None:
    """The geosparql namespace appears as geo: in our cells but geosparql: in the
    real files; IRI-based matching must treat them as one term, not a false gap."""
    from gmeow_tools.up_projection_audit import _canon_qname, _to_iri

    # config curies resolve to full IRIs; the real files supply full IRIs
    # directly, so the geo: cells and the files' geosparql: usage share one IRI.
    geo_iri = "http://www.opengis.net/ont/geosparql#"
    assert _to_iri("geo:Geometry") == geo_iri + "Geometry"
    assert _canon_qname(geo_iri + "Geometry").endswith(":Geometry")
    report = gate_audit()
    # geosparql geometry terms are covered (geo: cells), never reported as gaps
    geo_gaps = [t for t in report["gaps"] if "Geometry" in t or "asWKT" in t]
    assert geo_gaps == [], f"prefix-skew false gaps: {geo_gaps}"
