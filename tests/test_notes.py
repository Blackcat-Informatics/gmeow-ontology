"""Structural + DL-safety + SHACL guards for the notes & annotation building block.

Pins gmeow:Note / Annotation / Highlight / Bookmark / Comment hierarchy,
EvidenceSpan generalization, the backlink graph (mentions/mentionedIn/relatedNote),
comment threading (commentParent/hasReply), annotation motivations, and the
projection layer (W3C OA, schema.org, ActivityStreams, markdown wikilinks).
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
EX = Namespace("https://example.org/test/")
_PROJ_Q = Path(__file__).parent.parent / "queries" / "projections"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# =========================================================================== #
# gUFO grounding
# =========================================================================== #


def test_note_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Note"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_annotation_is_relator() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "Annotation"), RDF.type, OWL.Class) in graph
    assert (URIRef(GMEOW + "Annotation"), RDF.type, URIRef(GUFO + "Kind")) in graph
    assert (
        URIRef(GMEOW + "Annotation"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


def test_highlight_is_annotation() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Highlight"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Annotation"),
    ) in graph


def test_bookmark_is_annotation() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Bookmark"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Annotation"),
    ) in graph


def test_comment_is_note() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "Comment"), RDFS.subClassOf, URIRef(GMEOW + "Note")) in graph


def test_evidence_span_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "EvidenceSpan"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_selector_sub_class_of_evidence_span() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Selector"),
        RDFS.subClassOf,
        URIRef(GMEOW + "EvidenceSpan"),
    ) in graph


# =========================================================================== #
# Property typing
# =========================================================================== #


def test_annotation_roles_are_functional() -> None:
    graph = _graph()
    for prop in ("annotationBody", "annotationTarget", "annotationMotivation"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_annotation_target_span_is_non_functional() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "annotationTargetSpan")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_comment_parent_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "commentParent")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    # Not declared FunctionalProperty: hasReply is transitive, and in OWL 2 DL
    # a transitive property (and its inverse) is non-simple and cannot be
    # functional.  The "one parent per comment" constraint is enforced by
    # SHACL (sh:maxCount 1 on commentParent) instead.
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


def test_has_direct_reply_is_inverse_of_comment_parent() -> None:
    graph = _graph()
    direct = URIRef(GMEOW + "hasDirectReply")
    parent = URIRef(GMEOW + "commentParent")
    assert (direct, RDF.type, OWL.ObjectProperty) in graph
    assert (direct, OWL.inverseOf, parent) in graph


def test_has_reply_is_transitive_superproperty() -> None:
    graph = _graph()
    reply = URIRef(GMEOW + "hasReply")
    direct = URIRef(GMEOW + "hasDirectReply")
    assert (reply, RDF.type, OWL.TransitiveProperty) in graph
    assert (direct, RDFS.subPropertyOf, reply) in graph


def test_mentions_inverse_mentioned_in() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "mentionedIn"),
        OWL.inverseOf,
        URIRef(GMEOW + "mentions"),
    ) in graph


def test_related_note_is_symmetric() -> None:
    graph = _graph()
    assert (URIRef(GMEOW + "relatedNote"), RDF.type, OWL.SymmetricProperty) in graph


def test_note_properties_exist() -> None:
    graph = _graph()
    for prop in ("noteContent", "noteCreatedAt", "noteModifiedAt"):
        assert (URIRef(GMEOW + prop), RDF.type, OWL.DatatypeProperty) in graph
    assert (URIRef(GMEOW + "noteAuthor"), RDF.type, OWL.ObjectProperty) in graph


# =========================================================================== #
# Open value vocabulary (individuals, not subclasses)
# =========================================================================== #


def test_motivation_values_are_individuals() -> None:
    graph = _graph()
    motivation_class = URIRef(GMEOW + "AnnotationMotivation")
    assert len(set(graph.subjects(RDF.type, motivation_class))) == 10
    for ind in (
        "motivationDescribing",
        "motivationCommenting",
        "motivationHighlighting",
        "motivationBookmarking",
        "motivationTagging",
        "motivationLinking",
        "motivationQuestioning",
        "motivationReplying",
        "motivationAssessing",
        "motivationModerating",
    ):
        assert (
            URIRef(GMEOW + ind),
            RDF.type,
            motivation_class,
        ) in graph
    for rejected in ("DescribingMotivation", "CommentingMotivation"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


# =========================================================================== #
# Selector reuse — no duplicate model
# =========================================================================== #


def test_no_duplicate_selector_properties() -> None:
    """The selector properties live ONLY on EvidenceSpan/Selector; no second
    selector model is minted for annotations."""
    graph = _graph()
    for banned in (
        "annotationSelectorTextQuote",
        "annotationSelectorTextPosition",
        "annotationSelectorPage",
        "highlightSelector",
    ):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph, (
            f"{banned} duplicates selector model"
        )
        assert (node, RDF.type, OWL.ObjectProperty) not in graph, (
            f"{banned} duplicates selector model"
        )


# =========================================================================== #
# Trichotomy / orthogonality guards
# =========================================================================== #


def test_no_bridge_among_note_axes() -> None:
    """Typing, aboutness, and tagging remain orthogonal for notes."""
    graph = _graph()
    axes = {
        "hasTag": URIRef(GMEOW + "hasTag"),
        "isAbout": URIRef(GMEOW + "isAbout"),
        "mentions": URIRef(GMEOW + "mentions"),
    }
    for a, b in combinations(axes, 2):
        na, nb = axes[a], axes[b]
        assert (na, RDFS.subPropertyOf, nb) not in graph, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in graph, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in graph, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in graph, f"{b} ≡ {a} forbidden"


def test_note_tagging_reuses_has_tag() -> None:
    """Notes use the universal hasTag / Tagging building block; no note-specific
    tag property is minted."""
    graph = _graph()
    assert (URIRef(GMEOW + "noteTag"), RDF.type, OWL.ObjectProperty) not in graph
    assert (URIRef(GMEOW + "noteTagging"), RDF.type, OWL.ObjectProperty) not in graph


# =========================================================================== #
# SHACL instance tests
# =========================================================================== #


def test_note_with_content_passes_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, URIRef(GMEOW + "noteContent"), Literal("A test note.")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_note_with_label_passes_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, RDFS.label, Literal("Test Note")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_note_without_content_or_label_fails_shacl() -> None:
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))

    result = run_shacl(g)
    assert not result.ok
    assert any(
        "note content" in e.lower() or "rdfs:label" in e.lower() for e in result.errors
    )


def test_annotation_without_target_fails_shacl() -> None:
    g = Graph()
    g.add((EX.ann, RDF.type, URIRef(GMEOW + "Annotation")))
    g.add(
        (
            EX.ann,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationCommenting"),
        )
    )

    result = run_shacl(g)
    assert not result.ok
    assert any("annotationtarget" in e.lower() for e in result.errors)


def test_annotation_with_target_passes_shacl() -> None:
    g = Graph()
    g.add((EX.ann, RDF.type, URIRef(GMEOW + "Annotation")))
    g.add((EX.ann, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add(
        (
            EX.ann,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationCommenting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_highlight_without_selector_fails_shacl() -> None:
    g = Graph()
    g.add((EX.hl, RDF.type, URIRef(GMEOW + "Highlight")))
    g.add((EX.hl, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add(
        (
            EX.hl,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationHighlighting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))

    result = run_shacl(g)
    assert not result.ok
    assert any("selector" in e.lower() for e in result.errors)


def test_highlight_with_selector_passes_shacl() -> None:
    g = Graph()
    g.add((EX.hl, RDF.type, URIRef(GMEOW + "Highlight")))
    g.add((EX.hl, URIRef(GMEOW + "annotationTarget"), EX.doc))
    g.add((EX.hl, URIRef(GMEOW + "annotationTargetSpan"), EX.span))
    g.add(
        (
            EX.hl,
            URIRef(GMEOW + "annotationMotivation"),
            URIRef(GMEOW + "motivationHighlighting"),
        )
    )
    g.add((EX.doc, RDF.type, URIRef(GMEOW + "Entity")))
    g.add((EX.span, RDF.type, URIRef(GMEOW + "EvidenceSpan")))
    g.add((EX.span, URIRef(GMEOW + "selectorTextQuote"), Literal("highlighted text")))

    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# =========================================================================== #
# Projection round-trip / parse tests
# =========================================================================== #


def _sparql_parse(path: Path) -> None:
    """Minimal parse guard — the query must be syntactically valid SPARQL."""
    import sys

    from rdflib.plugins.sparql import prepareQuery

    # Large projection CONSTRUCT queries push pyparsing past the default limit.
    old = sys.getrecursionlimit()
    sys.setrecursionlimit(3000)
    try:
        text = path.read_text()
        prepareQuery(text)
    finally:
        sys.setrecursionlimit(old)


def test_notes_oa_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "web-annotation.rq")


def test_notes_schema_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "schema-org.rq")


def test_notes_as_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "activitystreams.rq")


def test_notes_markdown_projection_executable() -> None:
    _sparql_parse(_PROJ_Q / "markdown.rq")


# =========================================================================== #
# Standpoint / suppression guards
# =========================================================================== #


def test_notes_are_standpoint_indexed() -> None:
    """The standpoint machinery (accordingTo) is available on notes via the
    statement/provenance layer; the TBox does not forbid it."""
    g = _graph()
    assert (URIRef(GMEOW + "accordingTo"), RDF.type, OWL.AnnotationProperty) in g


def test_retracted_note_displayable_false() -> None:
    """A retracted note/comment sets displayable false (Principle 10)."""
    g = Graph()
    g.add((EX.note, RDF.type, URIRef(GMEOW + "Note")))
    g.add((EX.note, URIRef(GMEOW + "noteContent"), Literal("A retracted note.")))
    g.add(
        (
            EX.note,
            URIRef(GMEOW + "displayable"),
            Literal(
                "false", datatype=URIRef("http://www.w3.org/2001/XMLSchema#boolean")
            ),
        )
    )

    result = run_shacl(g)
    # displayable false is valid; the shape only warns when a reply's
    # parent is suppressed.
    assert result.ok, "\n".join(result.errors)
