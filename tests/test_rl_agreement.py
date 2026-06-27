# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the native-RL ≡ owlrl-RL agreement axis of the lane (#666 Task 5).

Two layers:

* The canonicalization + enforcement logic is exercised Docker-free here over
  synthetic closures (the tautology filter, the named-vocabulary restriction, the
  literal value-space normalization, and the strict pass/fail verdict).
* The end-to-end ``rl_agreement.run()`` over the real bundle is a
  ``classic_cross_check``-marked lane test (not required for normal repo use): it
  proves the axis ENFORCES — passes on real agreement, fails on a synthetic
  divergence.
"""

from __future__ import annotations

from pathlib import Path
from typing import cast

import pytest

# The native-RL ≡ owlrl-RL agreement axis is the classic_cross_check oracle lane:
# it builds rdflib graphs and runs the upstream owlrl reasoner over them. Both stay
# on REAL rdflib/owlrl (installed via the `.[crosscheck]` extra); skip-collect this
# whole module when they are absent from the runtime (purrdf P0, #834).
pytest.importorskip("rdflib")
pytest.importorskip("owlrl")

from rdflib import (
    OWL,
    RDF,
    RDFS,
    BNode,
    Graph,
    Literal,
    Namespace,
    URIRef,
)

from gmeow_tools.config import GTS_SNAPSHOT_FILE
from gmeow_tools.oracles import rl_agreement

EX = Namespace("https://example.org/rl/")
XSD = "http://www.w3.org/2001/XMLSchema#"


# --------------------------------------------------------------------------- #
# Canonicalization (Docker-free, pure logic)
# --------------------------------------------------------------------------- #


def test_canonical_drops_bnode_bearing_triples() -> None:
    g = Graph()
    bnode = BNode()
    g.add((EX.x, RDF.type, EX.C))  # kept
    g.add((EX.x, RDFS.subClassOf, bnode))  # dropped (bnode object)
    g.add((bnode, RDF.type, OWL.Restriction))  # dropped (bnode subject)
    canon = rl_agreement.canonical_named_closure(g)
    assert (str(EX.x), str(RDF.type), ("iri", str(EX.C))) in canon
    assert len(canon) == 1


def test_canonical_drops_rl_tautologies() -> None:
    g = Graph()
    g.add((EX.C, RDFS.subClassOf, EX.C))  # reflexive scm-cls
    g.add((EX.C, RDFS.subClassOf, OWL.Thing))  # C ⊑ owl:Thing
    g.add((EX.x, RDF.type, OWL.Thing))  # cls-thing
    g.add((EX.x, OWL.sameAs, EX.x))  # eq-ref
    g.add((EX.p, RDFS.domain, OWL.Thing))  # domain owl:Thing
    g.add((EX.a, EX.p, EX.b))  # kept (a real assertion)
    canon = rl_agreement.canonical_named_closure(g)
    assert canon == {(str(EX.a), str(EX.p), ("iri", str(EX.b)))}


def test_canonical_normalizes_integer_value_space() -> None:
    # xsd:nonNegativeInteger "4" and xsd:integer "4" canonicalize identically.
    g_nni = Graph()
    g_nni.add((EX.x, EX.n, Literal("4", datatype=URIRef(f"{XSD}nonNegativeInteger"))))
    g_int = Graph()
    g_int.add((EX.x, EX.n, Literal("4", datatype=URIRef(f"{XSD}integer"))))
    assert rl_agreement.canonical_named_closure(
        g_nni
    ) == rl_agreement.canonical_named_closure(g_int)


def test_enforce_passes_on_agreement_fails_on_divergence() -> None:
    agree = {"native_only": [], "oracle_only": [], "agree": 5}
    assert rl_agreement.enforce(agree) is True

    row = ("s", "p", ("iri", "o"))
    native_div = {"native_only": [row], "oracle_only": [], "agree": 5}
    assert rl_agreement.enforce(native_div) is False

    oracle_div = {"native_only": [], "oracle_only": [row], "agree": 5}
    assert rl_agreement.enforce(oracle_div) is False


def test_build_report_marks_divergence_rows_as_errors() -> None:
    result = {
        "agree": 1,
        "native_only": [("s1", "p1", ("iri", "o1"))],
        "oracle_only": [("s2", "p2", ("lit", "v", "None", "None"))],
        "native_seconds": 0.1,
        "owlrl_seconds": 0.2,
    }
    report = rl_agreement.build_report(result)
    errors = [f for f in report.findings if f["severity"] == "error"]
    assert len(errors) == 2
    assert all(f["code"] == rl_agreement.RULE_RL_DIVERGENCE for f in errors)


# --------------------------------------------------------------------------- #
# End-to-end lane enforcement (Docker-free but heavy: native RL + owlrl over the
# real told facts) — lane-only, never required for normal repo use.
# --------------------------------------------------------------------------- #


@pytest.mark.classic_cross_check
def test_rl_agreement_lane_enforces_over_the_real_bundle() -> None:
    """Native RL ≡ owlrl RL over the real told facts → the axis PASSES."""
    passed, result, _report = rl_agreement.run()
    native_only = cast("list[object]", result["native_only"])
    oracle_only = cast("list[object]", result["oracle_only"])
    assert passed, (
        "native RL and owlrl RL must agree on the canonicalized named-vocabulary "
        f"closure; native_only={native_only[:5]} oracle_only={oracle_only[:5]}"
    )
    assert cast("int", result["agree"]) > 0


@pytest.mark.classic_cross_check
def test_rl_agreement_lane_fails_on_a_synthetic_divergence(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A synthetic native-only triple must FAIL the enforced axis (strict, no knob)."""
    real_compare = rl_agreement.compare

    def fake_compare(gts: Path = GTS_SNAPSHOT_FILE) -> dict[str, object]:
        result = dict(real_compare(gts))
        native_only = list(cast("list[object]", result["native_only"]))
        native_only.append(
            (
                "https://example.org/x",
                str(RDF.type),
                ("iri", "https://example.org/Bogus"),
            )
        )
        result["native_only"] = native_only
        return result

    monkeypatch.setattr(rl_agreement, "compare", fake_compare)
    passed, result, _report = rl_agreement.run()
    assert passed is False
    assert any(
        "Bogus" in str(row) for row in cast("list[object]", result["native_only"])
    )
