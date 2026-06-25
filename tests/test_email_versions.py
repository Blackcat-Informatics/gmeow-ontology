"""Email versioning, variant, and patch-diff guards (issue #136).

Email canonical/variant status is modeled via the cross-cutting VersionMembership
relator from versions.ttl (#161), not as email-specific subclasses. This module
tests the email-specific identity keys, collision flags, fingerprints, and
patch-diff artifact terms.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, XSD, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    """Load the merged ontology graph without imports."""
    return load_merged_graph(include_imports=False)


def _fixture_path() -> str:
    """Return the path to the email coverage fixture."""
    return str(Path(__file__).parent / "fixtures" / "coverage" / "email.ttl")


# --------------------------------------------------------------------------- #
# Class guards
# --------------------------------------------------------------------------- #


def test_no_email_variant_subclasses() -> None:
    """Anti-regression: canonical/variant must be roles, not subclasses (#161)."""
    graph = _graph()
    for banned in ("EmailMessageVariant", "CanonicalEmailMessage"):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.Class) not in graph, f"{banned} must not exist"


# --------------------------------------------------------------------------- #
# Object property guards
# --------------------------------------------------------------------------- #


def test_has_patch_diff_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "hasPatchDiff")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.subPropertyOf, URIRef(GMEOW + "hasBodyPart")) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, URIRef(GMEOW + "EmailPatchDiff")) in graph
    # Non-functional: a message may carry header and body patches separately.
    assert (node, RDF.type, OWL.FunctionalProperty) not in graph


# --------------------------------------------------------------------------- #
# Datatype property guards
# --------------------------------------------------------------------------- #


def test_message_id_generated_boolean_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "messageIdGenerated")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, XSD.boolean) in graph


def test_message_id_collision_boolean_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "messageIdCollision")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, XSD.boolean) in graph


def test_canonical_fingerprint_literal_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "canonicalFingerprint")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_body_line_fingerprint_literal_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "bodyLineFingerprint")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_analysis_scope_literal_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "analysisScope")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


def test_analysis_input_body_line_literal_on_email_message() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "analysisInputBodyLine")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "EmailMessage")) in graph
    assert (node, RDFS.range, RDFS.Literal) in graph


# --------------------------------------------------------------------------- #
# Fixture round-trip: VersionSet + VersionMembership + patch diff
# --------------------------------------------------------------------------- #


def test_fixture_version_memberships_use_roles_not_subclasses() -> None:
    graph = load_merged_graph(include_imports=False)
    graph.parse(_fixture_path(), format="turtle")

    version_set = URIRef("https://example.org/mail/msgVersionSet")
    canonical_msg = URIRef("https://example.org/mail/msgCanonical")
    variant_msg = URIRef("https://example.org/mail/msgVariant")
    role_canonical = URIRef(GMEOW + "roleCanonical")
    role_variant = URIRef(GMEOW + "roleVariant")
    scale_minor = URIRef(GMEOW + "scaleMinor")

    # Both messages participate in the same VersionSet via VersionMembership.
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
    assert (
        patch,
        URIRef(GMEOW + "mediaType"),
        Literal("text/x-gmeow-patch"),
    ) in graph
    assert (
        patch,
        URIRef(GMEOW + "wasDerivedFrom"),
        canonical_body,
    ) in graph
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

    assert (
        canonical_msg,
        URIRef(GMEOW + "messageIdCollision"),
        Literal(True),
    ) in graph
    assert (
        variant_msg,
        URIRef(GMEOW + "messageIdCollision"),
        Literal(True),
    ) in graph
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
    assert (
        variant_msg,
        URIRef(GMEOW + "analysisScope"),
        Literal("body-only"),
    ) in graph
    assert (
        variant_msg,
        URIRef(GMEOW + "analysisInputBodyLine"),
        Literal("Please review the attached Q2 report."),
    ) in graph
