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

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
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
