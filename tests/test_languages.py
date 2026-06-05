"""Structural + DL-safety guards for the languages building block.

Pins the registry-INDEPENDENT Language hierarchy, first-class WritingSystem, the
two reified relators (WritingSystemUsage for co-mingled scripts, LanguageProficiency
for leveled skill) and their functional roles, the value-vs-subclass decisions, the
software bridge, the names↔languages transliteration retrofit, and the invariants:
a language needs no registry code, and scriptUsageInterval is distinct from names'
usageInterval.
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_language_hierarchy() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Language"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "FormalLanguage"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Language"),
    ) in graph
    assert (
        URIRef(GMEOW + "ProgrammingLanguage"),
        RDFS.subClassOf,
        URIRef(GMEOW + "FormalLanguage"),
    ) in graph
    assert (
        URIRef(GMEOW + "LanguageVersion"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Language"),
    ) in graph
    assert (
        URIRef(GMEOW + "WritingSystem"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph


def test_reified_relators_and_functional_roles() -> None:
    graph = _graph()
    for relator, roles in (
        ("WritingSystemUsage", ("usageLanguage", "usageWritingSystem", "scriptRole")),
        (
            "LanguageProficiency",
            (
                "proficiencyAgent",
                "proficiencyLanguage",
                "proficiencyModality",
                "proficiencyLevel",
                "proficiencyScale",
            ),
        ),
    ):
        assert (
            URIRef(GMEOW + relator),
            RDFS.subClassOf,
            URIRef(GUFO + "Relator"),
        ) in graph
        for role in roles:
            node = URIRef(GMEOW + role)
            assert (node, RDF.type, OWL.ObjectProperty) in graph
            assert (node, RDF.type, OWL.FunctionalProperty) in graph


def test_value_vocabularies_not_subclasses() -> None:
    graph = _graph()
    for vocab, sample in (
        ("LanguageOrigin", ("originNatural", "originAiGenerated", "originProgramming")),
        ("LanguageModality", ("modalitySpoken", "modalitySigned", "modalityMachine")),
        ("LanguageStatus", ("statusLiving", "statusConstructedActive")),
        ("WritingSystemType", ("wsTypeLogographic", "wsTypeNonLinear")),
        ("TextDirection", ("directionLtr", "directionBoustrophedon")),
        ("ScriptRole", ("scriptRoleLogographicContent", "scriptRoleTransliteration")),
        ("ProficiencyModality", ("profModalitySpeaking", "profModalityOverall")),
        ("ProficiencyScale", ("scaleCEFR", "scaleILR", "scaleACTFL")),
        ("ProficiencyLevel", ("cefrB2", "levelNative", "levelHeritage")),
        ("TransliterationScheme", ("schemeHepburn", "schemePinyin", "schemeIPA")),
    ):
        assert (
            URIRef(GMEOW + vocab),
            RDFS.subClassOf,
            URIRef(GMEOW + "Entity"),
        ) in graph
        for ind in sample:
            assert (URIRef(GMEOW + ind), RDF.type, URIRef(GMEOW + vocab)) in graph


def test_cefr_levels_carry_their_scale() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "cefrB2"),
        URIRef(GMEOW + "levelScale"),
        URIRef(GMEOW + "scaleCEFR"),
    ) in graph


def test_registry_independence_no_required_code() -> None:
    """A gmeow:Language needs NO code — code-less conlangs/AI-langs are first-class."""
    graph = _graph()
    code = URIRef(GMEOW + "languageCode")
    assert (code, RDF.type, OWL.DatatypeProperty) in graph
    # Optional + multi-registry: never functional.
    assert (code, RDF.type, OWL.FunctionalProperty) not in graph
    # No cardinality restriction anywhere forces a language to carry a code.
    for restriction in graph.subjects(OWL.onProperty, code):
        for card_pred in (
            OWL.minCardinality,
            OWL.cardinality,
            OWL.minQualifiedCardinality,
        ):
            assert not list(graph.objects(restriction, card_pred))


def test_script_usage_interval_distinct_from_names_usage_interval() -> None:
    graph = _graph()
    # The languages relator carries its period via a DISTINCT property...
    assert (
        URIRef(GMEOW + "scriptUsageInterval"),
        RDFS.domain,
        URIRef(GMEOW + "WritingSystemUsage"),
    ) in graph
    # ...not by reusing names' usageInterval (which stays scoped to NameUsage).
    assert (
        URIRef(GMEOW + "usageInterval"),
        RDFS.domain,
        URIRef(GMEOW + "WritingSystemUsage"),
    ) not in graph


def test_knows_language_shortcut_and_native_subproperty() -> None:
    graph = _graph()
    knows = URIRef(GMEOW + "knowsLanguage")
    assert (knows, RDF.type, OWL.ObjectProperty) in graph
    assert (knows, RDF.type, OWL.FunctionalProperty) not in graph
    assert (URIRef(GMEOW + "nativeLanguage"), RDFS.subPropertyOf, knows) in graph


def test_transliteration_scheme_retrofits_names() -> None:
    graph = _graph()
    scheme = URIRef(GMEOW + "transliterationScheme")
    # Domain Appellation = the names-module retrofit; range the scheme vocabulary.
    assert (scheme, RDFS.domain, URIRef(GMEOW + "Appellation")) in graph
    assert (scheme, RDFS.range, URIRef(GMEOW + "TransliterationScheme")) in graph


def test_software_bridge_and_version_lineage() -> None:
    graph = _graph()
    written = URIRef(GMEOW + "writtenInLanguage")
    assert (written, RDFS.domain, URIRef(GMEOW + "SoftwareProject")) in graph
    assert (written, RDFS.range, URIRef(GMEOW + "ProgrammingLanguage")) in graph
    # versionOf is functional (a version belongs to exactly one lineage).
    assert (
        URIRef(GMEOW + "versionOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph
