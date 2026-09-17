// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonical repository root")
}

fn synthetic_manifest_dataset() -> Dataset {
    Dataset::parse_turtle(
            br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://blackcatinformatics.ca/gmeow> owl:versionInfo "9.9.9" .
gmeow:SyntheticClass a owl:Class ; rdfs:label "Synthetic class" .
logic:syntheticProperty a owl:ObjectProperty ; rdfs:label "Synthetic property" .
gmeow:CanonicalClass a logic:Class ; rdfs:label "Canonical class" .
logic:canonicalObjectProperty a logic:ObjectProperty ; rdfs:label "Canonical object property" .
gmeow:canonicalDatatypeProperty a logic:DatatypeProperty ; rdfs:label "Canonical datatype property" .
gmeow:canonicalAnnotationProperty a logic:AnnotationProperty ; rdfs:label "Canonical annotation property" .
gmeow:canonicalIndividual a logic:NamedIndividual ; rdfs:label "Canonical individual" .
"#, None,
            "synthetic term manifest graph")
        .expect("parse synthetic term manifest graph")
}

fn digest_for(turtle: &[u8], term: &str) -> String {
    let dataset = Dataset::parse_turtle(turtle, None, "synthetic digest graph")
        .expect("parse synthetic digest graph");
    let quads: Vec<RdfQuad> = dataset
        .inner()
        .owned_quads()
        .filter(|quad| quad.graph_name.is_none())
        .collect();
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (position, quad) in quads.iter().enumerate() {
        if let Some(key) = subject_key(&quad.subject) {
            index.entry(key).or_default().push(position);
        }
    }
    definition_digest(&quads, &index, term).expect("definition digest")
}

#[test]
fn manifest_fanout_iri_is_auto_derived() {
    // The declared graph IRI must equal what the superset helper derives from the
    // committed path, so the fold reconstructs the committed 4th column.
    assert_eq!(
        crate::stages::superset::rdf_fanout_graph_iri(TERM_MANIFEST_RDF_PATH).as_deref(),
        Some(TERM_MANIFEST_GRAPH_IRI)
    );
}

#[test]
fn every_gmeow_typed_term_gets_a_digest() {
    let dataset = synthetic_manifest_dataset();
    let terms = documented_terms(&dataset).expect("documented terms");
    assert_eq!(terms.len(), 7, "the synthetic graph declares seven terms");
    let records =
        resolve_term_records(&dataset, &BTreeMap::new()).expect("resolve synthetic manifest");
    let text = manifest_nquads(&records);
    for term in &terms {
        assert!(
            text.contains(&format!("<{term}> <{DEFINITION_DIGEST}>")),
            "missing definition digest for term {term}"
        );
    }
    // Every quad carries the fanout 4th column and a blake3 digest is present.
    assert!(text.contains(TERM_MANIFEST_GRAPH_IRI));
    assert!(text.contains("blake3:"));
}

#[test]
fn digest_excludes_provenance_predicates() {
    // A term's digest is over its defining triples only; adding a provenance
    // predicate (e.g. addedInVersion) must not change it.
    let term = "https://blackcatinformatics.ca/gmeow/SyntheticTerm";
    let base = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:SyntheticTerm a owl:Class ; rdfs:label "Stable definition" .
"#;
    let with_provenance = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:SyntheticTerm a owl:Class ;
    rdfs:label "Stable definition" ;
    gmeow:addedInVersion "9.9.9" ;
    gmeow:definitionDigest "blake3:prior" .
"#;
    let without = digest_for(base, term);
    let with = digest_for(with_provenance, term);
    assert_eq!(
        without, with,
        "provenance must not perturb definition identity"
    );
    assert!(without.starts_with("blake3:"));
}

#[test]
fn generated_manifest_is_not_a_stage_input() {
    let root = repo_root();
    let files = TermManifestStage::new()
        .input_files(&root)
        .expect("term-manifest inputs");
    assert!(
        files.contains(&root.join(TERM_RELEASE_AUTHORITY_PATH)),
        "the tracked previous-release authority must salt the stage"
    );
    assert!(
        !files.contains(&root.join(TERM_MANIFEST_RDF_PATH)),
        "the stage's ignored generated output must never feed its semantic history"
    );
}

fn one_term_dataset(release: &str, label: &str, authored_note: Option<&str>) -> Dataset {
    let authored = authored_note.map_or_else(String::new, |note| {
        format!(
            r#"
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.0.0" ;
        gmeow:entryNote "{note}" ] ;"#
        )
    });
    let turtle = format!(
        r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<https://blackcatinformatics.ca/gmeow> owl:versionInfo "{release}" .
gmeow:BoundaryTerm a owl:Class ;
    rdfs:label "{label}" ;{authored}
    gmeow:addedInVersion "1.0.0" .
"#
    );
    Dataset::parse_turtle(turtle.as_bytes(), None, "release-boundary term graph")
        .expect("parse release-boundary term graph")
}

#[test]
fn real_definition_change_persists_on_the_second_fixed_point() {
    let before = one_term_dataset("1.0.0", "Before release", None);
    let authority = resolve_term_records(&before, &BTreeMap::new()).expect("seed previous release");
    let after = one_term_dataset("1.1.0", "After release", None);

    let first = render_with_authority(&after, &authority).expect("first post-change render");
    let second = render_with_authority(&after, &authority).expect("warm fixed-point render");
    assert_eq!(
        first, second,
        "the materialized first output must not become the next run's semantic prior"
    );
    let text = String::from_utf8(first).expect("manifest is UTF-8 N-Quads");
    assert!(text.contains(HAS_CHANGELOG_ENTRY), "{text}");
    assert!(
        text.contains(&format!("<{ENTRY_VERSION}> \"1.1.0\"")),
        "{text}"
    );
    assert_eq!(
        text.matches(CHANGE_NOTE).count(),
        1,
        "one real changed term produces one persistent computed entry"
    );
}

/// Preserve release history across refusal, advancement and no-op publication on tiny data.
#[test]
fn release_authority_is_ordered_and_same_release_rewrites_fail_closed() {
    let guard = tempfile::tempdir().expect("release authority repo");
    let root = guard.path();
    let initial = one_term_dataset("1.0.0", "Accepted definition", None);
    let (release, terms, wrote) = publish_release_authority(root, true, || Ok(initial))
        .expect("bootstrap accepted release from a synthetic dataset");
    assert_eq!((release.as_str(), terms, wrote), ("1.0.0", 1, true));
    let path = root.join(TERM_RELEASE_AUTHORITY_PATH);
    let accepted = std::fs::read(&path).expect("accepted authority bytes");
    let error = publish_release_authority(root, true, || {
        panic!("a refused bootstrap must not invoke the dataset loader")
    })
    .expect_err("bootstrap must never overwrite an accepted authority")
    .to_string();
    assert!(error.contains("refusing to bootstrap"), "{error}");
    assert_eq!(
        std::fs::read(&path).expect("authority after bootstrap refusal"),
        accepted
    );
    assert_eq!(
        accepted.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "tracked authority uses one compact JSON record plus its terminal newline"
    );

    let accepted_authority = read_release_authority(root).expect("read accepted authority");
    let older = one_term_dataset("0.9.0", "Older source", None);
    let error = release_boundary_order(&older, &accepted_authority)
        .expect_err("an ontology older than its authority must fail closed")
        .to_string();
    assert!(error.contains("future evidence"), "{error}");

    let rewritten = one_term_dataset("1.0.0", "Unreleased rewrite", None);
    let error = publish_release_authority(root, false, || Ok(rewritten))
        .expect_err("same-release content must not move accepted authority")
        .to_string();
    assert!(error.contains("unchanged ontology release"), "{error}");
    assert_eq!(
        std::fs::read(&path).expect("authority after refusal"),
        accepted,
        "a refused refresh preserves every accepted byte"
    );

    let next = one_term_dataset("1.1.0", "Accepted next definition", None);
    let (release, terms, wrote) = publish_release_authority(root, false, || Ok(next))
        .expect("advance at newer release from a synthetic dataset");
    assert_eq!((release.as_str(), terms, wrote), ("1.1.0", 1, true));
    let advanced = std::fs::read(&path).expect("advanced authority bytes");
    assert_ne!(
        advanced, accepted,
        "a new release advances authority identity"
    );
    let unchanged = one_term_dataset("1.1.0", "Accepted next definition", None);
    let (_, _, wrote) = publish_release_authority(root, false, || Ok(unchanged))
        .expect("same-release fixed-point no-op");
    assert!(
        !wrote,
        "an identical same-release refresh must not rewrite bytes"
    );
    assert_eq!(
        std::fs::read(&path).expect("authority after no-op"),
        advanced,
        "same-release no-op preserves authority identity"
    );
}

/// Read the same accepted authority before and after writing synthetic output, preserving docs.
#[test]
fn fresh_and_warm_fixed_points_render_byte_identical_term_docs() {
    let guard = tempfile::tempdir().expect("synthetic clean worktree");
    let root = guard.path();
    let before = one_term_dataset("1.0.0", "Before release", None);
    let prior = resolve_term_records(&before, &BTreeMap::new())
        .expect("accepted synthetic previous release");
    let authority = ReleaseAuthority {
        schema: TERM_RELEASE_AUTHORITY_SCHEMA.to_string(),
        release: "1.0.0".to_string(),
        terms: prior.clone(),
    };
    validate_release_authority(&authority).expect("valid synthetic release authority");
    let authority_path = root.join(TERM_RELEASE_AUTHORITY_PATH);
    std::fs::create_dir_all(authority_path.parent().expect("authority parent"))
        .expect("authority directory");
    std::fs::write(
        &authority_path,
        serde_json::to_vec_pretty(&authority).expect("serialize synthetic authority"),
    )
    .expect("write synthetic authority");
    let after = one_term_dataset("1.1.0", "After release", None);

    // The tiny fixture contains only release evidence. Explicit synthetic data
    // supplies the terms; no test discovers or compiles repository sources.
    let materialized = root.join(TERM_MANIFEST_RDF_PATH);
    assert!(
        !materialized.exists(),
        "fresh synthetic worktree starts with no generated manifest"
    );

    let fresh_authority = read_release_authority(root).expect("authority before output");
    release_boundary_order(&after, &fresh_authority).expect("newer synthetic release");
    let fresh = render_with_authority(&after, &fresh_authority.terms)
        .expect("render the supplied one-term dataset");
    std::fs::create_dir_all(materialized.parent().expect("manifest parent"))
        .expect("first materialization directory");
    std::fs::write(&materialized, &fresh).expect("materialize first manifest output");
    let warm_authority = read_release_authority(root).expect("authority after output");
    release_boundary_order(&after, &warm_authority).expect("same synthetic release boundary");
    let warm = render_with_authority(&after, &warm_authority.terms)
        .expect("render again from the supplied dataset and accepted authority");
    assert_eq!(fresh, warm, "manifest bytes must be fixed-point stable");
    assert_eq!(
        std::fs::read(&materialized).expect("read first materialization"),
        fresh,
        "warm rendering must neither read back nor rewrite the first generated output"
    );

    let fresh_records = resolve_term_records(&after, &prior).expect("fresh resolved records");
    let warm_records = resolve_term_records(&after, &prior).expect("warm resolved records");
    assert_eq!(
        canonical_manifest_bytes(&manifest_nquads(&fresh_records))
            .expect("fresh canonical manifest"),
        fresh
    );
    assert_eq!(
        canonical_manifest_bytes(&manifest_nquads(&warm_records)).expect("warm canonical manifest"),
        warm
    );

    /// Render the synthetic term and changelog pages with distinct authored and computed notes.
    fn docs_pages(records: &BTreeMap<String, PriorTerm>) -> (String, String) {
        let term_iri = format!("{GMEOW}BoundaryTerm");
        let record = records.get(&term_iri).expect("BoundaryTerm record");
        let mut changelog = vec![gmeow_docs::model::DocChangelogEntry {
            version: "1.1.0".to_string(),
            note: Some("Authored release note.".to_string()),
            source: gmeow_docs::model::DocChangelogSource::Authored,
        }];
        changelog.extend(record.changed_versions.iter().map(|version| {
            gmeow_docs::model::DocChangelogEntry {
                version: version.clone(),
                note: Some(CHANGE_NOTE.to_string()),
                source: gmeow_docs::model::DocChangelogSource::Computed,
            }
        }));
        let term = gmeow_docs::model::DocTerm {
            iri: term_iri,
            curie: "gmeow:BoundaryTerm".to_string(),
            label: Some("After release".to_string()),
            category: gmeow_docs::model::DocTermCategory::Class,
            content_digest: record.digest.clone(),
            added_in_version: Some(record.first_seen.clone()),
            changelog,
            ..Default::default()
        };
        let slug = gmeow_docs::slug::term_slug(&term);
        let model = gmeow_docs::DocsModel {
            title: "Synthetic lifecycle".to_string(),
            version: "1.1.0".to_string(),
            terms: vec![term],
            ..Default::default()
        };
        (
            gmeow_docs::to_markdown(&model, &gmeow_docs::Page::Term(slug)),
            gmeow_docs::to_markdown(&model, &gmeow_docs::Page::Changelog),
        )
    }

    let (fresh_page, fresh_changelog) = docs_pages(&fresh_records);
    let (warm_page, warm_changelog) = docs_pages(&warm_records);
    assert_eq!(
        fresh_page.as_bytes(),
        warm_page.as_bytes(),
        "term documentation must be byte-identical before and after materialization"
    );
    assert_eq!(
        fresh_changelog.as_bytes(),
        warm_changelog.as_bytes(),
        "global changelog must be byte-identical before and after materialization"
    );
    for (surface, markdown) in [
        ("term page", &fresh_page),
        ("global changelog", &fresh_changelog),
    ] {
        let authored = markdown.find("(authored)").unwrap_or_else(|| {
            panic!("authored changelog identity must be visible on the {surface}")
        });
        let computed = markdown.find("(computed)").unwrap_or_else(|| {
            panic!("computed changelog identity must be visible on the {surface}")
        });
        assert!(
            authored < computed,
            "same-version authored and computed entries must remain distinct and deterministic \
                 on the {surface}"
        );
    }
}
