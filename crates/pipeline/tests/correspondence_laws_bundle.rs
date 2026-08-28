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

use gmeow_pipeline::catalog_families::{check_target_catalogs, load_catalog_families};

#[path = "support/authenticated_bundle.rs"]
mod authenticated_bundle;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
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
    authenticated_bundle::graph_triples(graph_iri)
}

/// Return the exact correspondence subjects from the authenticated mappings-stage product.
/// This preserves the producer-to-shipped transport parity proof without recompiling mappings
/// inside the test process.
fn canonical_grounding_correspondence_subjects() -> BTreeSet<String> {
    let fixture = gmeow_pipeline::fixture::stage_fixture(&repo_root(), 1, "stage-mappings")
        .expect("authenticated mappings-stage product; tests never produce it");
    let grounding_type = format!("{LOGIC}GroundingCorrespondence");
    authenticated_bundle::graph_triples_from(
        fixture.outcome.product.dataset(),
        CORRESPONDENCE_LAWS_GRAPH,
    )
    .into_iter()
    .filter(|(_, predicate, object)| predicate == RDF_TYPE && object == &grounding_type)
    .map(|(subject, _, _)| subject)
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
    // The registered target catalogs are ONTOLOGY DATA (`gmeow:CatalogFamily` rows in
    // dsl/mappings/catalog-families.ttl), read here through the SAME loader the mappings
    // stage gates with. Rust carries no family list, no namespace stem, and no ratchet
    // number: admitting a new external surface is an ontology edit, reviewable beside the
    // bridge cells it licenses, exactly as `gmeow:ProjectionVocabulary` does for the
    // guarded-residue ratchet.
    let families = load_catalog_families(&repo_root()).expect("catalog-family registry loads");
    let mut source_namespaces = std::collections::BTreeMap::<&str, usize>::new();
    let mut endpoint_pairs = BTreeSet::new();
    let mut shipped_targets: Vec<(String, String)> = Vec::new();

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
        shipped_targets.push((correspondence.clone(), targets[0].to_owned()));
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
    // Exactly-one-registered-family per target AND the per-family raise-only floor, in one
    // pass through the shared gate. An unregistered target is still a hard failure, and so
    // is a family that has lost rows below its `gmeow:catalogTargetMinimum`.
    let target_families = check_target_catalogs(
        &families,
        shipped_targets
            .iter()
            .map(|(cell, target)| (cell.as_str(), target.as_str())),
        "shipped gmeow.gts correspondence-laws graph",
    )
    .expect("every shipped grounding target is registered and every family holds its floor");
    assert_eq!(
        target_families.len(),
        families.len(),
        "the measured report must cover every registered family"
    );
    // The two OBO relation stems are read back OUT of the registry, so even this
    // spot-check names no namespace literal in Rust.
    let stem = |name: &str| -> String {
        let family = families
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no registered catalog family named {name}"));
        assert_eq!(
            family.namespaces.len(),
            1,
            "{name}: expected a single stem for the spot-check"
        );
        family.namespaces[0].clone()
    };
    for (source, target) in [
        (format!("{LOGIC}partOf"), format!("{}0000050", stem("BFO"))),
        (format!("{LOGIC}overlaps"), format!("{}0002131", stem("RO"))),
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
    let put = String::from_utf8(
        gmeow_pipeline::fixture::authenticated_artifact(
            &repo_root(),
            "stage-mappings",
            "generated/queries/sioc.put.rq",
        )
        .expect("authenticated sioc.put.rq; tests never produce it"),
    )
    .expect("authenticated sioc.put.rq is UTF-8");
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
