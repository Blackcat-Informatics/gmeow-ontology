"""Structural + DL-safety guards for the languages building block (RETAINED tests).

The following tests have been migrated to declarative slicetest cells in
slices/extensions/languages/tests/structural.ttl and removed from this file:

  test_reified_relators_and_functional_roles  → ex:saWritingSystemUsageIsRelator,
      ex:saUsageLanguageFunctional, ex:saUsageWritingSystemFunctional,
      ex:saScriptRoleFunctional, ex:saLanguageProficiencyIsRelator,
      ex:saProficiencyAgentFunctional, ex:saProficiencyLanguageFunctional,
      ex:saProficiencyModalityFunctional, ex:saProficiencyLevelFunctional,
      ex:saProficiencyScaleFunctional
  test_value_vocabularies_not_subclasses      → ex:saLanguageOriginIsValueVocab,
      ex:saLanguageOriginSeeds, ex:saLanguageModalityIsValueVocab,
      ex:saLanguageModalitySeeds, ex:saLanguageStatusIsValueVocab,
      ex:saLanguageStatusSeeds, ex:saWritingSystemTypeIsValueVocab,
      ex:saWritingSystemTypeSeeds, ex:saTextDirectionIsValueVocab,
      ex:saTextDirectionSeeds, ex:saScriptRoleIsValueVocab, ex:saScriptRoleSeeds,
      ex:saProficiencyModalitySeeds, ex:saProficiencyScaleSeeds,
      ex:saProficiencyLevelSeeds, ex:saTransliterationSchemeSeeds
  test_cefr_levels_carry_their_scale          → ex:saCefrB2CarriesScale
  test_script_usage_interval_distinct_from_names_usage_interval →
      ex:saScriptUsageIntervalDomain, ex:saUsageIntervalNotDomainWritingSystemUsage
  test_knows_language_shortcut_and_native_subproperty →
      ex:saKnowsLanguageIsObjectProperty, ex:saKnowsLanguageNotFunctional,
      ex:saNativeLanguageSubPropertyOf

RETAINED here (not migratable as scopeModule cells):

  test_language_hierarchy — Language, FormalLanguage, ProgrammingLanguage,
    WritingSystem all defined in slices/core/language/module.ttl (cross-slice).
  test_registry_independence_no_required_code — languageCode in
    slices/core/language/module.ttl; also performs a dynamic cardinality sweep.
  test_transliteration_scheme_retrofits_names — transliterationScheme in
    slices/core/language/module.ttl (cross-slice).
  test_software_bridge_and_version_lineage — writtenInLanguage in
    slices/core/language/module.ttl; versionOf in slices/core/coreference/module.ttl
    (both cross-slice).
  test_internal_language_tags — languageTag in slices/core/language/module.ttl.
  test_projection_bcp47_tags_are_distinct_from_registry_codes — bcp47Tag in
    slices/core/language/module.ttl (cross-slice).
  test_core_seed_languages_use_reference_catalog_iris — langEnglish, langFrench,
    langMandarin in slices/core/language/module.ttl (cross-slice).
  test_reference_catalog_languages_are_annotated_and_aligned — include_imports=True;
    dynamic sweep with numeric ISO 639-1 count assertions.
  test_reference_catalog_writing_systems_are_annotated — include_imports=True;
    dynamic whole-graph sweep.
  test_reference_catalog_programming_languages_typed — include_imports=True;
    reference-catalog subjects (cross-slice).
  test_reference_catalog_glottolog_alignments — include_imports=True; dynamic.
  test_language_tag_map_is_deterministic_and_covers_catalog — tests Python tool
    function (gmeow_tools.language_tags.load_tag_map), not a TBox triple assertion.
  test_inverse_tag_map_recovers_natural_internal_tags — tests Python tool function.
  test_retag_graph_to_internal_lifts_public_to_canonical — tests Python tool
    function on a synthetic graph; not a TBox triple assertion.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, URIRef
from gmeow_rdf.compat.rdflib.namespace import SKOS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"

#: Complete ISO 639-1 two-letter code set (184 entries). Stable since 2000.
EXPECTED_ISO639_1_CODES: frozenset[str] = frozenset(
    {
        "aa",
        "ab",
        "ae",
        "af",
        "ak",
        "am",
        "an",
        "ar",
        "as",
        "av",
        "ay",
        "az",
        "ba",
        "be",
        "bg",
        "bi",
        "bm",
        "bn",
        "bo",
        "br",
        "bs",
        "ca",
        "ce",
        "ch",
        "co",
        "cr",
        "cs",
        "cu",
        "cv",
        "cy",
        "da",
        "de",
        "dv",
        "dz",
        "ee",
        "el",
        "en",
        "eo",
        "es",
        "et",
        "eu",
        "fa",
        "ff",
        "fi",
        "fj",
        "fo",
        "fr",
        "fy",
        "ga",
        "gd",
        "gl",
        "gn",
        "gu",
        "gv",
        "ha",
        "he",
        "hi",
        "ho",
        "hr",
        "ht",
        "hu",
        "hy",
        "hz",
        "ia",
        "id",
        "ie",
        "ig",
        "ii",
        "ik",
        "io",
        "is",
        "it",
        "iu",
        "ja",
        "jv",
        "ka",
        "kg",
        "ki",
        "kj",
        "kk",
        "kl",
        "km",
        "kn",
        "ko",
        "kr",
        "ks",
        "ku",
        "kv",
        "kw",
        "ky",
        "la",
        "lb",
        "lg",
        "li",
        "ln",
        "lo",
        "lt",
        "lu",
        "lv",
        "mg",
        "mh",
        "mi",
        "mk",
        "ml",
        "mn",
        "mr",
        "ms",
        "mt",
        "my",
        "na",
        "nb",
        "nd",
        "ne",
        "ng",
        "nl",
        "nn",
        "no",
        "nr",
        "nv",
        "ny",
        "oc",
        "oj",
        "om",
        "or",
        "os",
        "pa",
        "pi",
        "pl",
        "ps",
        "pt",
        "qu",
        "rm",
        "rn",
        "ro",
        "ru",
        "rw",
        "sa",
        "sc",
        "sd",
        "se",
        "sg",
        "sh",
        "si",
        "sk",
        "sl",
        "sm",
        "sn",
        "so",
        "sq",
        "sr",
        "ss",
        "st",
        "su",
        "sv",
        "sw",
        "ta",
        "te",
        "tg",
        "th",
        "ti",
        "tk",
        "tl",
        "tn",
        "to",
        "tr",
        "ts",
        "tt",
        "tw",
        "ty",
        "ug",
        "uk",
        "ur",
        "uz",
        "ve",
        "vi",
        "vo",
        "wa",
        "wo",
        "xh",
        "yi",
        "yo",
        "za",
        "zh",
        "zu",
    }
)


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
    """Reference-catalog languages carry labels, tags, and alignments (#111, #396)."""
    graph = load_merged_graph(include_imports=True)
    defined_by = URIRef(GMEOW + "imports/languages-reference")
    tag_prop = URIRef(GMEOW + "languageTag")
    bcp_prop = URIRef(GMEOW + "bcp47Tag")
    code_prop = URIRef(GMEOW + "languageCode")
    catalog_subjects: set[URIRef] = set()
    for subject in graph.subjects(RDF.type, URIRef(GMEOW + "Language")):
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

    catalog_iso1_codes = {
        str(obj)
        for lang in catalog_languages
        for obj in graph.objects(lang, code_prop)
        if isinstance(obj, Literal) and len(str(obj)) == 2
    }
    assert catalog_iso1_codes == EXPECTED_ISO639_1_CODES, (
        f"missing ISO 639-1 codes: {EXPECTED_ISO639_1_CODES - catalog_iso1_codes}; "
        f"unexpected codes: {catalog_iso1_codes - EXPECTED_ISO639_1_CODES}"
    )

    for lang in catalog_languages:
        assert (lang, RDFS.label, None) in graph, f"{lang} missing rdfs:label"
        assert (lang, SKOS.definition, None) in graph, f"{lang} missing skos:definition"
        assert (lang, tag_prop, None) in graph, f"{lang} missing languageTag"
        assert (lang, bcp_prop, None) in graph, f"{lang} missing bcp47Tag"
        assert (lang, code_prop, None) in graph, f"{lang} missing languageCode"
        assert (lang, SKOS.exactMatch, None) in graph, f"{lang} missing skos:exactMatch"


def test_reference_catalog_writing_systems_are_annotated() -> None:
    """Every writing system referenced by the catalog carries required annotations."""
    graph = load_merged_graph(include_imports=True)
    defined_by = URIRef(GMEOW + "imports/languages-reference")
    lang_type = URIRef(GMEOW + "Language")
    uses_ws = URIRef(GMEOW + "usesWritingSystem")
    ws_type = URIRef(GMEOW + "WritingSystem")

    catalog_languages = {
        s
        for s in graph.subjects(RDF.type, lang_type)
        if isinstance(s, URIRef) and (s, RDFS.isDefinedBy, defined_by) in graph
    }
    writing_systems = {
        ws
        for lang in catalog_languages
        for ws in graph.objects(lang, uses_ws)
        if isinstance(ws, URIRef)
    }

    assert writing_systems, "No writing systems found for reference-catalog languages"
    for iri in writing_systems:
        assert (iri, RDF.type, ws_type) in graph, f"{iri} missing gmeow:WritingSystem"
        assert (iri, RDFS.label, None) in graph, f"{iri} missing rdfs:label"
        assert (iri, SKOS.definition, None) in graph, f"{iri} missing skos:definition"


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


def test_inverse_tag_map_recovers_natural_internal_tags() -> None:
    """The inverse map (public BCP-47 → internal) is the up-projection direction —
    ``fnComposeBcp`` read backwards (#451). It is built from **natural** languages
    only: a programming language's code is also tagged ``@en``, so including the
    programming catalog would make ``en`` ambiguous and drop it — but a consumer
    *prose* ``@en`` literal is natural English, never code."""
    from gmeow_tools.language_tags import load_inverse_tag_map

    inverse = load_inverse_tag_map(load_merged_graph(include_imports=True))
    assert inverse["en"] == "x-gmeow-english"
    assert inverse["fr"] == "x-gmeow-french"
    assert inverse["zh"] == "x-gmeow-mandarin"


def test_retag_graph_to_internal_lifts_public_to_canonical() -> None:
    """``retag_graph_to_internal`` retags public BCP-47 literals up to the canonical
    internal form and leaves untagged / already-internal literals alone (#451)."""
    from gmeow_rdf.compat.rdflib import Graph as RDFGraph
    from gmeow_rdf.compat.rdflib import Literal, URIRef

    from gmeow_tools.language_tags import retag_graph_to_internal

    g = RDFGraph()
    s = URIRef("https://ex.org/s")
    plain = URIRef("https://ex.org/plain")
    g.add((s, URIRef("https://ex.org/prose"), Literal("hello", lang="en")))
    g.add(
        (s, URIRef("https://ex.org/canon"), Literal("bonjour", lang="x-gmeow-french"))
    )
    g.add((s, plain, Literal("42")))
    retag_graph_to_internal(g)
    tags = {o.language for _s, _p, o in g if isinstance(o, Literal) and o.language}
    assert tags == {"x-gmeow-english", "x-gmeow-french"}, "public lifted, internal kept"
    assert (s, plain, Literal("42")) in g, "untagged literal untouched"
