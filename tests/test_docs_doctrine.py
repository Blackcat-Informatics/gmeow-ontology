# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Docs ship WITH the ontology (#325) — the three-tier doctrine, gated.

Tier 1: term docs canonical in the graph (markdown datatype, pairsWith,
graded advisory/example depth). Tier 2: guides structurally bound (anchor
lint; stub = error). Tier 3 + packaging: guides ride the GTS snapshot as
content-addressed blobs linked via guideBlob, and the build fails when a
guide anchors a missing term (docs-in-sync as a build invariant, P7).
"""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, RDFS, Graph, Literal, Namespace, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.describe import build_card, render_card, resolve_term
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.language_tags import load_tag_map, resolve_lang_input
from gmeow_tools.validate import guide_anchor_lint, structural_lint

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_ontology_docs_inputs_track_every_rendered_example() -> None:
    """Every file the docs render verbatim must be a declared docs input.

    The ontology docs fold into the GTS bundle (#bundle), and the drift gate
    skips regeneration when ``ontology_docs_inputs()`` hashes unchanged. So a
    file rendered into the docs but absent from that input list is silent
    staleness: its content changes, the input hash does not, the committed
    snapshot is never rebuilt. This exact hole — slice ``examples/*.ttl``, which
    ``_collect_examples`` renders verbatim, missing from the input list — left
    ``generated/dist/gmeow.gts`` stale after new examples merged. The
    millisecond early-warning for the 4-minute snapshot reproduction test.
    """
    from gmeow_tools import ontology_docs as ontology_docs_mod
    from gmeow_tools.config import SLICES_DIR
    from gmeow_tools.ontology_docs import ontology_docs_inputs

    declared = {p.resolve() for p in ontology_docs_inputs()}

    # The vendored stylesheet write_simple_css() copies verbatim into every page.
    rendered_css = (
        Path(ontology_docs_mod.__file__).with_name("assets") / "simple.css"
    ).resolve()
    assert rendered_css in declared, (
        "assets/simple.css is copied verbatim into every doc page (and thus the "
        "GTS bundle) but is absent from ontology_docs_inputs() — the drift gate "
        "cannot see it change, so the committed snapshot goes silently stale."
    )

    rendered_examples = {p.resolve() for p in SLICES_DIR.glob("*/*/examples/*.ttl")}
    assert rendered_examples, "no slice example files discovered"
    missing = sorted(
        p.relative_to(SLICES_DIR.parent).as_posix()
        for p in rendered_examples - declared
    )
    assert not missing, (
        "slice example files are rendered into the docs (and thus the GTS "
        "bundle) but absent from ontology_docs_inputs() — the drift gate cannot "
        "see them change, so the committed snapshot goes silently stale:\n  "
        + "\n  ".join(missing)
    )


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


def test_advisory_properties_are_annotation_only() -> None:
    """WHEN/HOW/WHERE advice is machine-visible documentation, not logic."""
    g = _graph()
    for prop in (
        GM.useWhen,
        GM.avoidWhen,
        GM.howToUse,
        GM.useForConsumer,
        GM.avoidForConsumer,
    ):
        assert (prop, RDF.type, OWL.AnnotationProperty) in g
        assert (prop, RDFS.label, None) in g
        assert (prop, SKOS.definition, None) in g
        assert (prop, RDFS.isDefinedBy, None) in g
    assert (GM.useWhen, RDFS.subPropertyOf, SKOS.scopeNote) in g
    assert (GM.avoidWhen, RDFS.subPropertyOf, SKOS.scopeNote) in g
    assert (GM.useForConsumer, RDFS.range, GM.ProjectionContext) in g
    assert (GM.avoidForConsumer, RDFS.range, GM.ProjectionContext) in g


def test_graded_gate_warns_not_errors_on_missing_depth() -> None:
    """Missing advisory/example content on core public-facing terms is a WARNING
    (incremental coverage), never an error — and the base #221 trio stays
    an error.

    Exercised against a synthetic probe (a core-slice class with the #221 trio
    but no advisory depth) rather than live coverage gaps: once every core term
    carries its advisory depth the live graph reports zero Tier-1 warnings, so
    the invariant — missing depth *warns*, never *errors* — must be pinned with a
    controlled input that always has a gap."""
    result = structural_lint(_tier_1_probe_graph())
    depth = [w for w in result.warnings if "Tier-1 depth" in w]
    assert depth, "graded gate should report a missing-depth gap as a warning"
    assert not any("Tier-1 depth" in e for e in result.errors)
    assert any("gmeow:useWhen" in w for w in depth)
    assert any("gmeow:howToUse" in w for w in depth)
    # The base #221 trio (label/definition/isDefinedBy) is present on the probe,
    # so depth omissions never escalate to errors.
    assert not any("Tier-1 depth" in e for e in result.errors)


def _tier_1_probe_graph(*, how_to_use: bool = False) -> Graph:
    graph = Graph()
    term = GM.ReviewDepthProbe
    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("review depth probe", lang="x-gmeow-english")))
    graph.add(
        (
            term,
            SKOS.definition,
            Literal(
                "Synthetic class used to exercise advisory warnings.",
                lang="x-gmeow-english",
            ),
        )
    )
    graph.add((term, RDFS.isDefinedBy, URIRef(GMEOW + "slices/kernel")))
    if how_to_use:
        graph.add(
            (
                term,
                GM.howToUse,
                Literal(
                    "Use as a focused validator regression fixture.",
                    lang="x-gmeow-english",
                ),
            )
        )
    return graph


def test_graded_gate_keeps_how_to_use_and_example_warnings_distinct() -> None:
    missing_how = structural_lint(_tier_1_probe_graph()).warnings
    assert any("missing gmeow:howToUse" in warning for warning in missing_how)
    assert not any("skos:example" in warning for warning in missing_how)

    missing_example = structural_lint(_tier_1_probe_graph(how_to_use=True)).warnings
    assert any(
        "has gmeow:howToUse but no skos:example" in warning
        for warning in missing_example
    )


def test_advisory_consumer_values_must_resolve() -> None:
    g = _graph()
    g.add((GM.hasName, GM.useForConsumer, GM.NoSuchConsumer))
    result = structural_lint(g)
    assert any("non-ProjectionContext" in e for e in result.errors)


def test_module_matrix_reports_advisory_coverage() -> None:
    from gmeow_tools.matrix import render_matrix

    text = render_matrix()
    assert "advisory coverage:" in text
    names_row = next(line for line in text.splitlines() if line.startswith("| names |"))
    assert "core" in names_row
    assert "/" in names_row.split("|")[-3]


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


def test_anchor_lint_rejects_wrong_heading_depth(tmp_path: Path) -> None:
    """`## gmeow:X` / `#### gmeow:X` are malformed anchors, not invisible
    ones — the canonical Tier-2 shape is exactly `### gmeow:Term`."""
    _fake_slice(tmp_path, "shallow", "# G\n\n## gmeow:Goal\n\nProse.\n")
    _fake_slice(tmp_path, "deep", "# G\n\n### gmeow:Goal\n\n#### gmeow:Event\n\nP.\n")
    result = guide_anchor_lint(_graph(), root=tmp_path)
    errors = "\n".join(result.errors)
    assert "shallow" in errors and "wrong" in errors and "heading depth" in errors
    assert "deep" in errors and "gmeow:Event" in errors


def test_repo_guides_are_stub_free_and_bound() -> None:
    """The uplift is complete: zero stubs, every anchor resolves."""
    result = guide_anchor_lint(_graph())
    assert result.ok, "\n".join(result.errors[:10])


# --------------------------------------------------------------------------- #
# gmeow describe
# --------------------------------------------------------------------------- #


def test_describe_resolves_and_renders_the_target_shape() -> None:
    g = _graph()
    term, candidates = resolve_term(g, "hasName")
    assert term == GM.hasName and not candidates
    tag_map = load_tag_map(g)
    selector = resolve_lang_input(None, tag_map)
    card = build_card(g, term, selector=selector, tag_map=tag_map)
    assert card.slice_name == "names"
    assert card.slice_tier == "core"
    assert "gmeow:NameUsage" in card.pairs_with
    assert card.use_when and card.how_to_use
    text = render_card(card)
    assert "Use when" in text
    assert "How" in text
    assert "Use for" in text
    assert "Pairs with" in text
    assert "Guide: slices/core/names/docs.md" in text


def test_describe_suggests_on_ambiguity() -> None:
    g = _graph()
    term, candidates = resolve_term(g, "narrat")
    assert term is None
    assert any(c.startswith("Narrat") or c.startswith("narrat") for c in candidates)


def test_resolve_term_rejects_empty_queries() -> None:
    """An empty query must not prefix-match the whole graph."""
    g = _graph()
    for query in ("", "   ", "gmeow:"):
        term, candidates = resolve_term(g, query)
        assert term is None and candidates == []


def test_describe_fails_gracefully_on_missing_gts(tmp_path: Path) -> None:
    from gmeow_tools.describe import describe

    text, code = describe("hasGoal", gts_path=tmp_path / "nope.gts")
    assert code == 1
    assert "not found" in text


# --------------------------------------------------------------------------- #
# GTS packaging — blobs + the docs-in-sync invariant
# --------------------------------------------------------------------------- #


def test_compile_gts_embeds_doc_blobs_round_trip() -> None:
    from blake3 import blake3
    from gts import read, to_nquads
    from rdflib import Literal

    from gmeow_tools.gts_producer import compile_gts

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


def test_snapshot_generator_tracks_gts_codec_inputs() -> None:
    """Codec/writer changes must invalidate the committed GTS snapshot cache."""
    from gmeow_tools import gts_gen
    from gmeow_tools.config import PROJECT_ROOT

    generator = gts_gen.GtsSnapshotGenerator  # @register left an instance here
    paths = {
        path.resolve().relative_to(PROJECT_ROOT).as_posix()
        for path in generator.implementation_paths  # type: ignore[attr-defined]
    }

    # The GTS engine is the external gmeow-gts package; its pinned version (and
    # thus any codec/writer/wire change) is tracked through uv.lock, not by
    # hashing files inside site-packages.
    assert "uv.lock" in paths
    assert "src/gmeow_tools/config.py" in paths
    assert "src/gmeow_tools/graph.py" in paths
    assert "src/gmeow_tools/gts_producer.py" in paths
    assert "src/gmeow_tools/ontology_docs.py" in paths
    assert "src/gmeow_tools/self_desc.py" in paths
    assert "src/gmeow_tools/slices.py" in paths
    assert "src/gmeow_tools/transform.py" in paths
    assert "src/gmeow_tools/validate.py" in paths


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
