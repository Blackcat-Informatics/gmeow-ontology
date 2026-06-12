# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Docs ship WITH the ontology (#325) — the three-tier doctrine, gated.

Tier 1: term docs canonical in the graph (markdown datatype, pairsWith,
graded scopeNote/example depth). Tier 2: guides structurally bound (anchor
lint; stub = error). Tier 3 + packaging: guides ride the GTS snapshot as
content-addressed blobs linked via guideBlob, and the build fails when a
guide anchors a missing term (docs-in-sync as a build invariant, P7).
"""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, RDFS, Graph, Namespace, URIRef
from rdflib.namespace import OWL

from gmeow_tools.describe import build_card, render_card, resolve_term
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import guide_anchor_lint, structural_lint

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Tier 1 — vocabulary and graded gate
# --------------------------------------------------------------------------- #


def test_markdown_datatype_is_annotation_only() -> None:
    """gmeow:markdown exists as a named datatype and appears in NO logical
    axiom (annotation literals only — the DL profile stays untouched)."""
    g = _graph()
    assert (GM.markdown, RDF.type, RDFS.Datatype) in g
    # Never a property range in logical axioms.
    assert list(g.subjects(RDFS.range, GM.markdown)) == []


def test_pairs_with_is_an_annotation_property() -> None:
    g = _graph()
    assert (GM.pairsWith, RDF.type, OWL.AnnotationProperty) in g
    assert (GM.guideBlob, RDF.type, OWL.AnnotationProperty) in g
    # Seeded pairings: the flat shortcut points at its relator.
    expected = {
        (GM.hasGoal, GM.IntentionTenure),
        (GM.cites, GM.CitationAct),
        (GM.hasTag, GM.Tagging),
        (GM.hasParticipant, GM.Participation),
        (GM.hasName, GM.NameUsage),
        (GM.narrates, GM.NarrationUsage),
        (GM.overrides, GM.PrecedenceTenure),
        (GM.depicts, GM.DepictionUsage),
    }
    actual = set(g.subject_objects(GM.pairsWith))
    assert expected <= actual


def test_graded_gate_warns_not_errors_on_missing_depth() -> None:
    """Missing scopeNote/example on core public-facing terms is a WARNING
    (incremental coverage), never an error — and the base #221 trio stays
    an error."""
    result = structural_lint(_graph())
    depth = [w for w in result.warnings if "Tier-1 depth" in w]
    assert depth, "graded gate should be reporting coverage gaps"
    assert not any("Tier-1 depth" in e for e in result.errors)


# --------------------------------------------------------------------------- #
# Tier 2 — the anchor lint
# --------------------------------------------------------------------------- #


def _fake_slice(tmp_path: Path, name: str, guide: str | None) -> Path:
    slice_dir = tmp_path / "core" / name
    slice_dir.mkdir(parents=True)
    (slice_dir / "manifest.ttl").write_text("# manifest\n", encoding="utf-8")
    if guide is not None:
        (slice_dir / "docs.md").write_text(guide, encoding="utf-8")
    return slice_dir


def test_anchor_lint_accepts_resolving_anchors(tmp_path: Path) -> None:
    _fake_slice(tmp_path, "good", "# Guide\n\n### gmeow:Goal\n\nProse.\n")
    result = guide_anchor_lint(_graph(), root=tmp_path)
    assert result.ok, result.errors


def test_anchor_lint_rejects_renamed_terms(tmp_path: Path) -> None:
    _fake_slice(tmp_path, "bad", "# Guide\n\n### gmeow:NoSuchTermEver\n\nProse.\n")
    result = guide_anchor_lint(_graph(), root=tmp_path)
    assert not result.ok
    assert any("NoSuchTermEver" in e for e in result.errors)


def test_anchor_lint_rejects_stubs_and_missing_guides(tmp_path: Path) -> None:
    _fake_slice(tmp_path, "stubby", "# G\n\n*This is a STUB guide (#325 Tier-2).*\n")
    _fake_slice(tmp_path, "guideless", None)
    result = guide_anchor_lint(_graph(), root=tmp_path)
    errors = "\n".join(result.errors)
    assert "stubby" in errors and "still a stub" in errors
    assert "guideless" in errors and "missing docs.md" in errors


def test_repo_guides_are_stub_free_and_bound() -> None:
    """The uplift is complete: zero stubs, every anchor resolves."""
    result = guide_anchor_lint(_graph())
    assert result.ok, "\n".join(result.errors[:10])


# --------------------------------------------------------------------------- #
# gmeow describe
# --------------------------------------------------------------------------- #


def test_describe_resolves_and_renders_the_target_shape() -> None:
    g = _graph()
    term, candidates = resolve_term(g, "hasGoal")
    assert term == GM.hasGoal and not candidates
    card = build_card(g, term)
    assert card.slice_name == "teleology"
    assert card.slice_tier == "core"
    assert "gmeow:IntentionTenure" in card.pairs_with
    text = render_card(card)
    assert "Pairs with" in text
    assert "Guide: slices/core/teleology/docs.md" in text


def test_describe_suggests_on_ambiguity() -> None:
    g = _graph()
    term, candidates = resolve_term(g, "narrat")
    assert term is None
    assert any(c.startswith("Narrat") or c.startswith("narrat") for c in candidates)


# --------------------------------------------------------------------------- #
# GTS packaging — blobs + the docs-in-sync invariant
# --------------------------------------------------------------------------- #


def test_compile_gts_embeds_doc_blobs_round_trip() -> None:
    from blake3 import blake3
    from rdflib import Literal

    from gmeow_tools.gts_producer import compile_gts
    from gts import read, to_nquads

    payload = b"# A tiny guide\n\nVerified-by-construction prose.\n"
    digest = "blake3:" + blake3(payload).hexdigest()
    g = Graph()
    slice_iri = URIRef(GMEOW + "slices/example")
    g.add((slice_iri, RDF.type, OWL.Ontology))
    g.add((slice_iri, GM.guideBlob, Literal(digest)))
    data = compile_gts(g, None, doc_blobs=[(payload, "text/markdown", "docs:example")])
    package = read(data)
    assert digest.removeprefix("blake3:") in {
        k.removeprefix("blake3:") for k in package.blobs
    }
    key = next(k for k in package.blobs if k.endswith(digest.removeprefix("blake3:")))
    assert package.blobs[key] == payload
    assert digest in to_nquads(package)


def test_snapshot_generator_fails_on_bad_anchor(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The docs-in-sync build invariant (P7): a guide anchoring a missing
    term fails the package build."""
    from gmeow_tools import gts_gen
    from gmeow_tools.validate import ValidationResult

    broken = ValidationResult()
    broken.errors.append("slice x: docs.md anchors gmeow:Ghost (renamed?)")

    import gmeow_tools.validate as validate_mod

    monkeypatch.setattr(validate_mod, "guide_anchor_lint", lambda *a, **k: broken)
    generator = gts_gen.GtsSnapshotGenerator  # @register left an instance here
    with pytest.raises(ValueError, match="docs-in-sync invariant"):
        generator.render(Path("/tmp/never-used-staging"))  # type: ignore[arg-type,call-arg]
