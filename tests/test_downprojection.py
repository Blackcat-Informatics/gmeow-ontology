# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Down-projection hardening invariants (#452).

The maximal down-projection must be valid AS-IS to every consumer parser and
truthfully *derived* from GMEOW: co-typing on the same node, representational
fan-out, regeneration (never source-verbatim), BCP-47 consumer tiers, and tier
discipline (provenance only in the canonical forms).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Literal, URIRef

from gmeow_tools.transform import transform
from gmeow_tools.transpile import transpile

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
FOAF = "http://xmlns.com/foaf/0.1/"
ORG = "http://www.w3.org/ns/org#"
REIFIES = URIRef("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies")

_ORG_SOURCE = f"""
@prefix schema: <{SCHEMA}> .
@prefix ex: <https://ex.org/> .
ex:acme a schema:Organization ; schema:name "Acme" .
"""

_DATE_SOURCE = f"""
@prefix schema: <{SCHEMA}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://ex.org/> .
ex:doc a schema:CreativeWork ; schema:name "Doc" ;
    schema:datePublished "2020-01-15"^^xsd:date .
"""


def _maximal(source: str, tmp_path: Path, profiles: list[str]) -> Graph:
    src = tmp_path / "s.ttl"
    src.write_text(source, encoding="utf-8")
    transpile(src, out_dir=tmp_path / "out", profiles=profiles)
    return Graph().parse(tmp_path / "out" / "index.ttl", format="turtle")


def test_co_typing_on_the_same_node(tmp_path: Path) -> None:
    """A gmeow:Organization node emits each vocab's Organization type on the SAME
    @id — no minted-separate nodes, no identity fragmentation."""
    index = _maximal(_ORG_SOURCE, tmp_path, ["schema-org", "foaf", "org"])
    acme = URIRef("https://ex.org/acme")
    types = {str(o) for o in index.objects(acme, RDF.type)}
    # the GMEOW type and every projected vocab type, all on one node
    assert GM + "Organization" in types
    assert SCHEMA + "Organization" in types
    assert FOAF + "Organization" in types
    assert ORG + "FormalOrganization" in types


def test_representational_fan_out(tmp_path: Path) -> None:
    """One flat source value fans out to every applicable target encoding — a
    date reaches ≥2 distinct temporal predicates in the output."""
    index = _maximal(_DATE_SOURCE, tmp_path, ["schema-org", "dcterms", "oai_dc"])
    doc = URIRef("https://ex.org/doc")
    date_preds = {
        str(p) for p, o in index.predicate_objects(doc) if "2020-01-15" in str(o)
    }
    assert len(date_preds) >= 2, f"expected fan-out, got {date_preds}"
    assert any(p.startswith(SCHEMA) for p in date_preds)  # schema:datePublished
    assert any("/dc/" in p for p in date_preds)  # a Dublin Core date


def test_regenerated_from_gmeow_not_source(tmp_path: Path) -> None:
    """The maximal's consumer triples are produced by down-projecting the
    pure-GMEOW intermediate, not copied from the source: a schema.org-only source
    yields FOAF + org triples (vocabularies the source never carried)."""
    index = _maximal(_ORG_SOURCE, tmp_path, ["schema-org", "foaf", "org"])
    preds_and_types = {str(p) for p in index.predicates()} | {
        str(o) for o in index.objects(None, RDF.type) if isinstance(o, URIRef)
    }
    assert any(t.startswith(FOAF) for t in preds_and_types), "FOAF not regenerated"
    assert any(t.startswith(ORG) for t in preds_and_types), "org not regenerated"


def test_consumer_tiers_are_bcp47_canonical_keeps_internal(tmp_path: Path) -> None:
    """The consumer tiers (.ttl/.jsonld/.nt) carry public BCP-47 tags; the
    canonical .nq keeps the internal x-gmeow-* tags (round-trip fidelity)."""
    abox = tmp_path / "g.ttl"
    abox.write_text(
        f"@prefix gmeow: <{GM}> . @prefix ex: <https://ex.org/> .\n"
        f'ex:o a gmeow:Organization ; gmeow:fullName "Acme"@x-gmeow-english .',
        encoding="utf-8",
    )
    transform(abox, out_dir=tmp_path / "out", profiles=["schema-org"])
    out = tmp_path / "out"
    # parse each consumer tier and assert no LITERAL carries an internal tag
    # (robust to any "x-gmeow" substring that might appear in an IRI)
    tiers = (("index.ttl", "turtle"), ("index.jsonld", "json-ld"), ("index.nt", "nt"))
    for tier, fmt in tiers:
        for _s, _p, o in Graph().parse(out / tier, format=fmt):
            if isinstance(o, Literal) and o.language:
                assert not o.language.startswith("x-gmeow"), f"{tier}: {o.language}"
    # the canonical N-Quads keep the internal tag (the source of truth)
    import gmeow_rdf

    store = gmeow_rdf.Store()
    store.bulk_load((out / "index.nq").read_bytes(), format=gmeow_rdf.RdfFormat.N_QUADS)
    internal = {
        q.object.language
        for q in store
        if isinstance(q.object, gmeow_rdf.Literal) and q.object.language
    }
    assert any(lang.startswith("x-gmeow") for lang in internal)


def test_tier_discipline_no_reifiers_in_consumer_tiers(tmp_path: Path) -> None:
    """Provenance reifiers live only in the canonical .gts/.nq; the consumer
    tiers are clean asserted triples with no rdf:reifies noise."""
    src = tmp_path / "s.ttl"
    src.write_text(_ORG_SOURCE, encoding="utf-8")
    transpile(src, out_dir=tmp_path / "out", profiles=["schema-org"])
    out = tmp_path / "out"
    index = Graph().parse(out / "index.ttl", format="turtle")
    assert (None, REIFIES, None) not in index  # no reifiers in the readable tier
    assert "reifies" in (out / "index.nq").read_text(encoding="utf-8")  # but in .nq
