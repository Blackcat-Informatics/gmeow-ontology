// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical GMEOW prefix registry and CURIE shortening — the single authority
//! shared by every correspondence lowering (SSSOM CURIEs, EDOAL/SPARQL prefix
//! headers) and the projection lints.

/// The canonical GMEOW prefix registry, in `config.PREFIXES` **insertion order**.
///
/// This is the single authority for both CURIE-shortening and the emitted prefix
/// blocks. It mirrors the curated `PREFIXES` config, NOT the per-file `@prefix`
/// declarations — those use different prefix *names* for the same namespace (e.g. a
/// source declares `bf:` where the registry names it `bibframe`), some registry
/// prefixes are never declared in a source they shorten, and some source prefixes
/// are absent from the registry (left as bare absolute URIs). Using `@prefix`
/// declarations instead of this registry produces drift on the committed corpus, so
/// byte-parity demands the curated registry.
///
/// Insertion order is load-bearing: CURIE-shortening sorts by descending namespace
/// length with the *registry order* as the tie-break (a stable sort keyed on
/// descending namespace length over the registry's insertion order).
pub const PREFIX_REGISTRY: &[(&str, &str)] = &[
    ("gmeow", "https://blackcatinformatics.ca/gmeow/"),
    ("logic", "https://blackcatinformatics.ca/logic/"),
    ("lang", "https://blackcatinformatics.ca/lang/"),
    ("math", "https://blackcatinformatics.ca/math/"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("vs", "http://www.w3.org/2003/06/sw-vocab-status/ns#"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcmitype", "http://purl.org/dc/dcmitype/"),
    ("vann", "http://purl.org/vocab/vann/"),
    ("void", "http://rdfs.org/ns/void#"),
    ("dcat", "http://www.w3.org/ns/dcat#"),
    ("sh", "http://www.w3.org/ns/shacl#"),
    ("dqv", "http://www.w3.org/ns/dqv#"),
    ("sssom", "https://w3id.org/sssom/"),
    ("semapv", "https://w3id.org/semapv/vocab/"),
    ("fno", "https://w3id.org/function/ontology#"),
    ("fnom", "https://w3id.org/function/vocabulary/mapping#"),
    ("edoal", "http://ns.inria.org/edoal/1.0/#"),
    (
        "align",
        "http://knowledgeweb.semanticweb.org/heterogeneity/alignment#",
    ),
    // Affect classifier label registries — per-registry external label identities,
    // held under a distinct authority path so a model label can never be mistaken
    // for a canonical gmeow: emotion term.
    (
        "gmeow-goemotions",
        "https://blackcatinformatics.ca/gmeow-registry/goemotions/",
    ),
    (
        "gmeow-hf",
        "https://blackcatinformatics.ca/gmeow-registry/hf/",
    ),
    (
        "gmeow-labelset",
        "https://blackcatinformatics.ca/gmeow-registry/labelset/",
    ),
    // W3C EmotionML Vocabularies — the external bridge target of the affect EmotionML
    // projection (set-level relatedMatch cells; the per-item categories are XML `name`
    // attributes with no per-term IRI, so only the vocabulary-set anchors are bridged).
    ("emo", "https://www.w3.org/TR/emotion-voc/#"),
    // Open English WordNet — the live, dereferenceable OntoLex-Lemon surface for the
    // affect lexical bridge. Its per-synset IRIs (…/id/oewn-<offset>-<pos>) content-
    // negotiate to lemon RDF, so emotion terms bridge to WordNet "feeling" synsets by
    // reference. The tight base yields clean CURIEs (oewn:07531593-n), matching the
    // obi:/iao: convention. (Distinct from the defunct Princeton WordNet-Affect export.)
    ("oewn", "https://en-word.net/id/oewn-"),
    ("gufo", "http://purl.org/nemo/gufo#"),
    ("ontouml", "https://w3id.org/ontouml#"),
    ("umbel", "http://umbel.org/umbel#"),
    ("umbelrc", "http://umbel.org/umbel/rc/"),
    (
        "dul",
        "http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#",
    ),
    ("bfo", "http://purl.obolibrary.org/obo/"),
    ("ro", "http://purl.obolibrary.org/obo/RO_"),
    ("sumo", "https://www.ontologyportal.org/SUMO.owl#"),
    ("cyc", "http://sw.opencyc.org/2012/05/10/concept/en/"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("rel", "http://purl.org/vocab/relationship/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("prof", "http://www.w3.org/ns/dx/prof/"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ssn", "http://www.w3.org/ns/ssn/"),
    ("sweet", "http://sweetontology.net/"),
    ("om", "http://www.wurvoc.org/vocabularies/om-1.8/"),
    ("qb", "http://purl.org/linked-data/cube#"),
    ("mf", "http://www.opengis.net/ont/movingfeatures#"),
    ("sta", "http://www.opengis.net/def/ont/sensorthings/1.1/"),
    ("iso19156", "http://www.isotc211.org/iso19156/"),
    (
        "oboe",
        "http://ecoinformatics.org/oboe/oboe.1.2/oboe-core.owl#",
    ),
    ("obi", "http://purl.obolibrary.org/obo/OBI_"),
    ("iao", "http://purl.obolibrary.org/obo/IAO_"),
    ("pato", "http://purl.obolibrary.org/obo/PATO_"),
    ("crmarc", "http://www.cidoc-crm.org/crmarchaeo/"),
    ("iptc", "http://iptc.org/std/NewsML-G2/"),
    ("bbc", "http://www.bbc.co.uk/ontologies/news/"),
    ("obscore", "http://www.ivoa.net/rdf/ObsCore#"),
    ("ppsr", "https://purl.org/ppsr/core#"),
    ("loinc", "http://loinc.org/rdf/"),
    ("snomed", "http://snomed.info/id/"),
    ("np", "http://www.nanopub.org/nschema#"),
    ("crm", "http://www.cidoc-crm.org/cidoc-crm/"),
    ("crmsci", "http://www.cidoc-crm.org/extensions/crmsci/"),
    ("crminf", "http://www.ics.forth.gr/isl/CRMinf/"),
    ("crmdig", "http://www.ics.forth.gr/isl/CRMdig/"),
    ("oa", "http://www.w3.org/ns/oa#"),
    ("exif", "http://www.w3.org/2003/12/exif/ns#"),
    ("iiif", "http://iiif.io/api/presentation/3#"),
    ("cito", "http://purl.org/spar/cito/"),
    ("credit", "https://credit.niso.org/contributor-roles/"),
    ("pav", "http://purl.org/pav/"),
    ("org", "http://www.w3.org/ns/org#"),
    ("moat", "http://moat-project.org/ns#"),
    ("tags", "http://www.holygoat.co.uk/owl/redwood/0.1/tags/"),
    ("time", "http://www.w3.org/2006/time#"),
    ("teo", "https://sbmi.uth.edu/bsdi/TEO_1.0.0.owl#"),
    ("pos", "http://purl.org/ieee1872-owl/pos#"),
    ("cora", "http://purl.org/ieee1872-owl/cora#"),
    ("knowrob", "http://knowrob.org/kb/knowrob.owl#"),
    ("soma", "http://www.ease-crc.org/ont/SOMA.owl#"),
    ("qudt", "http://qudt.org/schema/qudt/"),
    ("unit", "http://qudt.org/vocab/unit/"),
    ("edtf", "http://id.loc.gov/datatypes/edtf/"),
    ("periodo", "http://n2t.net/ark:/99152/"),
    (
        "gts",
        "http://resource.geosciml.org/ontology/timescale/gts#",
    ),
    ("ivoa", "http://www.ivoa.net/rdf/"),
    ("crmgeo", "http://www.ics.forth.gr/isl/CRMgeo/"),
    ("lode", "http://linkedevents.org/ontology/"),
    ("sem", "http://semanticweb.cs.vu.nl/2009/11/sem/"),
    ("ical", "http://www.w3.org/2002/12/cal/icaltzd#"),
    ("schema", "https://schema.org/"),
    ("gedcom", "http://www.w3.org/2000/10/swap/pim/gedcom#"),
    ("vcard", "http://www.w3.org/2006/vcard/ns#"),
    ("mo", "http://purl.org/ontology/mo/"),
    ("mbz", "https://musicbrainz.org/"),
    ("discogs", "https://www.discogs.com/"),
    ("afo", "https://w3id.org/afo/onto/1.1#"),
    ("afv", "https://w3id.org/afo/vocab/1.1#"),
    ("jams", "http://w3id.org/polifonia/ontology/jams/"),
    ("pon", "https://w3id.org/polifonia/ontology/"),
    ("chord", "http://purl.org/ontology/chord/"),
    ("mimo", "http://www.mimo-db.eu/InstrumentsKeywords/"),
    ("pplan", "http://purl.org/net/p-plan#"),
    ("opmw", "https://www.opmw.org/ontology/"),
    ("bpmn", "http://www.omg.org/spec/BPMN/20100524/MODEL#"),
    ("ro_crate", "https://w3id.org/ro/crate/#"),
    ("brick", "https://brickschema.org/schema/Brick#"),
    ("bot", "https://w3id.org/bot#"),
    ("ifc", "http://www.buildingsmart-tech.org/ifcOWL/IFC4#"),
    ("vcardx", "https://blackcatinformatics.ca/vcard-ext/"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("sf", "http://www.opengis.net/ont/sf#"),
    ("wgs84", "http://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("gtfs", "http://vocab.gtfs.org/terms#"),
    ("tgn", "http://vocab.getty.edu/tgn/"),
    ("lgdo", "http://linkedgeodata.org/ontology/"),
    ("pleiades", "http://pleiades.stoa.org/places/vocab#"),
    ("whg", "https://whgazetteer.org/"),
    ("gvp", "http://vocab.getty.edu/ontology#"),
    ("mrg", "http://marineregions.org/ns/ontology#"),
    ("bibo", "http://purl.org/ontology/bibo/"),
    ("bibframe", "http://id.loc.gov/ontologies/bibframe/"),
    ("dpv", "https://w3id.org/dpv#"),
    ("frbr", "http://purl.org/vocab/frbr/core#"),
    ("fabio", "http://purl.org/spar/fabio/"),
    ("lrmoo", "http://iflastandards.info/ns/lrm/lrmoo/"),
    ("sioc", "http://rdfs.org/sioc/ns#"),
    ("as", "https://www.w3.org/ns/activitystreams#"),
    ("mads", "http://www.loc.gov/mads/rdf/v1#"),
    ("esco", "http://data.europa.eu/esco/model#"),
    ("esco-base", "http://data.europa.eu/esco/"),
    ("ceterms", "https://purl.org/ctdl/terms/"),
    ("ctdlasn", "https://credreg.net/ctdlasn/terms/"),
    ("onet", "https://www.onetcenter.org/"),
    (
        "nmo",
        "http://www.semanticdesktop.org/ontologies/2007/03/22/nmo#",
    ),
    ("wot", "http://xmlns.com/wot/0.1/"),
    ("vc", "https://www.w3.org/2018/credentials#"),
    ("did", "https://www.w3.org/ns/did#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("cc", "http://creativecommons.org/ns#"),
    ("premis", "http://www.loc.gov/premis/rdf/v3/"),
    ("rstmt", "https://rightsstatements.org/vocab/"),
    ("ccpd", "https://creativecommons.org/publicdomain/"),
    ("spdx", "http://spdx.org/rdf/terms#"),
    ("spdxlic", "http://spdx.org/licenses/"),
    ("codemeta", "https://codemeta.github.io/terms/#"),
    ("forgefed", "https://forgefed.org/ns#"),
    ("swh", "https://www.softwareheritage.org/data-model/"),
    ("ma", "http://www.w3.org/ns/ma-ont#"),
    ("gsso", "http://purl.obolibrary.org/obo/GSSO_"),
    ("homosaurus", "https://homosaurus.org/v4/"),
    ("fhir", "http://hl7.org/fhir/"),
    ("bio", "http://purl.org/vocab/bio/0.1/"),
    ("gx", "http://gedcomx.org/"),
    ("gxv", "http://gedcomx.org/v1/"),
    ("gn", "http://www.geonames.org/ontology#"),
    ("wd", "http://www.wikidata.org/entity/"),
    ("wdt", "http://www.wikidata.org/prop/direct/"),
    ("wikibase", "http://wikiba.se/ontology#"),
    ("p", "http://www.wikidata.org/prop/"),
    ("ps", "http://www.wikidata.org/prop/statement/"),
    ("wds", "http://www.wikidata.org/entity/statement/"),
    ("lexvo", "http://lexvo.org/id/"),
    ("lvont", "http://lexvo.org/ontology#"),
    ("glottolog", "https://glottolog.org/resource/languoid/id/"),
    ("ontolex", "http://www.w3.org/ns/lemon/ontolex#"),
    ("lime", "http://www.w3.org/ns/lemon/lime#"),
    (
        "fibo-fnd-acc-cur",
        "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/CurrencyAmount/",
    ),
    (
        "fibo-iso4217",
        "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/ISO4217-CurrencyCodes/",
    ),
    (
        "fibo-fnd-acc-ae",
        "https://spec.edmcouncil.org/fibo/ontology/FND/Accounting/AccountingEquity/",
    ),
    (
        "fibo-fnd-pas-ps",
        "https://spec.edmcouncil.org/fibo/ontology/FND/ProductsAndServices/ProductsAndServices/",
    ),
    (
        "fibo-fbc-fi-fi",
        "https://spec.edmcouncil.org/fibo/ontology/FBC/FinancialInstruments/FinancialInstruments/",
    ),
    (
        "fibo-fbc-pas-fpas",
        "https://spec.edmcouncil.org/fibo/ontology/FBC/ProductsAndServices/FinancialProductsAndServices/",
    ),
    ("mls", "http://www.w3.org/ns/mls#"),
    (
        "otelgenai",
        "https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/",
    ),
    ("faldo", "http://biohackathon.org/resource/faldo#"),
    ("so", "http://purl.obolibrary.org/obo/SO_"),
    ("ladm", "http://www.opengis.net/ont/ladm#"),
    ("cp", "http://inspire.ec.europa.eu/ont/cp#"),
];

/// The registry namespace IRI for a prefix, or `None`.
pub fn registry_iri(prefix: &str) -> Option<&'static str> {
    PREFIX_REGISTRY
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, ns)| *ns)
}

/// The registry as owned `(prefix, namespace)` pairs — the candidate prefix set the
/// canonical-Turtle renderer subsets to the prefixes a graph actually uses.
pub fn registry_pairs() -> Vec<(String, String)> {
    PREFIX_REGISTRY
        .iter()
        .map(|(p, ns)| ((*p).to_owned(), (*ns).to_owned()))
        .collect()
}

/// Build the longest-namespace-first `(namespace, prefix)` table used to shorten an
/// IRI to a CURIE. Stable sort keyed on descending namespace length; the tie-break
/// among equal-length namespaces is the registry's own insertion order. The table is
/// computed and sorted once, then returned as a borrow of the cached slice — this is
/// called per IRI (via `get_leg::curie`), so re-sorting the ~190-entry registry on
/// every call would be wasteful.
pub fn ns_to_prefix() -> &'static [(&'static str, &'static str)] {
    static TABLE: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut pairs: Vec<(&'static str, &'static str)> =
            PREFIX_REGISTRY.iter().map(|(p, ns)| (*ns, *p)).collect();
        pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.len()));
        pairs
    })
}

/// A SSSOM-safe identifier: a `prefix:local` CURIE when a registry namespace
/// prefixes the IRI, otherwise the bare absolute URI. Mirrors the historical
/// `_sssom_id ∘ curie`: an unmatched namespace yields the bare URI verbatim.
pub fn sssom_id(iri: &str, ns_to_prefix: &[(&str, &str)]) -> String {
    for (ns, prefix) in ns_to_prefix {
        if let Some(local) = iri.strip_prefix(*ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_unique_prefixes_and_namespaces() {
        let mut prefixes: Vec<&str> = PREFIX_REGISTRY.iter().map(|(p, _)| *p).collect();
        let n = prefixes.len();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(prefixes.len(), n, "duplicate prefix in registry");
    }

    #[test]
    fn registry_insertion_order_is_preserved() {
        // Order is load-bearing for CURIE tie-breaks: the first four entries are the
        // GMEOW-local grounding namespaces (gmeow, logic, lang, math), ahead of the
        // standard vocabularies (owl next). The grounding order is logic: < lang: < math:.
        assert_eq!(PREFIX_REGISTRY[0].0, "gmeow");
        assert_eq!(PREFIX_REGISTRY[1].0, "logic");
        assert_eq!(PREFIX_REGISTRY[2].0, "lang");
        assert_eq!(PREFIX_REGISTRY[3].0, "math");
        assert_eq!(PREFIX_REGISTRY[4].0, "owl");
    }

    #[test]
    fn curie_prefers_longest_namespace() {
        // `obi` (…/obo/OBI_) must win over `bfo` (…/obo/) for an OBI IRI — the
        // descending-namespace-length sort is what guarantees the most specific CURIE.
        let table = ns_to_prefix();
        assert_eq!(
            sssom_id("http://purl.obolibrary.org/obo/OBI_0000123", table),
            "obi:0000123"
        );
        assert_eq!(
            sssom_id("http://purl.obolibrary.org/obo/BFO_0000001", table),
            "bfo:BFO_0000001"
        );
        assert_eq!(
            sssom_id("http://purl.obolibrary.org/obo/RO_0002131", table),
            "ro:0002131"
        );
    }

    #[test]
    fn unmatched_iri_is_bare() {
        let table = ns_to_prefix();
        assert_eq!(
            sssom_id("http://unknown.example/x", table),
            "http://unknown.example/x"
        );
    }
}
