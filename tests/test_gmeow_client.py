# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""End-to-end tests of the gmeow client (#296, D1, Principle 13).

The quickstart IS the product: these tests run the README flow verbatim,
then verify the claims the pitch makes — reified-claim semantics, P10
supersession with an audit trail, and a transport-verifiable package.
"""

from __future__ import annotations

import time
from pathlib import Path

from gmeow import Claim, Memory
from gts import read


def test_quickstart_flow_verbatim(tmp_path: Path) -> None:
    """The README example, line for line."""
    mem = Memory(tmp_path / "assistant.gts")

    claim = mem.store(
        "Patrick prefers explicit error handling over exceptions-as-flow",
        source="conversation 2026-06-10",
        confidence=0.8,
        according_to="claude-fable-5",
    )
    assert isinstance(claim, Claim)
    assert claim.confidence == 0.8

    hits = mem.recall("error handling preferences", min_confidence=0.5)
    assert hits and hits[0].text.startswith("Patrick prefers explicit")
    assert hits[0].according_to == "claude-fable-5"
    assert hits[0].source == "conversation 2026-06-10"
    assert hits[0].created is not None

    mem.revise(
        claim,
        reason="user stated the opposite for scripts",
        superseded_by=mem.store(
            "For one-off scripts Patrick is fine with exceptions-as-flow",
            confidence=0.9,
            according_to="claude-fable-5",
        ),
    )
    # the superseded claim no longer surfaces...
    texts = [c.text for c in mem.recall("error handling")]
    assert all("prefers explicit" not in t for t in texts)
    # ...but is never deleted (P10): the audit trail is one flag away
    history = mem.recall("error handling", include_suppressed=True)
    assert any(c.suppressed and "prefers explicit" in c.text for c in history)


def test_store_is_a_reified_rdf12_statement(tmp_path: Path) -> None:
    """Under the hood: a quad, a reifier binding, and annotation rows."""
    mem = Memory(tmp_path / "m.gts")
    mem.store("water is wet", confidence=1.0, according_to="everyone")
    g = read((tmp_path / "m.gts").read_bytes())
    assert len(g.quads) == 1
    assert len(g.reifiers) == 1
    # confidence + according_to + created
    assert len(g.annotations) == 3
    assert g.segment_profiles == ["ai-package"]


def test_each_write_is_one_appended_segment(tmp_path: Path) -> None:
    """Persistence IS §3.1 composition: store twice, get two segments."""
    mem = Memory(tmp_path / "m.gts")
    mem.store("first")
    mem.store("second")
    g = read((tmp_path / "m.gts").read_bytes())
    assert len(g.segment_heads) == 2
    assert mem.verify() == []  # chain-clean across the composition


def test_recall_ranks_by_overlap_and_filters_confidence(tmp_path: Path) -> None:
    mem = Memory(tmp_path / "m.gts")
    mem.store("cats are mammals", confidence=0.9)
    mem.store("cats chase red laser dots", confidence=0.4)
    mem.store("the moon is not made of cheese", confidence=0.99)

    hits = mem.recall("cats laser")
    assert hits[0].text == "cats chase red laser dots"

    confident = mem.recall("cats", min_confidence=0.5)
    assert [c.text for c in confident] == ["cats are mammals"]

    recent = mem.recall()  # empty query: most recent first
    assert recent[0].text == "the moon is not made of cheese"


def test_revision_audit_trail_links_successor(tmp_path: Path) -> None:
    mem = Memory(tmp_path / "m.gts")
    old = mem.store("pluto is a planet", confidence=0.9)
    new = mem.store("pluto is a dwarf planet", confidence=0.99)
    mem.revise(old, reason="IAU 2006", superseded_by=new)

    g = read((tmp_path / "m.gts").read_bytes())
    # the derivation annotation exists: successor wasDerivedFrom predecessor
    derived = "https://blackcatinformatics.ca/gmeow/wasDerivedFrom"
    links = [
        (g.terms[r].value, g.terms[v].value)
        for r, p, v in g.annotations
        if g.terms[p].value == derived
    ]
    assert (new.id, old.id) in links
    # and the suppression carries the reason
    assert any(s.reason == "IAU 2006" for s in g.suppressions)


def test_contradicting_standpoints_coexist(tmp_path: Path) -> None:
    """Principle 9: standpoint-indexed contradiction, no overwrite."""
    mem = Memory(tmp_path / "m.gts")
    mem.store("the dress is blue", according_to="team-blue", confidence=0.8)
    mem.store("the dress is gold", according_to="team-gold", confidence=0.8)
    hits = mem.recall("dress")
    assert {c.according_to for c in hits} == {"team-blue", "team-gold"}


def test_empty_and_missing_package(tmp_path: Path) -> None:
    mem = Memory(tmp_path / "never-written.gts")
    assert mem.recall("anything") == []
    assert mem.claims() == []
    assert mem.verify() == []


def test_five_minute_gate_locally(tmp_path: Path) -> None:
    """The P13 gate's in-repo half: the full flow needs no setup steps and
    completes in seconds (the CI clean-venv install half lives in the
    release workflow)."""
    start = time.monotonic()
    mem = Memory(tmp_path / "m.gts")
    c = mem.store("fast", confidence=1.0)
    assert mem.recall("fast")
    mem.revise(c, reason="done")
    assert time.monotonic() - start < 5.0


def test_confidence_is_validated(tmp_path: Path) -> None:
    mem = Memory(tmp_path / "m.gts")
    for bad in (1.5, -0.1, float("nan"), float("inf")):
        try:
            mem.store("x", confidence=bad)
            raise AssertionError(f"accepted {bad}")
        except ValueError:
            pass


def test_projection_keeps_claims_mentioning_quoted_triple_syntax(
    tmp_path: Path,
) -> None:
    """The RDF 1.1 projection drops binding lines, never user text."""
    mem = Memory(tmp_path / "m.gts")
    mem.store("RDF 1.2 writes quoted triples as <<( s p o )>> tokens")
    ds = mem.to_rdflib()
    texts = [str(o) for _, _, o, _ in ds.quads((None, None, None, None))]
    assert any("<<(" in t for t in texts)


def test_rdflib_interop(tmp_path: Path) -> None:
    """gmeow[rdf]: the folded package parses into an rdflib Dataset."""
    mem = Memory(tmp_path / "m.gts")
    mem.store("water is wet", confidence=1.0)
    ds = mem.to_rdflib()
    assert len(list(ds.quads((None, None, None, None)))) >= 1
