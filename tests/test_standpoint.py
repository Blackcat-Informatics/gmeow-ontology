"""The standpoint / contested-claims facility (#43).

GMEOW dissolves the edit war by refusing a single winning slot: a contested fact
is several standpoint-indexed claims that COEXIST, none privileged. This module
pins that as a structural + behavioural invariant -- the three axes (standpoint
perpendicular to source perpendicular to confidence) are orthogonal, the two
clocks (fact-time perpendicular to standpoint-time) stay apart, coexistence is
SHACL-clean and reasoning-safe. The facility realises Standpoint Logic
(box/diamond = gmeow:standpointModality, the subset poset = gmeow:sharpens, * =
gmeow:universalStandpoint). See slices/core/standpoint/module.ttl.

Asserted-TBox invariants whose ASK subjects are all local to the standpoint
module have been migrated to slices/core/standpoint/tests/structural.ttl
(#867). Retained here: dynamic-set sweeps, whole-graph guards, bnode-list
walks, run_shacl ExampleConformance calls, .rq projection checks, DSL checks,
load_mappings SSSOM checks, and filesystem existence checks.
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, SKOS, Graph, Namespace, URIRef

from gmeow_tools.config import PROJECTION_QUERY_DIR, STATEMENT_RDF12_FILE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.statement_dsl import (
    load_statement_dsl,
)
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX = Namespace("https://example.org/shapes/")
SHAPES_FIXTURES = Path(__file__).parent / "fixtures" / "shapes"
_PROJ_Q = PROJECTION_QUERY_DIR
STANDPOINT_OWL2_QUERY = _PROJ_Q / "standpoint-owl2.rq"
STANDPOINT_CRMINF_QUERY = _PROJ_Q / "standpoint-crminf.rq"
STANDPOINT_PROV_QUERY = _PROJ_Q / "standpoint-prov.rq"
STANDPOINT_OA_QUERY = _PROJ_Q / "standpoint-oa.rq"
STANDPOINT_SCHEMA_QUERY = _PROJ_Q / "standpoint-schema.rq"
STANDPOINT_BBC_QUERY = _PROJ_Q / "standpoint-bbc.rq"
CRMINF = Namespace("http://www.ics.forth.gr/isl/CRMinf/")
CRM = Namespace("http://www.cidoc-crm.org/cidoc-crm/")
PROV = Namespace("http://www.w3.org/ns/prov#")
OA = Namespace("http://www.w3.org/ns/oa#")
DCTERMS = Namespace("http://purl.org/dc/terms/")
SCHEMA = Namespace("https://schema.org/")
STANDPOINT_LABEL = URIRef("https://blackcatinformatics.ca/gmeow#standpointLabel")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    return Graph().parse(SHAPES_FIXTURES / f"{name}.ttl", format="turtle")


# --------------------------------------------------------------------------- #
# Term-level structure (the merged ontology graph)
# --------------------------------------------------------------------------- #
# NOTE: accordingTo/standpointModality AnnotationProperty, Standpoint class
# hierarchy, sharpens transitivity, universalStandpoint individual,
# standpointClaim property shape, and claimModality property shape have been
# migrated to slices/core/standpoint/tests/structural.ttl (#867, cells 1-14).
# --------------------------------------------------------------------------- #


def test_modality_value_vocab_spans_belief_values() -> None:
    """gmeow:StandpointModality is the belief-value axis — at least as expressive
    as both the Standpoint-Logic □/◊ AND the CRMinf belief value
    (true/probable/possible/false). Refuted (denial) is the term that makes GMEOW
    ≥ CRMinf: a standpoint can hold a proposition FALSE, not merely be silent."""
    g = _graph()
    members = set(g.subjects(RDF.type, GM.StandpointModality))
    assert members == {
        GM.unequivocal,
        GM.probable,
        GM.conceivable,
        GM.refuted,
        GM.bullshit,
    }


def test_three_axes_are_orthogonal() -> None:
    """standpoint ⟂ source ⟂ confidence: no inferential bridge among accordingTo,
    wasAttributedTo, confidence (mirrors test_identity_orthogonality)."""
    g = _graph()
    axes = [GM.accordingTo, GM.wasAttributedTo, GM.confidence]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_vantage_semantically_subsumes_according_to() -> None:
    """gmeow:vantage ⊑ gmeow:accordingTo is documented on the TBox (#68).
    Direct rdfs:subPropertyOf axiomatisation is impossible because accordingTo
    is an AnnotationProperty (DL-clean, Principle 3) while vantage is an
    ObjectProperty; the subsumption is documented, not reasoned."""
    g = _graph()
    scope_note = g.value(GM.vantage, SKOS.scopeNote)
    assert scope_note is not None
    text = str(scope_note)
    assert "gmeow:vantage ⊑ gmeow:accordingTo" in text, (
        f"vantage scopeNote must document the semantic subsumption: {text}"
    )
    assert "not axiomatised" in text or "not axiomatized" in text


def test_vantage_recognises_observer_as_standpoint() -> None:
    """The vantage agent — observer, sensor, perceiver — IS a standpoint (#68)."""
    g = _graph()
    definition = g.value(GM.vantage, SKOS.definition)
    assert definition is not None
    text = str(definition)
    assert "observer" in text and "sensor" in text and "perceiver" in text, (
        f"vantage definition must name observer/sensor/perceiver as standpoint: {text}"
    )
    assert "IS a standpoint" in text or "is a standpoint" in text, (
        f"vantage definition must assert the observer-as-standpoint doctrine: {text}"
    )


def test_according_to_references_vantage_as_reified_counterpart() -> None:
    """accordingTo definition references vantage as its reified counterpart (#68)."""
    g = _graph()
    definition = g.value(GM.accordingTo, SKOS.definition)
    assert definition is not None
    text = str(definition)
    assert "vantage" in text, f"accordingTo definition must reference vantage: {text}"
    assert "accordingTo becomes the gmeow:vantage" in text, (
        f"accordingTo definition must document the promotion path: {text}"
    )


def test_no_preferred_or_primary_term_is_declared() -> None:
    """No GMEOW vocabulary term is a preferred/primary selector — there is no single
    slot to win (Principle 9). The term-absence twin of the SHACL + lint guards."""
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"


def test_contested_places_cannot_force_inconsistency() -> None:
    """Coexistence is reasoning-safe: contested containment can't make the reasoned
    graph inconsistent, because containedInPlace is not functional and places are
    not declared pairwise-disjoint. (The full ELK proof is the make reason gate.)"""
    g = _graph()
    assert (GM.containedInPlace, RDF.type, OWL.FunctionalProperty) not in g
    assert (GM.Place, RDF.type, OWL.Class) in g
    assert (GM.Place, OWL.disjointWith, GM.Place) not in g


# --------------------------------------------------------------------------- #
# Statement-DSL cells (the RDF-1.2 lead) + lint
# --------------------------------------------------------------------------- #


def test_crimea_pair_coexists_in_the_dsl() -> None:
    """Both Crimea claims are authored, each standpoint-indexed, neither privileged."""
    cells = {c.iri: c for c in load_statement_dsl().cells}
    ru = cells[URIRef(GMEOW + "examples/claim-crimea-in-russia-per-ru")]
    un = cells[URIRef(GMEOW + "examples/claim-crimea-in-ukraine-per-un")]
    # Same subject + predicate, contradictory objects — both retained.
    assert ru.triple.subject == un.triple.subject
    assert ru.triple.predicate == un.triple.predicate == GM.containedInPlace
    assert ru.triple.obj != un.triple.obj
    # Each carries an accordingTo standpoint annotation.
    for cell in (ru, un):
        assert any(a.prop == GM.accordingTo for a in cell.annotations)


def test_two_clocks_stay_distinct() -> None:
    """The two-clock cell keeps fact-time (validFrom, 1850s) and standpoint/
    observation-time (assertedAt, 2025) apart — they never collapse."""
    cells = {c.iri: c for c in load_statement_dsl().cells}
    iri = URIRef(GMEOW + "examples/claim-territory-1850-per-2025-historiography")
    ann = {a.prop: a.value for a in cells[iri].annotations}
    assert str(ann[GM.validFrom]).startswith("1850")
    assert str(ann[GM.assertedAt]).startswith("2025")
    assert ann[GM.validFrom] != ann[GM.assertedAt]
    # And it records the standpoint's modal force as conceivable (◊), not settled.
    assert ann[GM.standpointModality] == GM.conceivable


def test_rdf12_artifact_carries_the_standpoint_axis() -> None:
    """The committed RDF-1.2 lead artifact actually serialises gmeow:accordingTo
    (the cells round-trip through the compiler — full isomorphism is covered by the
    Jena gate in test_statements.py)."""
    text = STATEMENT_RDF12_FILE.read_text(encoding="utf-8")
    assert "accordingTo" in text
    assert "standpointModality" in text


# --------------------------------------------------------------------------- #
# SHACL data shapes
# --------------------------------------------------------------------------- #


def test_coexistence_fixture_conforms() -> None:
    """Contradictory standpoint-indexed claims COEXIST with no violation (the
    centerpiece) — and both objects are retained."""
    g = _fixture("standpoint-coexistence")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    objs = set(g.objects(EX.crimea, GM.containedInPlace))
    assert {EX.russia, EX.ukraine} <= objs  # both retained, neither privileged


def test_preferred_claim_is_flagged() -> None:
    result = run_shacl(_fixture("standpoint-preferred-violation"))
    assert not result.ok
    assert any("preferred/primary" in e for e in result.errors), result.errors


def test_withdrawn_standpoint_warning_does_not_fail() -> None:
    """A withdrawn (closed-interval) tenure without gmeow:displayable false warns,
    but does not hard-fail (Principle 10 — suppression, never erasure)."""
    result = run_shacl(_fixture("standpoint-withdrawn-warning"))
    assert result.ok, f"warning-only graph must pass; errors: {result.errors}"
    assert any("displayable false" in w for w in result.warnings), result.warnings


# --------------------------------------------------------------------------- #
# The standpoint projection — LOSSLESS only; no winner-selecting variant exists
# --------------------------------------------------------------------------- #


def test_no_frame_collapsing_projection_exists() -> None:
    """There is NO down-projection that selects one standpoint. Collapsing a
    contested fact to a chosen frame would re-create the single winning slot the
    facility abolishes (Principle 9) — so only the lossless projection ships."""
    assert STANDPOINT_OWL2_QUERY.exists()
    selecting = STANDPOINT_OWL2_QUERY.parent / "standpoint.rq"
    assert not selecting.exists(), "a frame-selecting projection picks a winner"


def test_standpoint_owl2_projection_emits_tool_compatible_labels() -> None:
    """The lossless Standpoint-OWL 2 projection re-expresses accordingTo +
    standpointModality as the cl-tud/standpoint-owl2 standpointLabel encoding:
    Box for unequivocal (□), Diamond for conceivable (◊), the standpoint name
    carried, and the property IRI ending in #standpointLabel (the tool's
    matching convention)."""
    ex = Namespace("https://blackcatinformatics.ca/gmeow/examples/standpoint/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint.ttl", format="turtle")
    out = data.query(STANDPOINT_OWL2_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert str(STANDPOINT_LABEL).endswith("#standpointLabel")  # tool convention
    labels = {s: str(o) for s, _, o in out.triples((None, STANDPOINT_LABEL, None))}
    # RU claim is held conceivable (◊ → Diamond); UN claim unequivocal (□ → Box).
    assert "<Diamond>" in labels[ex["ax-ru"]]
    assert "standpoint-ru" in labels[ex["ax-ru"]]
    assert "<Box>" in labels[ex["ax-un"]]
    # The base axiom is preserved alongside the standpoint label (lossless).
    assert (ex.crimea, GM.containedInPlace, ex.russia) in out


def test_crminf_projection_is_at_least_as_expressive() -> None:
    """The CRMinf projection re-expresses every claim as I1 Argumentation / I2
    Belief / I4 Proposition Set with an explicit J5-holds-to-be belief value —
    true/possible/false — so a standpoint's DENIAL is carried first-class (GMEOW
    ≥ CRMinf) and the (refuted) proposition is referred to, never asserted."""
    ex = Namespace("https://blackcatinformatics.ca/gmeow/examples/standpoint/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint.ttl", format="turtle")
    out = data.query(STANDPOINT_CRMINF_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    # The argumentation structure is present and attributed to the standpoint actor.
    assert set(out.subjects(RDF.type, CRMINF.I1_Argumentation))
    assert set(out.subjects(RDF.type, CRMINF.I2_Belief))
    assert ex["standpoint-intl-law"] in set(out.objects(None, CRM.P14_carried_out_by))

    # Belief values span the space — the refuted claim holds the proposition FALSE.
    values = {str(o) for o in out.objects(None, CRMINF.J5_holds_to_be)}
    assert {"true", "possible", "false"} <= values

    # The denied proposition is REFERRED TO, never asserted as a base fact.
    assert (ex.crimea, GM.containedInPlace, ex.russia) not in out
    assert ex.crimea in set(out.objects(None, CRM.P67_refers_to))


def test_prov_projection_attributes_every_standpoint() -> None:
    """The PROV-O projection makes each reified claim a prov:Entity attributed
    (qualifiedAttribution) to its standpoint agent — every standpoint retained,
    none privileged, and the proposition kept reified (never asserted)."""
    ex = Namespace("https://blackcatinformatics.ca/gmeow/examples/standpoint/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint.ttl", format="turtle")
    out = data.query(STANDPOINT_PROV_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    # Each claim is a prov:Entity attributed to its standpoint; all retained.
    attributed = set(out.objects(None, PROV.wasAttributedTo))
    assert {ex["standpoint-ru"], ex["standpoint-un"], ex["standpoint-intl-law"]} <= (
        attributed
    )
    assert set(out.subjects(RDF.type, PROV.Attribution))  # qualified attribution
    # The proposition stays reified (owl:annotated*), never asserted as a base fact.
    assert (ex.crimea, GM.containedInPlace, ex.russia) not in out
    assert ex.crimea in set(out.objects(None, OWL.annotatedSource))


def test_oa_projection_annotates_each_claim() -> None:
    """The Web Annotation projection makes each reified claim an oa:Annotation —
    creator = the standpoint, target = the subject, body = the reified statement —
    preserving every standpoint and never asserting the proposition."""
    ex = Namespace("https://blackcatinformatics.ca/gmeow/examples/standpoint/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint.ttl", format="turtle")
    out = data.query(STANDPOINT_OA_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert set(out.subjects(RDF.type, OA.Annotation))
    creators = set(out.objects(None, DCTERMS.creator))
    assert {ex["standpoint-ru"], ex["standpoint-un"], ex["standpoint-intl-law"]} <= (
        creators
    )
    assert ex.crimea in set(out.objects(None, OA.hasTarget))
    # Proposition kept reified, never asserted.
    assert (ex.crimea, GM.containedInPlace, ex.russia) not in out


def test_schema_projection_emits_per_standpoint_claims() -> None:
    """The schema.org projection makes each (non-denied) claim a schema:Claim
    authored by its standpoint — per-standpoint claims coexist (no single verdict),
    a denial is excluded (carried by CRMinf), and the base triple is never asserted."""
    ex = Namespace("https://blackcatinformatics.ca/gmeow/examples/standpoint/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint.ttl", format="turtle")
    out = data.query(STANDPOINT_SCHEMA_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert set(out.subjects(RDF.type, SCHEMA.Claim))
    authors = set(out.objects(None, SCHEMA.author))
    # The asserting standpoints appear; the denying one (refuted) is excluded.
    assert {ex["standpoint-ru"], ex["standpoint-un"]} <= authors
    assert ex["standpoint-intl-law"] not in authors
    # No base triple asserted.
    assert (ex.crimea, GM.containedInPlace, ex.russia) not in out


# --------------------------------------------------------------------------- #
# Issue #127 — StandpointClaim as Observation specialization
# --------------------------------------------------------------------------- #
# NOTE: standpointClaim and claimModality property shapes have been migrated
# to slices/core/standpoint/tests/structural.ttl (#867, cells 12-14).
# --------------------------------------------------------------------------- #


def test_standpoint_tenure_generates_claim_restriction() -> None:
    """StandpointTenure has an EL restriction requiring at least one standpointClaim."""
    g = _graph()
    restrictions = list(g.objects(GM.StandpointTenure, RDFS.subClassOf))
    assert any(
        (r, OWL.onProperty, GM.standpointClaim) in g
        and (r, OWL.someValuesFrom, GM.StandpointClaim) in g
        for r in restrictions
    )


# --------------------------------------------------------------------------- #
# Projection competency tests — StandpointClaim individuals (#127)
# --------------------------------------------------------------------------- #


def test_standpoint_crminf_projection_from_standpoint_claim_reified() -> None:
    """Branch B: StandpointClaim with reified-statement observedFeature produces
    the same CRMinf structure as the annotation-form fixture."""
    data = Graph().parse(
        COVERAGE_FIXTURES / "standpoint-claim-reified.ttl", format="turtle"
    )
    out = data.query(STANDPOINT_CRMINF_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert set(out.subjects(RDF.type, CRMINF.I1_Argumentation))
    assert set(out.subjects(RDF.type, CRMINF.I2_Belief))
    values = {str(o) for o in out.objects(None, CRMINF.J5_holds_to_be)}
    assert {"true", "possible", "false"} <= values


def test_standpoint_crminf_projection_from_standpoint_claim_entity() -> None:
    """Branch C: StandpointClaim with generic-entity observedFeature produces
    CRMinf with crm:P67_refers_to pointing to the entity."""
    ex = Namespace("https://example.org/test/")
    data = Graph().parse(
        COVERAGE_FIXTURES / "standpoint-claim-entity.ttl", format="turtle"
    )
    out = data.query(STANDPOINT_CRMINF_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert set(out.subjects(RDF.type, CRMINF.I1_Argumentation))
    assert ex.place1 in set(out.objects(None, CRM.P67_refers_to))


def test_standpoint_schema_projection_from_standpoint_claim_entity() -> None:
    """Branch C: schema projection renders the entity IRI as schema:text."""
    ex = Namespace("https://example.org/test/")
    data = Graph().parse(
        COVERAGE_FIXTURES / "standpoint-claim-entity.ttl", format="turtle"
    )
    out = data.query(STANDPOINT_SCHEMA_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    texts = {str(o) for o in out.objects(None, SCHEMA.text)}
    assert str(ex.place1) in texts


def test_bbc_projection_exists() -> None:
    """The BBC News Ontology projection is generated and ships with the repo."""
    assert STANDPOINT_BBC_QUERY.exists()


def test_bbc_projection_emits_news_event() -> None:
    """A StandpointClaim about an Event produces a bbc:NewsEvent."""
    ex = Namespace("https://example.org/test/")
    bbc_ns = Namespace("http://www.bbc.co.uk/ontologies/news/")
    data = Graph().parse(COVERAGE_FIXTURES / "standpoint-bbc.ttl", format="turtle")
    out = data.query(STANDPOINT_BBC_QUERY.read_text(encoding="utf-8")).graph
    assert out is not None

    assert set(out.subjects(RDF.type, bbc_ns.NewsEvent))
    assert ex.event1 in set(out.objects(None, bbc_ns.about))


# --------------------------------------------------------------------------- #
# Issue #170 — Language variety standpoint coexistence
# --------------------------------------------------------------------------- #


def test_variety_coexistence_fixture_conforms() -> None:
    """Contradictory varietyKind assertions COEXIST with no violation (Principle 9)."""
    g = _fixture("variety-coexistence")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    ex_lang = Namespace("https://example.org/lang/")
    kinds = set(g.objects(ex_lang.scots, GM.varietyKind))
    assert {GM.kindLanguage, GM.kindDialect} <= kinds, (
        f"both varietyKind values must be retained: {kinds}"
    )


# --------------------------------------------------------------------------- #
# Issue #171 — Etymology derivation coexistence
# --------------------------------------------------------------------------- #


def test_etymology_coexistence_fixture_conforms() -> None:
    """Contradictory derivationKind assertions COEXIST with no violation
    (Principle 9)."""
    g = _fixture("etymology-coexistence")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    all_kinds: set[URIRef] = set()
    for deriv in g.subjects(RDF.type, GM.EtymologicalDerivation):
        for kind in g.objects(deriv, GM.derivationKind):
            if isinstance(kind, URIRef):
                all_kinds.add(kind)
    assert {GM.derivationBorrowing, GM.derivationReanalysis} <= all_kinds, (
        f"both derivationKind values must be retained: {all_kinds}"
    )


# --------------------------------------------------------------------------- #
# Mapping alignment tests (#127)
# --------------------------------------------------------------------------- #


def test_standpoint_claim_maps_to_crminf_i5() -> None:
    """SSSOM row exists for StandpointClaim → crminf:I5_Inference_Making."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:StandpointClaim"
        and m.object_id == "crminf:I5_Inference_Making"
    ]
    assert matches, "StandpointClaim must map to crminf:I5_Inference_Making"


def test_standpoint_claim_maps_to_iao_assertion() -> None:
    """SSSOM row exists for StandpointClaim → iao:assertion."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:StandpointClaim" and m.object_id == "iao:assertion"
    ]
    assert matches, "StandpointClaim must map to iao:assertion"


def test_standpoint_claim_maps_to_oa_annotation() -> None:
    """SSSOM row exists for StandpointClaim → oa:Annotation."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:StandpointClaim" and m.object_id == "oa:Annotation"
    ]
    assert matches, "StandpointClaim must map to oa:Annotation"


def test_standpoint_maps_to_iptc_assertor() -> None:
    """SSSOM row exists for Standpoint → iptc:Assertor."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:Standpoint" and m.object_id == "iptc:Assertor"
    ]
    assert matches, "Standpoint must map to iptc:Assertor"


def test_claim_modality_maps_to_sosa_has_result() -> None:
    """SSSOM row exists for claimModality → sosa:hasResult."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:claimModality" and m.object_id == "sosa:hasResult"
    ]
    assert matches, "claimModality must map to sosa:hasResult"
