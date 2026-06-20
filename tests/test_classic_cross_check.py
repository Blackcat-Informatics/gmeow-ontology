# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the enforced native↔oracle divergence cross-check (#666, Task 4).

These tests are Docker-free: they exercise the authoritative Rust comparator
(``gmeow_logic.build_divergence_ledger``), the enforcement verdict, and the SARIF
emission over SYNTHETIC native/ELK/HermiT inputs. The real ELK/HermiT oracle run
is exercised in the ``make classic-cross-check`` lane / CI, never here.
"""

from __future__ import annotations

import json

import gmeow_logic
import pytest

from gmeow_tools import classic_cross_check as crosscheck

CROSSCHECK_WORLD = crosscheck.CROSSCHECK_WORLD


# --------------------------------------------------------------------------- #
# build_divergence_ledger — classification + tallies (Rust comparator)
# --------------------------------------------------------------------------- #


def test_ledger_all_agree_plus_dlgap() -> None:
    """Identical native+ELK subsumptions + matching consistency + a gap → no fail."""
    native = [("A", "B", CROSSCHECK_WORLD), ("B", "C", CROSSCHECK_WORLD)]
    elk = [("A", "B", CROSSCHECK_WORLD), ("B", "C", CROSSCHECK_WORLD)]
    ledger = gmeow_logic.build_divergence_ledger(
        native,
        elk,
        True,
        [],
        True,
        [],
        [("reason.dl-gap.complementOf", "beyond EL: complementOf")],
    )
    assert ledger["agree"] == 3  # 2 subsumptions + 1 consistency agreement
    assert ledger["native_only"] == 0
    assert ledger["oracle_only"] == 0
    assert ledger["dl_gap"] == 1


def test_ledger_native_only_subsumption() -> None:
    native = [("A", "B", CROSSCHECK_WORLD), ("B", "C", CROSSCHECK_WORLD)]
    elk = [("A", "B", CROSSCHECK_WORLD)]
    ledger = gmeow_logic.build_divergence_ledger(native, elk, True, [], True, [], [])
    assert ledger["native_only"] == 1
    assert ledger["oracle_only"] == 0
    native_rows = [r for r in ledger["rows"] if r["kind"] == "NativeOnly"]
    assert native_rows[0]["subject"] == "B"
    assert native_rows[0]["object"] == "C"
    assert native_rows[0]["category"] == "subsumption"


def test_ledger_oracle_only_subsumption() -> None:
    native = [("A", "B", CROSSCHECK_WORLD)]
    elk = [("A", "B", CROSSCHECK_WORLD), ("B", "C", CROSSCHECK_WORLD)]
    ledger = gmeow_logic.build_divergence_ledger(native, elk, True, [], True, [], [])
    assert ledger["oracle_only"] == 1
    assert ledger["native_only"] == 0
    oracle_rows = [r for r in ledger["rows"] if r["kind"] == "OracleOnly"]
    assert oracle_rows[0]["subject"] == "B"


def test_ledger_consistency_disagreement() -> None:
    """Native consistent, HermiT inconsistent → one NativeOnly + one OracleOnly."""
    ledger = gmeow_logic.build_divergence_ledger([], [], True, [], False, [], [])
    assert ledger["native_only"] == 1
    assert ledger["oracle_only"] == 1
    kinds = {r["kind"] for r in ledger["rows"]}
    assert "NativeOnly" in kinds
    assert "OracleOnly" in kinds


def test_ledger_bracket_normalization_agrees() -> None:
    """Native angle-bracketed display forms compare equal to ELK bare IRIs."""
    native = [("<http://x/A>", "<http://x/B>", f"<{CROSSCHECK_WORLD}>")]
    elk = [("http://x/A", "http://x/B", CROSSCHECK_WORLD)]
    ledger = gmeow_logic.build_divergence_ledger(native, elk, True, [], True, [], [])
    assert ledger["agree"] == 2  # 1 subsumption + 1 consistency
    assert ledger["native_only"] == 0
    assert ledger["oracle_only"] == 0


# --------------------------------------------------------------------------- #
# enforce — strict-by-default, DlGap-tolerant
# --------------------------------------------------------------------------- #


def test_enforce_passes_on_agree_plus_dlgap() -> None:
    ledger = gmeow_logic.build_divergence_ledger(
        [("A", "B", CROSSCHECK_WORLD)],
        [("A", "B", CROSSCHECK_WORLD)],
        True,
        [],
        True,
        [],
        [("reason.dl-gap.x", "honest gap")],
    )
    assert crosscheck.enforce(ledger) is True


def test_enforce_fails_on_native_only() -> None:
    ledger = gmeow_logic.build_divergence_ledger(
        [("A", "B", CROSSCHECK_WORLD)],
        [],
        True,
        [],
        True,
        [],
        [],
    )
    assert crosscheck.enforce(ledger) is False


def test_enforce_fails_on_oracle_only() -> None:
    ledger = gmeow_logic.build_divergence_ledger(
        [],
        [("A", "B", CROSSCHECK_WORLD)],
        True,
        [],
        True,
        [],
        [],
    )
    assert crosscheck.enforce(ledger) is False


def test_enforce_dlgap_alone_does_not_fail() -> None:
    """A DlGap is the ONLY honest-expected, non-failing class."""
    ledger = gmeow_logic.build_divergence_ledger(
        [],
        [],
        True,
        [],
        True,
        [],
        [("reason.dl-gap.disjoint", "beyond EL"), ("reason.dl-gap.union", "beyond EL")],
    )
    assert ledger["dl_gap"] == 2
    assert crosscheck.enforce(ledger) is True


# --------------------------------------------------------------------------- #
# build_report — SARIF carries the agreement matrix + per-tool timing
# --------------------------------------------------------------------------- #


def test_report_sarif_carries_agreement_and_timing() -> None:
    ledger = gmeow_logic.build_divergence_ledger(
        [("A", "B", CROSSCHECK_WORLD)],
        [("A", "B", CROSSCHECK_WORLD)],
        True,
        [],
        True,
        [],
        [("reason.dl-gap.x", "honest gap")],
    )
    report = crosscheck.build_report(ledger, elk_seconds=1.5, hermit_seconds=900.0)
    sarif = json.loads(report.to_sarif())
    rule_ids = {result["ruleId"] for result in sarif["runs"][0]["results"]}
    # Agreement-matrix + per-tool timing findings are present.
    assert crosscheck.RULE_AGREEMENT in rule_ids
    assert crosscheck.RULE_TIMING in rule_ids
    # The DlGap row carries the dl-gap rule-id.
    assert crosscheck.RULE_DL_GAP in rule_ids

    messages = [result["message"]["text"] for result in sarif["runs"][0]["results"]]
    assert any("agreement matrix" in m for m in messages)
    assert any("ELK" in m and "1.50s" in m for m in messages)
    assert any("HermiT" in m and "900.00s" in m for m in messages)


def test_report_divergence_is_error_severity() -> None:
    """NativeOnly / OracleOnly rows are error-severity (they fail the lane)."""
    ledger = gmeow_logic.build_divergence_ledger(
        [("A", "B", CROSSCHECK_WORLD)],
        [("C", "D", CROSSCHECK_WORLD)],
        True,
        [],
        True,
        [],
        [],
    )
    report = crosscheck.build_report(ledger, elk_seconds=1.0, hermit_seconds=2.0)
    assert report.error_count >= 2  # one native-only + one oracle-only
    assert not report.ok


def test_report_rule_id_mapping() -> None:
    """Subsumption vs consistency rows map to distinct rule-ids."""
    subsumption_row = {
        "kind": "NativeOnly",
        "category": "subsumption",
        "subject": "A",
        "object": "B",
    }
    consistency_row = {
        "kind": "OracleOnly",
        "category": "consistency",
        "subject": "",
        "object": "inconsistent",
    }
    gap_row = {"kind": "DlGap", "category": "consistency"}
    assert crosscheck._rule_id_for(subsumption_row) == (
        crosscheck.RULE_SUBSUMPTION_DIVERGENCE
    )
    assert crosscheck._rule_id_for(consistency_row) == (
        crosscheck.RULE_CONSISTENCY_DIVERGENCE
    )
    assert crosscheck._rule_id_for(gap_row) == crosscheck.RULE_DL_GAP


# --------------------------------------------------------------------------- #
# native-result extraction helpers
# --------------------------------------------------------------------------- #


def test_native_subsumptions_drops_self_and_non_subclassof() -> None:
    result = {
        "inferred": [
            {
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "<http://x/B>",
                "world": "http://x/w1",
                "is_edb": False,
            },
            {  # self-subsumption — dropped
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "<http://x/A>",
                "world": "http://x/w1",
                "is_edb": True,
            },
            {  # non-subClassOf — dropped
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#label",
                "object": '"A"',
                "world": "http://x/w1",
                "is_edb": True,
            },
        ]
    }
    subs = crosscheck.native_subsumptions(result)
    assert subs == [("http://x/A", "http://x/B", CROSSCHECK_WORLD)]


def test_native_subsumptions_collapses_worlds() -> None:
    """The same edge in two worlds collapses to one cross-check-world key."""
    result = {
        "inferred": [
            {
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "http://x/B",
                "world": "http://x/w1",
                "is_edb": False,
            },
            {
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "http://x/B",
                "world": "http://x/w2",
                "is_edb": False,
            },
        ]
    }
    subs = crosscheck.native_subsumptions(result)
    assert subs == [("http://x/A", "http://x/B", CROSSCHECK_WORLD)]


# --------------------------------------------------------------------------- #
# corpus alignment — told facts, transitive closure, equivalence expansion
# --------------------------------------------------------------------------- #


def test_transitive_closure() -> None:
    edges = {("A", "B"), ("B", "C"), ("C", "D")}
    closed = crosscheck._transitive_closure(edges)
    assert ("A", "C") in closed
    assert ("A", "D") in closed
    assert ("B", "D") in closed


def test_write_told_facts_only_edb(tmp_path: object) -> None:
    """Only ``is_edb`` axioms are serialized; the world is dropped (flattened)."""
    from pathlib import Path

    out = Path(str(tmp_path)) / "told.ttl"
    native = {
        "inferred": [
            {
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "<http://x/B>",
                "world": "http://x/w1",
                "is_edb": True,
            },
            {  # derived — excluded from the told corpus
                "subject": "http://x/A",
                "predicate": "http://www.w3.org/2000/01/rdf-schema#subClassOf",
                "object": "<http://x/C>",
                "world": "http://x/w1",
                "is_edb": False,
            },
        ]
    }
    crosscheck.write_told_facts(native, path=out)
    text = out.read_text(encoding="utf-8")
    assert "http://x/B" in text  # told edge present
    assert "http://x/C" not in text  # derived edge excluded
    assert "owl#Ontology" in text  # ontology header stamped


def test_subsumption_edges_expands_equivalence() -> None:
    """``owl:equivalentClass`` expands into a bidirectional subsumption pair."""
    import gmeow_rdf

    store = gmeow_rdf.Store()
    store.load(
        (
            b"<http://x/A> "
            b"<http://www.w3.org/2002/07/owl#equivalentClass> <http://x/B> .\n"
            b"<http://x/C> "
            b"<http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://x/A> .\n"
        ),
        format=gmeow_rdf.RdfFormat.N_TRIPLES,
    )
    edges = crosscheck._subsumption_edges(store)
    assert ("http://x/A", "http://x/B") in edges
    assert ("http://x/B", "http://x/A") in edges  # bidirectional
    assert ("http://x/C", "http://x/A") in edges


# --------------------------------------------------------------------------- #
# Docker oracle integration (the REAL lane) — runs ONLY in classic-cross-check
# --------------------------------------------------------------------------- #


@pytest.mark.classic_cross_check
def test_enforced_lane_agrees_on_real_bundle(tmp_path: object) -> None:
    """The real ELK + HermiT oracles agree with native on the committed bundle.

    Exercises the FULL enforced lane: native reasoning → told-facts staging →
    ELK + HermiT over the SAME corpus → Rust comparator → SARIF + enforcement.
    Native ≡ oracle on the real bundle, so enforcement passes (exit 0) and the
    only honest-expected non-agreement class is ``DlGap``.
    """
    from pathlib import Path

    passed, ledger, report = crosscheck.run(output_dir=Path(str(tmp_path)))
    assert passed, (
        f"native↔oracle divergence on the real bundle: "
        f"native_only={ledger['native_only']} oracle_only={ledger['oracle_only']}"
    )
    assert ledger["native_only"] == 0
    assert ledger["oracle_only"] == 0
    assert ledger["agree"] > 0
    assert report.ok
    # The SARIF/JSON artifacts were written to the lane output dir.
    assert (Path(str(tmp_path)) / f"{crosscheck.CROSSCHECK_STEM}.sarif").exists()
