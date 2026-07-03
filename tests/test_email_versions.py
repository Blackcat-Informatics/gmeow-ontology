"""Email versioning, variant, and patch-diff guards.

Email canonical/variant status is modeled via the cross-cutting VersionMembership
relator from versions.ttl, not as email-specific subclasses. This module
tests the email-specific identity keys, collision flags, fingerprints, and
patch-diff artifact terms.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import RDF, Graph, Literal, URIRef
from purrdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def _fixture_path() -> str:
    """Return the path to the email coverage fixture."""
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


def test_fixture_version_memberships_use_roles_not_subclasses() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    version_set = URIRef("https://example.org/mail/msgVersionSet")
    canonical_msg = URIRef("https://example.org/mail/msgCanonical")
    variant_msg = URIRef("https://example.org/mail/msgVariant")
    role_canonical = URIRef(GMEOW + "roleCanonical")
    role_variant = URIRef(GMEOW + "roleVariant")
    scale_minor = URIRef(GMEOW + "scaleMinor")
    q = """
    PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
    SELECT ?membership ?msg ?role ?scale WHERE {
        ?membership a gmeow:VersionMembership ;
                    gmeow:versionSet ?set ;
                    gmeow:versionMember ?msg ;
                    gmeow:versionRole ?role .
        OPTIONAL { ?membership gmeow:versionScale ?scale . }
    }
    """
    results = list(graph.query(q, initBindings={"set": version_set}))
    by_msg: dict[str, dict[str, object]] = {}
    for r in results:
        assert isinstance(r, ResultRow)
        msg = str(r[1])
        by_msg[msg] = {"role": str(r[2]), "scale": str(r[3]) if r[3] else None}
    assert str(canonical_msg) in by_msg
    assert str(variant_msg) in by_msg
    assert by_msg[str(canonical_msg)]["role"] == str(role_canonical)
    assert by_msg[str(variant_msg)]["role"] == str(role_variant)
    assert by_msg[str(variant_msg)]["scale"] == str(scale_minor)


def test_fixture_patch_diff_links_and_digest() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    variant_msg = URIRef("https://example.org/mail/msgVariant")
    patch = URIRef("https://example.org/mail/variantPatch")
    canonical_body = URIRef("https://example.org/mail/msgCanonicalBody")
    assert (variant_msg, URIRef(GMEOW + "hasPatchDiff"), patch) in graph
    assert (patch, RDF.type, URIRef(GMEOW + "EmailPatchDiff")) in graph
    assert (patch, URIRef(GMEOW + "mediaType"), Literal("text/x-gmeow-patch")) in graph
    assert (patch, URIRef(GMEOW + "wasDerivedFrom"), canonical_body) in graph
    assert (
        patch,
        URIRef(GMEOW + "contentDigest"),
        Literal("blake3:patch-bytes"),
    ) in graph


def test_fixture_collision_flags_and_fingerprints() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")
    canonical_msg = URIRef("https://example.org/mail/msgCanonical")
    variant_msg = URIRef("https://example.org/mail/msgVariant")
    assert (canonical_msg, URIRef(GMEOW + "messageIdCollision"), Literal(True)) in graph
    assert (variant_msg, URIRef(GMEOW + "messageIdCollision"), Literal(True)) in graph
    assert (
        canonical_msg,
        URIRef(GMEOW + "canonicalFingerprint"),
        Literal("blake3:canonical-body-hash"),
    ) in graph
    assert (
        variant_msg,
        URIRef(GMEOW + "bodyLineFingerprint"),
        Literal("blake3:variant-body-line-hash"),
    ) in graph
    assert (variant_msg, URIRef(GMEOW + "analysisScope"), Literal("body-only")) in graph
    assert (
        variant_msg,
        URIRef(GMEOW + "analysisInputBodyLine"),
        Literal("Please review the attached Q2 report."),
    ) in graph
