// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    // crates/validate/src/self_desc.rs → repo root is three ancestors up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn load() -> SelfDescription {
    let path = default_self_desc_path(&repo_root());
    load_self_description(&path).expect("self-description parses")
}

#[test]
fn parses_core_metadata() {
    let sd = load();
    assert_eq!(sd.concept_doi, "10.67342/26w4o");
    assert_eq!(sd.version, "0.1.0");
    assert_eq!(sd.release_date, "2026-06-03");
    assert_eq!(sd.version_doi, None);
    assert_eq!(sd.doi(), "10.67342/26w4o");
    assert_eq!(sd.version_iri, "https://blackcatinformatics.ca/gmeow/0.1.0");
    assert_eq!(
        sd.license_uri,
        "https://creativecommons.org/licenses/by/4.0/"
    );
    assert_eq!(sd.homepage, "https://blackcatinformatics.ca/gmeow");
    assert_eq!(
        sd.repo_url,
        "https://github.com/Blackcat-Informatics/gmeow-ontology"
    );
    assert!(!sd.title.is_empty());
    assert!(!sd.description.is_empty());
}

#[test]
fn parses_depositor_and_registrant() {
    let sd = load();
    assert_eq!(sd.depositor_name, "Blackcat Informatics® Inc.");
    assert_eq!(sd.depositor_email, "root@blackcatinformatics.ca");
    assert_eq!(sd.registrant, "Blackcat Informatics® Inc.");
    assert_eq!(
        sd.registrant_wikidata.as_deref(),
        Some("http://www.wikidata.org/entity/Q140285712")
    );
}

#[test]
fn parses_contributors_orgs_first_then_persons() {
    let sd = load();
    assert_eq!(sd.contributors.len(), 2);
    let org = &sd.contributors[0];
    assert_eq!(org.kind, "organization");
    assert_eq!(org.name, "Blackcat Informatics® Inc.");
    assert_eq!(org.sequence, "first");
    assert_eq!(org.orcid, None);
    let person = &sd.contributors[1];
    assert_eq!(person.kind, "person");
    assert_eq!(person.name, "Patrick Audley");
    assert_eq!(person.sequence, "additional");
    assert_eq!(
        person.orcid.as_deref(),
        Some("https://orcid.org/0000-0003-4382-7625")
    );
    assert_eq!(person.given_name(), "Patrick");
    assert_eq!(person.surname(), "Audley");
}

#[test]
fn deposit_input_carries_config_and_sorted_alignments() {
    let sd = load();
    let deposit = deposit_input(&sd);
    assert_eq!(
        deposit.config.deposit_format,
        "Turtle; RDF/XML; N-Triples; JSON-LD; OWL; SHACL; GTS"
    );
    assert!(deposit.config.crossmark_enabled);
    let keys: Vec<&str> = deposit
        .config
        .alignment_targets
        .iter()
        .map(|t| t.key.as_str())
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "alignment targets must be key-sorted");
    // Every curated target is present and its related identifier is the namespace.
    assert_eq!(
        deposit.config.alignment_targets.len(),
        deposit_config::ALIGNMENT_TARGETS.len()
    );
    for target in &deposit.config.alignment_targets {
        assert_eq!(target.doi, None);
        assert_eq!(target.related_identifier, target.namespace);
    }
}

#[test]
fn deposit_and_lint_json_round_trip_through_serde() {
    let sd = load();
    let deposit_json = deposit_input_json(&sd).expect("deposit json");
    let back: DepositInput = serde_json::from_str(&deposit_json).expect("valid deposit json");
    assert_eq!(back.self_description.concept_doi, "10.67342/26w4o");

    let lint_json = lint_input_json(&sd, Some("cff".into()), None).expect("lint json");
    assert!(lint_json.contains("\"citation_cff\":\"cff\""));
    assert!(lint_json.contains("\"ontology_ttl\":null"));
}

#[test]
fn live_stamp_embeds_version() {
    let sd = load();
    let (timestamp, batch_id) = live_stamp(&sd);
    assert_eq!(timestamp.len(), 14);
    assert!(timestamp.bytes().all(|b| b.is_ascii_digit()));
    assert!(batch_id.starts_with("gmeow-0.1.0-"));
    assert!(batch_id.ends_with(&timestamp));
}

#[test]
fn iso_date_validation() {
    assert!(is_iso_date("2026-06-03"));
    assert!(is_iso_date("2024-02-29")); // leap day
    assert!(!is_iso_date("2023-02-29")); // not a leap year
    assert!(!is_iso_date("2026-13-01"));
    assert!(!is_iso_date("2026-6-3"));
    assert!(!is_iso_date("2026/06/03"));
    assert!(!is_iso_date("not-a-date"));
}
