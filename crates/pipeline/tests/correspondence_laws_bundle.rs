// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deliverable A acceptance tests over the SHIPPED bundle.
//!
//! AC1 (positive): the `graph/correspondence-laws` named graph of the committed
//! `generated/dist/gmeow.gts` carries a discharged `logic:SectionLaw` claim
//! (`logic:lawClaimed = logic:SectionLaw`, `logic:lawDischargeVerdict =
//! logic:ObligationDischarged`) for each of the three SIOC mnemomorphic CompleteOver cells
//! (`mapSiocContainer`, `mapSiocHasContainer`, `mapSiocReplyOf`).
//!
//! AC3 (honest floor): `mapSiocTopic` carries NO discharged `logic:SectionLaw` claim in the
//! bundle, AND contributes no atom to the committed `generated/queries/sioc.put.rq` (its
//! `sioc:topic` inverse is absent — the put query has only the three recoverable branches).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences;
use gmeow_validate::store::dataset_from_paths;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GUFO: &str = "http://purl.org/nemo/gufo#";
const BFO: &str = "http://purl.obolibrary.org/obo/BFO_";
const RO: &str = "http://purl.obolibrary.org/obo/RO_";
const SUMO: &str = "https://www.ontologyportal.org/SUMO.owl#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SH: &str = "http://www.w3.org/ns/shacl#";
const DUL: &str = "http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#";
const IAO: &str = "http://purl.obolibrary.org/obo/IAO_";
const PATO: &str = "http://purl.obolibrary.org/obo/PATO_";
const YAMATO: &str = "http://www.hozo.jp/owl/YAMATO20210808.miz.owl#";
const OPENCYC: &str = "http://sw.opencyc.org/concept/";
const ONTOUML: &str = "https://w3id.org/ontouml#";
const QB: &str = "http://purl.org/linked-data/cube#";
const STATO: &str = "http://purl.obolibrary.org/obo/STATO_";
const OBCS: &str = "http://purl.obolibrary.org/obo/OBCS_";
const SIO: &str = "http://semanticscience.org/resource/SIO_";
const OBI: &str = "http://purl.obolibrary.org/obo/OBI_";
const ONTOLEX: &str = "http://www.w3.org/ns/lemon/ontolex#";
const LEXINFO: &str = "http://www.lexinfo.net/ontology/3.0/lexinfo#";
const WORDNET: &str = "https://globalwordnet.github.io/schemas/wn#";
const NIF: &str = "http://persistence.uni-leipzig.org/nlp2rdf/ontologies/nif-core#";
const OA: &str = "http://www.w3.org/ns/oa#";
const WIKIDATA: &str = "http://www.wikidata.org/entity/";
const LEXVO: &str = "http://lexvo.org/id/";
const GLOTTOLOG: &str = "https://glottolog.org/resource/languoid/id/";
const IANA_LANGUAGE_REGISTRY: &str = "https://www.iana.org/assignments/language-subtag-registry";
const QUDT: &str = "http://qudt.org/";
const SI_DIGITAL: &str = "https://si-digital-framework.org/SI/";
const OM2: &str = "http://www.ontology-of-units-of-measure.org/resource/om-2/";
const OM1: &str = "http://www.wurvoc.org/vocabularies/om-1.8/";
const SOSA: &str = "http://www.w3.org/ns/sosa/";
const IVOA_OBSCORE: &str = "http://www.ivoa.net/rdf/ObsCore#";
const LOINC: &str = "http://loinc.org/rdf/";
const OPENMATH_HTTP: &str = "http://www.openmath.org/cd/";
const OPENMATH_HTTPS: &str = "https://openmath.org/cd/";
const MATHLIB: &str = "https://leanprover-community.github.io/mathlib4_docs/";
const DLMF: &str = "https://dlmf.nist.gov/";
const OEIS: &str = "https://oeis.org/";
const RDF_TEST_MANIFEST: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#";
const OWL_TEST_ONTOLOGY: &str = "http://www.w3.org/2007/OWL/testOntology#";
// The process-model catalogs the prescription / enactment spine grounds onto. They are
// authored on the logic: owner surface (slices/grounding/logic/mappings/plan-enactment-bridges.ttl)
// because an external formalism has exactly one authoring home, so each one is a registered
// catalog family here exactly like the upper-ontology families above.
const PPLAN: &str = "http://purl.org/net/p-plan#";
const PROV: &str = "http://www.w3.org/ns/prov#";
const SCHEMA_ORG: &str = "https://schema.org/";
const OPMW: &str = "https://www.opmw.org/ontology/";
const BPMN: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL#";
const RO_CRATE: &str = "https://w3id.org/ro/crate/#";
const AIRFLOW: &str = "https://airflow.apache.org/concept/";
const CWL: &str = "https://w3id.org/cwl/cwl#";
const WDL: &str = "https://openwdl.org/concept/";
const TEMPORAL: &str = "https://temporal.io/concept/";
const NEXTFLOW: &str = "https://www.nextflow.io/concept/";
const OPENEHR_TASK_PLANNING: &str =
    "https://specifications.openehr.org/releases/PROC/latest/task_planning.html#";
const CORRESPONDENCE_LAWS_GRAPH: &str =
    "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The ground triples (subject, predicate, object as IRI/label strings) of ONE named graph of
/// the committed `gmeow.gts`, read through the kernel GTS reader.
fn graph_triples(graph_iri: &str) -> Vec<(String, String, String)> {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let g = purrdf::gts::read_graph(&bytes, true).expect("read_graph");
    let term = |id: usize| -> String {
        g.terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_else(|| format!("<term {id}>"))
    };
    let mut out = Vec::new();
    for &(s, p, o, gname) in &g.quads {
        let Some(gid) = gname else { continue };
        if term(gid) != graph_iri {
            continue;
        }
        out.push((term(s), term(p), term(o)));
    }
    out
}

/// Compile the three canonical grounding mapping directories and return the exact
/// content-addressed correspondence subjects they produce. Comparing frontend cell IRIs to the
/// bundle would be wrong because the shipped subjects are compiler-minted; comparing only counts
/// would let one stale extra mask one missing canonical correspondence.
fn canonical_grounding_correspondence_subjects() -> BTreeSet<String> {
    let root = repo_root();
    let mut paths = Vec::new();
    for slice in ["logic", "math", "lang"] {
        let mappings = root.join("slices/grounding").join(slice).join("mappings");
        for entry in std::fs::read_dir(&mappings)
            .unwrap_or_else(|error| panic!("read {}: {error}", mappings.display()))
        {
            let path = entry.expect("mapping directory entry").path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("ttl") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let dataset = dataset_from_paths(&paths).expect("canonical grounding mappings parse");
    let view = DslView::new(dataset.as_ref());
    transpile_correspondences(&view, &view)
        .expect("canonical grounding correspondences compile")
        .correspondences
        .into_iter()
        .filter(|correspondence| correspondence.grounding)
        .map(|correspondence| correspondence.iri)
        .collect()
}

/// Objects `o` such that `(subject, predicate, o)` is present.
fn objects_of<'a>(
    triples: &'a [(String, String, String)],
    subject: &str,
    predicate: &str,
) -> Vec<&'a str> {
    triples
        .iter()
        .filter(|(s, p, _)| s == subject && p == predicate)
        .map(|(_, _, o)| o.as_str())
        .collect()
}

/// Whether `claim` is a discharged `logic:SectionLaw` law-claim node in `triples`.
fn is_discharged_section_law(triples: &[(String, String, String)], claim: &str) -> bool {
    let claimed = objects_of(triples, claim, &format!("{LOGIC}lawClaimed"));
    let verdict = objects_of(triples, claim, &format!("{LOGIC}lawDischargeVerdict"));
    claimed.contains(&format!("{LOGIC}SectionLaw").as_str())
        && verdict.contains(&format!("{LOGIC}ObligationDischarged").as_str())
}

/// Correspondence subjects whose `logic:getLeg` is `cell` (the pattern-bearing cell IRI).
fn correspondences_for_cell<'a>(
    triples: &'a [(String, String, String)],
    cell: &str,
) -> Vec<&'a str> {
    triples
        .iter()
        .filter(|(_, p, o)| p == &format!("{LOGIC}getLeg") && o == cell)
        .map(|(s, _, _)| s.as_str())
        .collect()
}

/// Every discharged `logic:SectionLaw` claim reachable from a correspondence whose `getLeg`
/// is `cell` (via `logic:hasLawClaim`).
fn discharged_section_claims_for_cell(
    triples: &[(String, String, String)],
    cell: &str,
) -> Vec<String> {
    let mut claims = Vec::new();
    for corr in correspondences_for_cell(triples, cell) {
        for claim in objects_of(triples, corr, &format!("{LOGIC}hasLawClaim")) {
            if is_discharged_section_law(triples, claim) {
                claims.push(claim.to_owned());
            }
        }
    }
    claims
}

#[test]
fn shipped_bundle_carries_the_complete_grounding_correspondence_catalog() {
    let triples = graph_triples(CORRESPONDENCE_LAWS_GRAPH);
    let grounding_type = format!("{LOGIC}GroundingCorrespondence");
    let grounding: BTreeSet<String> = triples
        .iter()
        .filter(|(_, predicate, object)| predicate == RDF_TYPE && object == &grounding_type)
        .map(|(subject, _, _)| subject.clone())
        .collect();
    let source_predicate = format!("{LOGIC}sourceEndpoint");
    let target_predicate = format!("{LOGIC}targetEndpoint");
    let class_predicate = format!("{LOGIC}morphismClass");
    let kind_predicate = format!("{LOGIC}morphismKind");
    let preservation_predicate = format!("{LOGIC}preservationKind");
    let target_catalogs: &[(&str, &[&str], usize)] = &[
        ("gUFO", &[GUFO], 52),
        ("BFO", &[BFO], 13),
        ("RO", &[RO], 4),
        ("SUMO", &[SUMO], 24),
        ("OWL", &[OWL], 28),
        ("RDFS", &[RDFS], 5),
        ("SHACL", &[SH], 15),
        ("DUL", &[DUL], 6),
        ("IAO", &[IAO], 1),
        ("PATO", &[PATO], 1),
        ("YAMATO", &[YAMATO], 9),
        ("OpenCyc", &[OPENCYC], 6),
        ("OntoUML", &[ONTOUML], 9),
        ("RDF Data Cube", &[QB], 1),
        ("STATO", &[STATO], 5),
        ("OBCS", &[OBCS], 5),
        ("SIO", &[SIO], 1),
        ("OBI", &[OBI], 1),
        ("OntoLex", &[ONTOLEX], 5),
        ("LexInfo", &[LEXINFO], 3),
        ("Global WordNet", &[WORDNET], 6),
        ("NIF", &[NIF], 6),
        ("Web Annotation", &[OA], 4),
        ("Wikidata", &[WIKIDATA], 146),
        ("Lexvo", &[LEXVO], 2),
        ("Glottolog", &[GLOTTOLOG], 1),
        ("IANA Language Registry", &[IANA_LANGUAGE_REGISTRY], 1),
        ("QUDT", &[QUDT], 11),
        ("SI Digital Framework", &[SI_DIGITAL], 7),
        ("OM 2", &[OM2], 2),
        ("OM 1.8", &[OM1], 1),
        ("SOSA", &[SOSA], 1),
        ("IVOA ObsCore", &[IVOA_OBSCORE], 1),
        ("LOINC", &[LOINC], 1),
        ("OpenMath", &[OPENMATH_HTTP, OPENMATH_HTTPS], 14),
        ("Mathlib", &[MATHLIB], 11),
        ("DLMF", &[DLMF], 5),
        ("OEIS", &[OEIS], 3),
        ("RDF Test Manifest", &[RDF_TEST_MANIFEST], 2),
        ("OWL Test Ontology", &[OWL_TEST_ONTOLOGY], 2),
        ("P-Plan", &[PPLAN], 6),
        ("PROV-O", &[PROV], 4),
        ("schema.org HowTo", &[SCHEMA_ORG], 7),
        ("OPMW", &[OPMW], 3),
        ("BPMN", &[BPMN], 3),
        ("RO-Crate", &[RO_CRATE], 1),
        ("Apache Airflow", &[AIRFLOW], 2),
        ("CWL", &[CWL], 2),
        ("WDL", &[WDL], 2),
        ("Temporal", &[TEMPORAL], 2),
        ("Nextflow", &[NEXTFLOW], 2),
        ("openEHR Task Planning", &[OPENEHR_TASK_PLANNING], 3),
    ];
    let mut target_families = std::collections::BTreeMap::<&str, usize>::new();
    let mut source_namespaces = std::collections::BTreeMap::<&str, usize>::new();
    let mut endpoint_pairs = BTreeSet::new();

    for correspondence in &grounding {
        let sources = objects_of(&triples, correspondence, &source_predicate);
        let targets = objects_of(&triples, correspondence, &target_predicate);
        assert_eq!(
            sources.len(),
            1,
            "{correspondence}: exactly one source endpoint"
        );
        assert_eq!(
            targets.len(),
            1,
            "{correspondence}: exactly one target endpoint"
        );
        let source_namespace = if sources[0].starts_with(LOGIC) {
            "logic"
        } else if sources[0].starts_with(MATH) {
            "math"
        } else if sources[0].starts_with(LANG) {
            "lang"
        } else if sources[0].starts_with(GMEOW) {
            // The logic-owned DUL/IAO/OpenCyc bridge rows ground the shared
            // gmeow:InformationObject rather than minting a duplicate logic: class.
            "gmeow-shared"
        } else {
            panic!(
                "{correspondence}: grounding source is outside logic:, math:, lang:, and shared gmeow:, got {}",
                sources[0]
            );
        };
        *source_namespaces.entry(source_namespace).or_default() += 1;
        for predicate in [&class_predicate, &kind_predicate, &preservation_predicate] {
            assert_eq!(
                objects_of(&triples, correspondence, predicate).len(),
                1,
                "{correspondence}: semantic judgment {predicate} must ship exactly once"
            );
        }

        endpoint_pairs.insert((sources[0].to_owned(), targets[0].to_owned()));
        let matches: Vec<_> = target_catalogs
            .iter()
            .filter(|(_, namespaces, _)| {
                namespaces
                    .iter()
                    .any(|namespace| targets[0].starts_with(namespace))
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "{correspondence}: target must belong to exactly one registered catalog family: {}",
            targets[0]
        );
        *target_families.entry(matches[0].0).or_default() += 1;
    }

    for (namespace, minimum) in [
        ("gmeow-shared", 12),
        ("lang", 46),
        ("logic", 175),
        ("math", 188),
    ] {
        assert!(
            source_namespaces
                .get(namespace)
                .copied()
                .unwrap_or_default()
                >= minimum,
            "the shipped grounding surface fell below the {namespace} source-ownership ratchet of {minimum}: {source_namespaces:?}"
        );
    }
    for &(family, _, minimum) in target_catalogs {
        assert!(
            target_families.get(family).copied().unwrap_or_default() >= minimum,
            "the shipped grounding surface fell below the {family} target-family ratchet of {minimum}: {target_families:?}"
        );
    }
    for (source, target) in [
        (format!("{LOGIC}partOf"), format!("{BFO}0000050")),
        (format!("{LOGIC}overlaps"), format!("{RO}0002131")),
    ] {
        assert!(
            endpoint_pairs.contains(&(source.clone(), target.clone())),
            "the shipped OBO relation catalog is missing {source} -> {target}"
        );
    }

    let canonical = canonical_grounding_correspondence_subjects();
    let missing: Vec<_> = canonical.difference(&grounding).cloned().collect();
    let stale: Vec<_> = grounding.difference(&canonical).cloned().collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "the shipped correspondence graph must carry exactly the canonical logic:, math:, and \
         lang: grounding subjects; missing={missing:?}; stale={stale:?}"
    );
}

#[test]
fn ac1_shipped_bundle_discharges_section_law_for_the_three_sioc_cells() {
    let triples = graph_triples(CORRESPONDENCE_LAWS_GRAPH);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a `graph/correspondence-laws` named graph"
    );

    // Each of the three SIOC mnemomorphic CompleteOver cells has a correspondence carrying a
    // discharged section-law claim.
    for cell_local in ["mapSiocContainer", "mapSiocHasContainer", "mapSiocReplyOf"] {
        let cell = format!("{GMEOW}{cell_local}");
        let claims = discharged_section_claims_for_cell(&triples, &cell);
        assert!(
            !claims.is_empty(),
            "{cell_local}: expected a discharged logic:SectionLaw claim in the bundle, found none"
        );
    }

    // Whole-graph floor: at least three discharged SectionLaw claims exist (one per SIOC cell).
    let discharged_section_total = triples
        .iter()
        .filter(|(_s, p, o)| {
            p == &format!("{LOGIC}lawClaimed") && o == &format!("{LOGIC}SectionLaw")
        })
        .filter(|(claim, _, _)| is_discharged_section_law(&triples, claim))
        .map(|(claim, _, _)| claim.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        discharged_section_total.len() >= 3,
        "expected >= 3 discharged logic:SectionLaw claims in the bundle, got {}",
        discharged_section_total.len()
    );
}

#[test]
fn ac3_mapsioctopic_carries_no_discharged_section_law_and_no_put_atom() {
    // Part 1 (bundle): mapSiocTopic has no discharged section-law claim — the honest floor.
    let triples = graph_triples(CORRESPONDENCE_LAWS_GRAPH);
    let topic_cell = format!("{GMEOW}mapSiocTopic");
    let topic_claims = discharged_section_claims_for_cell(&triples, &topic_cell);
    assert!(
        topic_claims.is_empty(),
        "mapSiocTopic must carry NO discharged logic:SectionLaw claim (Unsupported put leg), \
         found: {topic_claims:?}"
    );

    // Part 2 (committed put query): mapSiocTopic's `sioc:topic` inverse contributes no atom.
    // The put query has ONLY the three recoverable branches.
    let put = std::fs::read_to_string(repo_root().join("generated/queries/sioc.put.rq"))
        .expect("committed sioc.put.rq");
    assert!(
        !put.contains("topic"),
        "sioc.put.rq must not re-assert any sioc:topic atom (mapSiocTopic is Unsupported)"
    );
    // The three recoverable branches are present (Thread/Container, has_container/container_of,
    // reply_of/has_reply) and nothing else.
    let required_atoms = [
        "?sthread a gmeow:Thread",
        "?scmsg gmeow:partOfThread ?scthread",
        "?srmsg gmeow:inReplyTo ?sparent",
        "sioc:has_container",
        "sioc:reply_of",
    ];
    for atom in required_atoms {
        assert!(
            put.contains(atom),
            "sioc.put.rq must carry the recoverable branch atom `{atom}`"
        );
    }
    // Exactly three CONSTRUCT-template atoms (the three recoverable branches).
    let construct_body = put
        .split_once("CONSTRUCT {")
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(body, _)| body)
        .expect("CONSTRUCT block");
    let template_atoms = construct_body
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with('.'))
        .count();
    assert_eq!(
        template_atoms, 3,
        "the put query must CONSTRUCT exactly the three recoverable atoms, got {template_atoms}\n\
         body:{construct_body}"
    );
}
