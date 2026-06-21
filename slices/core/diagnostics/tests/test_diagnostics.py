# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Diagnostics slice — structural invariants of the gmeow:Finding vocabulary.

These assertions guard the load-bearing shape of the diagnostics slice (#654):
``gmeow:Finding`` as a real ``rdfs:subClassOf`` ``gmeow:Observation`` (a
diagnostic IS an observation, Principle 9), the severity / location properties
as subproperties of the observation roles, the closed-by-convention
``gmeow:DiagnosticSeverity`` value vocabulary (individuals, never subclasses),
and the GTS wire-coordinate datatype properties. The slice is a PROJECTION of
the Rust-owned canonical report, so it deliberately mints no truth/resolution
bits.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"
SKOS_DEFINITION = URIRef("http://www.w3.org/2004/02/skos/core#definition")
SLICE_IRI = URIRef("https://blackcatinformatics.ca/gmeow/slices/diagnostics")
_MODULE = Path(__file__).resolve().parents[1] / "module.ttl"

_SEVERITY_INDIVIDUALS = (
    "severityError",
    "severityWarning",
    "severityNote",
    "severityInfo",
)
# The four wire coordinates this slice OWNS (open-domain datatype properties a
# finding's location node carries). The fifth coordinate a finding may carry —
# gmeow:gtsSegmentIndex — is owned by the gts slice (functional, domain GTSSegment;
# the index that IS a document's composite identity, spec §3.1) and only REFERENCED
# here, so it is not in the locally-declared sets (single-owner invariant, #329).
_WIRE_COORDS = (
    "gtsTermId",
    "gtsQuadIndex",
    "gtsReifierId",
    "gtsFrameIndex",
)
_DATATYPE_PROPS = ("findingCode", "findingMessage", "findingTool", *_WIRE_COORDS)

# Every locally-declared term (15 total): the Finding class, the
# DiagnosticSeverity class, the 4 severity individuals, the 2 observation
# subproperties (findingSeverity / findingLocation), and the 3 + 4 datatype
# properties (code/message/tool + the 4 diagnostics-owned wire coordinates).
_DECLARED_TERMS = (
    "Finding",
    "DiagnosticSeverity",
    *_SEVERITY_INDIVIDUALS,
    "findingSeverity",
    "findingLocation",
    *_DATATYPE_PROPS,
)


def _graph() -> Graph:
    g = Graph()
    g.parse(_MODULE, format="turtle")
    return g


def _g(name: str) -> URIRef:
    return URIRef(GMEOW + name)


def test_finding_is_a_subkind_of_observation() -> None:
    """A diagnostic IS an observation — a rigid gufo:SubKind specialization.

    Finding must be a gufo:SubKind, NOT a gufo:Kind: gmeow:Observation is itself a
    gufo:Kind, and a Kind specializing a Kind is the OntoUML MixIden identity
    conflict (every endurant instantiates exactly one Kind). The SubKind inherits
    Observation's identity principle.
    """
    g = _graph()
    finding = _g("Finding")
    assert (finding, RDF.type, OWL.Class) in g
    assert (finding, RDF.type, URIRef(GUFO + "SubKind")) in g
    assert (finding, RDF.type, URIRef(GUFO + "Kind")) not in g
    assert (finding, RDFS.subClassOf, _g("Observation")) in g


def test_severity_and_location_subproperty_observation_roles() -> None:
    """Severity ⊑ observationResult; location ⊑ observedFeature."""
    g = _graph()
    assert (_g("findingSeverity"), RDFS.subPropertyOf, _g("observationResult")) in g
    assert (_g("findingLocation"), RDFS.subPropertyOf, _g("observedFeature")) in g
    # findingSeverity ranges over the closed severity vocabulary.
    assert (_g("findingSeverity"), RDFS.range, _g("DiagnosticSeverity")) in g
    # findingLocation keeps an OPEN range (per-kind narrowing is SHACL's job).
    assert (_g("findingLocation"), RDFS.range, None) not in g


def test_diagnostic_severity_is_a_value_vocabulary() -> None:
    """DiagnosticSeverity is a gufo:QualityValue; grades are individuals."""
    g = _graph()
    cls = _g("DiagnosticSeverity")
    assert (cls, RDF.type, OWL.Class) in g
    assert (cls, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in g
    for name in _SEVERITY_INDIVIDUALS:
        individual = _g(name)
        assert (individual, RDF.type, cls) in g, f"{name} must be a DiagnosticSeverity"
        # Grades are individuals, never subclasses.
        assert (individual, RDF.type, OWL.Class) not in g, f"{name} must not be a class"


def test_wire_coordinates_are_datatype_properties() -> None:
    """The GTS wire coordinates are datatype properties (range integers)."""
    g = _graph()
    nni = URIRef("http://www.w3.org/2001/XMLSchema#nonNegativeInteger")
    for name in _WIRE_COORDS:
        prop = _g(name)
        assert (prop, RDF.type, OWL.DatatypeProperty) in g, f"{name} must be datatype"
        assert (prop, RDFS.range, nni) in g, (
            f"{name} must range over nonNegativeInteger"
        )


def test_no_truth_or_resolution_bits() -> None:
    """The slice is a projection: it mints no isTrue/isResolved/outcome bits."""
    g = _graph()
    for forbidden in ("isTrue", "isFalse", "isResolved", "findingOutcome"):
        assert (_g(forbidden), None, None) not in g, f"{forbidden} must not exist"


def test_annotation_completeness() -> None:
    """Every locally-declared term carries the label/definition/isDefinedBy triad."""
    g = _graph()
    for name in _DECLARED_TERMS:
        term = _g(name)
        assert (term, RDFS.label, None) in g, f"{name} missing rdfs:label"
        assert (term, SKOS_DEFINITION, None) in g, f"{name} missing skos:definition"
        assert (term, RDFS.isDefinedBy, SLICE_IRI) in g, f"{name} missing isDefinedBy"


def test_graph_box_role_coverage() -> None:
    """Every locally-declared term declares its graph-box role."""
    g = _graph()
    box_role = _g("graphBoxRole")
    for name in _DECLARED_TERMS:
        assert (_g(name), box_role, None) in g, f"{name} missing gmeow:graphBoxRole"
