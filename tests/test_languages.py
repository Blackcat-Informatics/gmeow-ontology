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
from rdflib.namespace import SKOS

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
            URIRef(GUFO + "QualityValue"),
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
    # writtenInLanguage is the GENERIC content-language relation (#287
    # surgery): any InformationObject — a document, a source tree, an
    # inscription reading — is written in a first-class Language. The
    # software case is the subsumed instance (SourceTree ⊑ InformationObject,
    # ProgrammingLanguage ⊑ Language), never a software-shaped domain.
    written = URIRef(GMEOW + "writtenInLanguage")
    assert (written, RDFS.domain, URIRef(GMEOW + "InformationObject")) in graph
    assert (written, RDFS.range, URIRef(GMEOW + "Language")) in graph
    # versionOf is functional (a version belongs to exactly one lineage).
    assert (
        URIRef(GMEOW + "versionOf"),
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph


def test_internal_language_tags() -> None:
    graph = _graph()
    tag = URIRef(GMEOW + "languageTag")
    assert (tag, RDF.type, OWL.DatatypeProperty) in graph
    assert (tag, RDF.type, OWL.FunctionalProperty) in graph
    assert (tag, RDFS.domain, URIRef(GMEOW + "Language")) in graph
    assert (tag, RDFS.range, URIRef("http://www.w3.org/2001/XMLSchema#string")) in graph


def test_projection_bcp47_tags_are_distinct_from_registry_codes() -> None:
    graph = _graph()
    tag = URIRef(GMEOW + "bcp47Tag")
    assert (tag, RDF.type, OWL.DatatypeProperty) in graph
    assert (tag, RDF.type, OWL.FunctionalProperty) not in graph
    assert (tag, RDFS.domain, URIRef(GMEOW + "Language")) in graph
    assert (
        tag,
        RDFS.range,
        URIRef("http://www.w3.org/2001/XMLSchema#language"),
    ) in graph


def test_core_seed_languages_use_reference_catalog_iris() -> None:
    """Core seeds share the gmeow:lang* IRI style with the reference catalog (#111)."""
    graph = _graph()
    for lang_iri in ("langEnglish", "langFrench", "langMandarin"):
        iri = URIRef(GMEOW + lang_iri)
        assert (iri, RDF.type, URIRef(GMEOW + "Language")) in graph
        assert (iri, URIRef(GMEOW + "languageTag"), None) in graph
        assert (iri, URIRef(GMEOW + "bcp47Tag"), None) in graph


def test_reference_catalog_languages_are_annotated_and_aligned() -> None:
    """Reference-catalog languages carry labels, tags, and alignments (#111)."""
    graph = load_merged_graph(include_imports=True)
    defined_by = URIRef(GMEOW + "imports/languages-reference")
    tag_prop = URIRef(GMEOW + "languageTag")
    bcp_prop = URIRef(GMEOW + "bcp47Tag")
    code_prop = URIRef(GMEOW + "languageCode")
    catalog_subjects: set[URIRef] = set()
    for cls_name in ("Language", "FormalLanguage", "ProgrammingLanguage"):
        for subject in graph.subjects(RDF.type, URIRef(GMEOW + cls_name)):
            if (
                isinstance(subject, URIRef)
                and (
                    subject,
                    RDFS.isDefinedBy,
                    defined_by,
                )
                in graph
            ):
                catalog_subjects.add(subject)
    catalog_languages = sorted(catalog_subjects, key=str)
    assert len(catalog_languages) >= 30, "catalog should contain many languages"

    for lang in catalog_languages:
        assert (lang, RDFS.label, None) in graph, f"{lang} missing rdfs:label"
        assert (lang, SKOS.definition, None) in graph, f"{lang} missing skos:definition"
        assert (lang, tag_prop, None) in graph, f"{lang} missing languageTag"
        assert (lang, bcp_prop, None) in graph, f"{lang} missing bcp47Tag"
        assert (lang, code_prop, None) in graph, f"{lang} missing languageCode"
        assert (lang, SKOS.exactMatch, None) in graph, f"{lang} missing skos:exactMatch"


def test_reference_catalog_programming_languages_typed() -> None:
    """Reference-catalog programming languages are typed correctly (#111)."""
    graph = load_merged_graph(include_imports=True)
    for lang_iri in (
        "langPython",
        "langRust",
        "langJavaScript",
        "langTypeScript",
        "langJava",
    ):
        iri = URIRef(GMEOW + lang_iri)
        assert (iri, RDF.type, URIRef(GMEOW + "ProgrammingLanguage")) in graph


def test_reference_catalog_glottolog_alignments() -> None:
    """Natural languages in the reference catalog link to Glottolog (#111)."""
    graph = load_merged_graph(include_imports=True)
    defined_by = URIRef(GMEOW + "imports/languages-reference")
    # langEnglish is defined in both core and catalog; use catalog-only languages.
    glottolog_base = "https://glottolog.org/resource/languoid/id/"
    for lang_iri in ("langJapanese", "langArabic", "langHindi", "langSpanish"):
        iri = URIRef(GMEOW + lang_iri)
        assert (iri, RDFS.isDefinedBy, defined_by) in graph
        matches = list(graph.objects(iri, SKOS.exactMatch))
        glottos = [
            m
            for m in matches
            if isinstance(m, URIRef) and str(m).startswith(glottolog_base)
        ]
        assert glottos, f"{lang_iri} missing Glottolog skos:exactMatch"


def test_language_tag_map_is_deterministic_and_covers_catalog() -> None:
    """Public retagging map is deterministic and covers the catalog (#111, #164)."""
    from gmeow_tools.language_tags import load_tag_map

    graph_a = load_merged_graph(include_imports=True)
    graph_b = load_merged_graph(include_imports=True)
    tag_map_a = load_tag_map(graph_a)
    tag_map_b = load_tag_map(graph_b)
    assert tag_map_a == tag_map_b, "load_tag_map output must be deterministic"

    for internal_tag in (
        "x-gmeow-english",
        "x-gmeow-french",
        "x-gmeow-mandarin",
        "x-gmeow-japanese",
        "x-gmeow-arabic",
        "x-gmeow-hindi",
        "x-gmeow-python",
    ):
        assert internal_tag in tag_map_a, f"missing tag mapping for {internal_tag}"
        assert tag_map_a[internal_tag], f"empty BCP-47 mapping for {internal_tag}"
