// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn store_from(ttl: &str) -> Store {
    parse_turtle_lenient(ttl.as_bytes()).expect("parse")
}

/// The `cqQueryFile` resolution boundary: repo-root-relative, `..`-free, and
/// under one of the content-addressed roots. Anything else is a hard fail —
/// see `apply_competency_query_text`.
#[test]
fn competency_query_paths_are_confined_to_the_hashed_roots() {
    assert!(is_competency_query_path("queries/competency/agents.rq"));
    assert!(is_competency_query_path(
        "slices/core/kernel/queries/competency/k.rq"
    ));
    // Outside the hashed roots: unhashed text would be served stale forever.
    assert!(!is_competency_query_path("dsl/competency/agents.rq"));
    assert!(!is_competency_query_path("generated/queries/agents.rq"));
    // Absolute, and `..` escapes that would pass a naive prefix test.
    assert!(!is_competency_query_path("/etc/passwd"));
    assert!(!is_competency_query_path("queries/../dsl/agents.rq"));
    assert!(!is_competency_query_path("slices/..\\dsl\\agents.rq"));
}

/// A `cqQueryFile` outside the boundary is an ERROR, not a silent skip and not
/// a tolerated read — the model build fails and names the offending path.
#[test]
fn competency_query_outside_the_hashed_roots_hard_fails() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let root = tmp.path().join("gmeow-cq-root-test");
    std::fs::create_dir_all(root.join("dsl")).expect("mkdir");
    // The file EXISTS and is readable — only its location is illegal, so this
    // proves the boundary itself rejects, not a dangling-path fallback.
    std::fs::write(root.join("dsl/escape.rq"), b"SELECT * {}").expect("write");

    let mut model = DocsModel {
        competencies: vec![DocCompetency {
            iri: "https://blackcatinformatics.ca/gmeow/cq/escape".to_owned(),
            query_file: Some("dsl/escape.rq".to_owned()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let err = apply_competency_query_text(&mut model, &root)
        .expect_err("a cqQueryFile outside the hashed roots must hard-fail");
    let msg = err.to_string();
    assert!(
        msg.contains("dsl/escape.rq") && msg.contains("outside the resolution contract"),
        "the error must name the offending path and the contract, got: {msg}"
    );

    // The legal location, same bytes, resolves.
    std::fs::create_dir_all(root.join("queries/competency")).expect("mkdir");
    std::fs::write(root.join("queries/competency/ok.rq"), b"SELECT * {}").expect("write");
    model.competencies[0].query_file = Some("queries/competency/ok.rq".to_owned());
    apply_competency_query_text(&mut model, &root).expect("a query under queries/ resolves");
    assert_eq!(
        model.competencies[0].query_text.as_deref(),
        Some("SELECT * {}")
    );
}

#[test]
fn thesis_sentence_detection_is_structural() {
    assert!(detect_thesis_sentence(
        "# Heading\n\nThis slice grounds the documentation standard in RDF."
    ));
    // Only headings / tables / lists — no prose sentence.
    assert!(!detect_thesis_sentence(
        "# Heading\n\n| a | b |\n| - | - |\n"
    ));
    assert!(!detect_thesis_sentence("- a bullet\n- another"));
    assert!(!detect_thesis_sentence(""));
}

#[test]
fn realized_state_table_detection_requires_every_row_marked() {
    // A table with a "Realized state" column where every row carries a marker
    // (realized / design-only / partial / built) is complete.
    let complete = "\
| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| a.md | charter | realized | x |
| b.md | charter | **design-only** — nothing yet | y |
| c.md | charter | partial | z |
";
    assert!(detect_realized_state_complete(complete));

    // One row with an empty realized-state cell → incomplete.
    let holey = "\
| Document | Genre | Realized state | Contents |
| --- | --- | --- | --- |
| a.md | charter | realized | x |
| b.md | charter |  | y |
";
    assert!(!detect_realized_state_complete(holey));

    // No realized-state table at all → not complete (a gated miss).
    assert!(!detect_realized_state_complete(
        "| Rule | Shape |\n| - | - |\n| r | s |\n"
    ));
}

#[test]
fn extract_terms_classifies_and_curies() {
    let ttl = r#"
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Animal a owl:Class ;
    rdfs:label "Animal" ;
    skos:definition "A living organism." .

gmeow:Cat a owl:Class ;
    rdfs:subClassOf gmeow:Animal ;
    rdfs:label "Cat" .

gmeow:hasOwner a owl:ObjectProperty ;
    rdfs:domain gmeow:Cat ;
    rdfs:range gmeow:Person ;
    rdfs:comment "Ownership relation." .
"#;
    let store = store_from(ttl);
    let terms = extract_terms(&store, "https://example.org/slice/zoo", None);

    let cat = terms.iter().find(|t| t.iri.ends_with("Cat")).unwrap();
    assert_eq!(cat.category, DocTermCategory::Class);
    assert_eq!(cat.curie, "gmeow:Cat");
    assert_eq!(cat.label.as_deref(), Some("Cat"));
    assert_eq!(cat.parents, vec![format!("{GMEOW_NS}Animal")]);
    assert_eq!(cat.owner_slice, "https://example.org/slice/zoo");

    let prop = terms.iter().find(|t| t.iri.ends_with("hasOwner")).unwrap();
    assert_eq!(prop.category, DocTermCategory::Property);
    assert_eq!(prop.definition.as_deref(), Some("Ownership relation."));
    assert_eq!(prop.domain, vec![format!("{GMEOW_NS}Cat")]);
    assert_eq!(prop.range, vec![format!("{GMEOW_NS}Person")]);

    let animal = terms.iter().find(|t| t.iri.ends_with("Animal")).unwrap();
    assert_eq!(animal.definition.as_deref(), Some("A living organism."));
}

/// A term whose taxonomy is authored in the CANONICAL `logic:` spelling —
/// with no `rdfs:` edge anywhere — still renders its parents.
///
/// This is the blinding regression: reading only `rdfs:subClassOf` /
/// `rdfs:subPropertyOf` gave a re-authored term an EMPTY parent list, which
/// silently emptied both its parent section and its `term_neighbourhood_svg`
/// hierarchy diagram (which projects exactly `DocTerm::parents`).
#[test]
fn canonical_logic_subsumption_edges_are_parents() {
    let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Animal a owl:Class ;
    rdfs:label "Animal" .

gmeow:Cat a owl:Class ;
    logic:subClassOf gmeow:Animal ;
    rdfs:label "Cat" .

gmeow:touches a owl:ObjectProperty ;
    rdfs:label "touches" .

gmeow:grooms a owl:ObjectProperty ;
    logic:subPropertyOf gmeow:touches ;
    rdfs:label "grooms" .
"#;
    let store = store_from(ttl);
    let terms = extract_terms(&store, "https://example.org/slice/zoo", None);

    let cat = terms.iter().find(|t| t.iri.ends_with("Cat")).unwrap();
    assert_eq!(
        cat.parents,
        vec![format!("{GMEOW_NS}Animal")],
        "a `logic:subClassOf` edge is a parent"
    );

    let grooms = terms.iter().find(|t| t.iri.ends_with("grooms")).unwrap();
    assert_eq!(
        grooms.parents,
        vec![format!("{GMEOW_NS}touches")],
        "a `logic:subPropertyOf` edge is a parent"
    );

    // The user-visible surface: the hierarchy diagram is non-empty.
    assert!(
        crate::svg::term_neighbourhood_svg(cat).contains("Animal"),
        "the neighbourhood diagram must draw the canonical parent edge"
    );
}

/// `read_central_mapping_sets` distinguishes an absent file (fine — slices
/// carry their own sets) from a present-but-unparsable one (hard fail, never
/// a silent empty that would drop every relocated linkage's `MappingSet`).
#[test]
fn central_mapping_sets_absent_ok_but_malformed_hard_fails() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let root = tmp.path().join("gmeow-mapsets-test");
    let dir = root.join("dsl").join("mappings");
    std::fs::create_dir_all(&dir).expect("mkdir");

    // Absent file ⇒ Ok(empty).
    assert!(
        read_central_mapping_sets(&root)
            .expect("absent mapping-sets.ttl must be Ok")
            .is_empty(),
        "an absent central mapping-sets.ttl yields no sets, not an error"
    );

    // Present but unparsable ⇒ hard fail (no silent empty).
    std::fs::write(
        dir.join("mapping-sets.ttl"),
        "this is @@@ definitely not { valid ] turtle <<< ;;;",
    )
    .expect("write malformed");
    let err = read_central_mapping_sets(&root)
        .expect_err("a malformed central mapping-sets.ttl must hard-fail, not return empty");
    assert!(matches!(err, DocsError::MappingSets(_)));
}

#[test]
fn non_gmeow_terms_are_skipped() {
    let ttl = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
<https://example.org/Foo> a owl:Class .
"#;
    let store = store_from(ttl);
    assert!(extract_terms(&store, "s", None).is_empty());
}

/// Stability derivation precedence: explicit `gmeow:termStability`
/// wins; else `owl:deprecated`; else the owner-slice tier default.
#[test]
fn stability_resolves_by_precedence() {
    let ttl = r#"
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:CoreDefault a owl:Class .
gmeow:ExtDefault  a owl:Class .
gmeow:Deprecated  a owl:Class ; owl:deprecated true .
gmeow:Explicit    a owl:Class ;
    owl:deprecated true ;
    gmeow:termStability gmeow:stabilityExperimental .
"#;
    let store = store_from(ttl);
    let core = extract_terms(&store, "s", Some(&SliceTier::Core));
    let by = |ts: &[DocTerm], suffix: &str| {
        ts.iter()
            .find(|t| t.iri.ends_with(suffix))
            .unwrap()
            .stability
    };
    // Core tier → Stable default.
    assert_eq!(by(&core, "CoreDefault"), DocTermStability::Stable);
    // owl:deprecated overrides the tier default.
    assert_eq!(by(&core, "Deprecated"), DocTermStability::Deprecated);
    // Explicit annotation beats owl:deprecated.
    assert_eq!(by(&core, "Explicit"), DocTermStability::Experimental);

    // Same terms under an extension tier → ExtDefault becomes Experimental.
    let ext = extract_terms(&store, "s", Some(&SliceTier::Extension));
    assert_eq!(by(&ext, "ExtDefault"), DocTermStability::Experimental);
}

/// Reified authored changelog entries are parsed from blank nodes, source-tagged,
/// and sorted by `(version, note, source)`; `addedInVersion` is the lowest literal.
#[test]
fn changelog_entries_parse_and_sort() {
    let ttl = r#"
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:Thing a owl:Class ;
    gmeow:addedInVersion "1.0.2" ;
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.1.0" ;
        gmeow:entryNote "Widened range." ] ;
    gmeow:hasChangelogEntry [
        a gmeow:ChangelogEntry ;
        gmeow:entryVersion "1.0.2" ] .
"#;
    let store = store_from(ttl);
    let terms = extract_terms(&store, "s", Some(&SliceTier::Core));
    let thing = terms.iter().find(|t| t.iri.ends_with("Thing")).unwrap();
    assert_eq!(thing.added_in_version.as_deref(), Some("1.0.2"));
    assert_eq!(
        thing.changelog,
        vec![
            DocChangelogEntry {
                version: "1.0.2".to_string(),
                note: None,
                source: DocChangelogSource::Authored,
            },
            DocChangelogEntry {
                version: "1.1.0".to_string(),
                note: Some("Widened range.".to_string()),
                source: DocChangelogSource::Authored,
            },
        ]
    );
}

#[test]
fn authored_and_computed_changelog_entries_remain_distinct_and_stable() {
    let term_iri = format!("{GMEOW_NS}BoundaryTerm");
    let authored = DocChangelogEntry {
        version: "1.0.0".to_string(),
        note: Some("Authored release note.".to_string()),
        source: DocChangelogSource::Authored,
    };
    let computed_same_release = DocChangelogEntry {
        version: "1.0.0".to_string(),
        note: Some("Definition changed".to_string()),
        source: DocChangelogSource::Computed,
    };
    let computed_later = DocChangelogEntry {
        version: "1.1.0".to_string(),
        note: Some("Definition changed".to_string()),
        source: DocChangelogSource::Computed,
    };
    let mut model = DocsModel {
        terms: vec![DocTerm {
            iri: term_iri.clone(),
            changelog: vec![authored.clone()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let manifest = BTreeMap::from([(
        term_iri,
        TermProvenance {
            digest: "blake3:0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            added_in_version: Some("1.0.0".to_string()),
            changelog: vec![computed_same_release.clone(), computed_later.clone()],
        },
    )]);

    apply_term_manifest(&mut model, manifest.clone());
    assert_eq!(
        model.terms[0].changelog,
        vec![authored.clone(), computed_same_release, computed_later],
        "source kind is part of identity: authored and computed records at one release both \
             remain visible"
    );
    let first = model.terms[0].changelog.clone();
    apply_term_manifest(&mut model, manifest);
    assert_eq!(
        model.terms[0].changelog, first,
        "reapplying the fixed-point manifest must not rewrite or duplicate authored history"
    );
}

/// [`extract_loss_targets`] is generic: it finds every subject carrying
/// BOTH `logic:preservationKind` and `logic:complexityClass` — not just a
/// hardcoded filename's individuals — and correctly skips a subject that
/// carries only one of the two predicates (an unrelated pedagogical
/// individual, mirroring `ex:mortalityRuleSet` in the real
/// `projection-loss-ledger.ttl`).
#[test]
fn extract_loss_targets_finds_rows_and_skips_partial_subjects() {
    let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/demo/> .

ex:notARow a gmeow:InformationObject ;
    rdfs:label "carries only preservationKind, not a loss row"@x-gmeow-english ;
    logic:preservationKind logic:ValidationOnly .

ex:elProjectionReport a gmeow:InformationObject ;
    rdfs:label "OWL-EL projection of the demo rule set"@x-gmeow-english ;
    logic:preservationKind logic:SoundUnderApproximation ;
    logic:complexityClass "EL -> PTIME" .
"#;
    let store = store_from(ttl);
    let rows = extract_loss_targets(&store, "https://example.org/slice/demo");
    assert_eq!(rows.len(), 1, "only the fully-attributed subject is a row");
    let row = &rows[0];
    assert_eq!(row.target, "elProjectionReport");
    assert_eq!(
        row.label.as_deref(),
        Some("OWL-EL projection of the demo rule set")
    );
    assert_eq!(row.preservation_kind, "SoundUnderApproximation");
    assert_eq!(row.complexity_class, "EL -> PTIME");
    assert_eq!(row.slice, "https://example.org/slice/demo");
}

#[test]
fn extract_seams_reads_labels_directions_terms_and_docs() {
    let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .

<https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice .

<https://blackcatinformatics.ca/gmeow/seam/denotation>
    a gmeow:Seam ;
    rdfs:label "Denotation seam"@x-gmeow-english ;
    skos:definition "The lang -> logic seam."@x-gmeow-english ;
    gmeow:seamDirection [
        gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
        gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>
    ] ;
    gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;
    gmeow:seamOwningDoc "LANG-MEANING.md" .
"#;
    let store = store_from(ttl);
    assert!(is_grounding_slice(
        &store,
        "https://blackcatinformatics.ca/gmeow/slices/logic"
    ));
    assert!(!is_grounding_slice(
        &store,
        "https://blackcatinformatics.ca/gmeow/slices/lang"
    ));

    let seams = extract_seams(&store);
    assert_eq!(seams.len(), 1);
    let seam = &seams[0];
    assert_eq!(
        seam.iri,
        "https://blackcatinformatics.ca/gmeow/seam/denotation"
    );
    assert_eq!(seam.label.as_deref(), Some("Denotation seam"));
    assert_eq!(seam.definition.as_deref(), Some("The lang -> logic seam."));
    assert_eq!(
        seam.directions,
        vec![DocSeamDirection {
            from: "https://blackcatinformatics.ca/gmeow/slices/lang".to_string(),
            to: "https://blackcatinformatics.ca/gmeow/slices/logic".to_string(),
        }]
    );
    assert_eq!(
        seam.carrying_terms,
        vec![
            "https://blackcatinformatics.ca/lang/denotationKind".to_string(),
            "https://blackcatinformatics.ca/lang/denotationTarget".to_string(),
        ]
    );
    assert_eq!(seam.owning_docs, vec!["LANG-MEANING.md".to_string()]);
}

/// Build a bare in-memory [`ArtifactRecord`] for a Turtle example, mirroring
/// the shape [`extract_worked_instances`] reads (`logical_path` is the only
/// field it consults; the rest are filler).
fn example_artifact(logical_path: &str, ttl: &str) -> ArtifactRecord {
    ArtifactRecord {
        role: ArtifactRole::Example,
        logical_path: logical_path.to_string(),
        media_type: "text/turtle".to_string(),
        raw_digest: String::new(),
        semantic_digest: None,
        content: ttl.as_bytes().to_vec(),
    }
}

#[test]
fn example_and_fixture_coverage_includes_used_properties_without_counting_literal_mentions() {
    let artifact = example_artifact(
        "examples/context.ttl",
        r#"@prefix ex: <urn:context:> .
               @prefix logic: <https://blackcatinformatics.ca/logic/> .
               @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
               ex:c a logic:AttributedContext ;
                   logic:contextWorld ex:world ;
                   gmeow:accordingTo ex:observer ;
                   ex:note "logic:notUsed" ."#,
    );
    let owner = "https://blackcatinformatics.ca/gmeow/slices/logic";
    let expected = vec![
        "gmeow:accordingTo",
        "logic:AttributedContext",
        "logic:contextWorld",
    ];
    assert_eq!(extract_example(&artifact, owner).terms_referenced, expected);
    assert_eq!(
        extract_fixture(&artifact, owner, DocFixtureKind::Wellformed, None).terms_referenced,
        expected,
    );
}

/// A bare in-memory `text/markdown` [`ArtifactRecord`] carrying `body`. Selected
/// by media type (like a real `design/*.md`), so [`DocMarkdownDocument::collect`]
/// treats it as a first-class document.
fn markdown_artifact(logical_path: &str, body: &str) -> ArtifactRecord {
    ArtifactRecord {
        role: ArtifactRole::Other(logical_path.to_string()),
        logical_path: logical_path.to_string(),
        media_type: "text/markdown".to_string(),
        raw_digest: format!("digest-{logical_path}"),
        semantic_digest: None,
        content: body.as_bytes().to_vec(),
    }
}

/// A hand-built [`SliceRecord`] carrying only a set of artifacts — the minimum
/// [`DocMarkdownDocument::collect`] reads (it consults `record.artifacts` only).
/// The manifest graph is an empty frozen dataset; the manifest view is filler.
fn record_with_artifacts(slice_iri: &str, artifacts: Vec<ArtifactRecord>) -> SliceRecord {
    SliceRecord {
        manifest: ManifestView {
            slice_iri: slice_iri.to_string(),
            label: None,
            title: None,
            creators: Vec::new(),
            identifier: None,
            tier: None,
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
        },
        manifest_graph: purrdf::RdfDatasetBuilder::new()
            .freeze()
            .expect("empty dataset freezes"),
        artifacts,
        slice_dir: std::path::PathBuf::from("/nonexistent/synthetic-slice"),
    }
}

/// Item 1 (model ordering): `collect` selects every `text/markdown` artifact,
/// decodes it strictly, sorts by normalized logical path, and derives each
/// title from its first ATX H1 — regardless of artifact input order or role.
#[test]
fn collect_orders_documents_and_derives_titles() {
    // Deliberately out of sorted order on input; `design/*.md` carries the open
    // `ArtifactRole::Other` role, exercising media-type (not role) selection.
    let record = record_with_artifacts(
        "https://blackcatinformatics.ca/gmeow/slices/zoo",
        vec![
            markdown_artifact("docs.md", "# Zoo Guide\n\nProse.\n"),
            markdown_artifact("design/ARCHITECTURE.md", "# Architecture\n\n## Overview\n"),
            // A non-markdown artifact is ignored.
            example_artifact("examples/x.ttl", "ex:a a ex:B ."),
        ],
    );
    let docs = DocMarkdownDocument::collect(
        &record,
        "https://blackcatinformatics.ca/gmeow/slices/zoo",
        "zoo",
    )
    .expect("collect succeeds");
    assert_eq!(docs.len(), 2, "only the two markdown sources");
    assert_eq!(docs[0].source_path, "design/ARCHITECTURE.md");
    assert_eq!(docs[0].title, "Architecture");
    assert_eq!(docs[1].source_path, "docs.md");
    assert_eq!(docs[1].title, "Zoo Guide");
    assert_eq!(docs[0].raw_digest, "digest-design/ARCHITECTURE.md");
}

/// Item 10a (hard-fail): an invalid-UTF-8 markdown artifact makes `collect`
/// return `Err(MarkdownUtf8)` naming the offending source path — no lossy
/// fallback.
#[test]
fn collect_hard_fails_on_invalid_utf8_naming_path() {
    let mut bad = markdown_artifact("design/BAD.md", "");
    bad.content = b"# X\n\xff\xfe\n".to_vec();
    let record =
        record_with_artifacts("https://blackcatinformatics.ca/gmeow/slices/zoo", vec![bad]);
    let err = DocMarkdownDocument::collect(
        &record,
        "https://blackcatinformatics.ca/gmeow/slices/zoo",
        "zoo",
    )
    .expect_err("invalid UTF-8 must hard-fail");
    match &err {
        DocsError::MarkdownUtf8 { source_path, .. } => {
            assert_eq!(source_path, "design/BAD.md");
        }
        other => panic!("expected MarkdownUtf8, got {other:?}"),
    }
    assert!(err.to_string().contains("design/BAD.md"));
}

/// Item 10b (hard-fail): two markdown artifacts whose logical paths NORMALIZE to
/// the same logical path (`./design/A.md` vs `design/A.md`) make `collect` return
/// `Err(MarkdownPathCollision)` naming the colliding path — one source can never
/// silently shadow the other.
#[test]
fn collect_hard_fails_on_normalized_path_collision() {
    let record = record_with_artifacts(
        "https://blackcatinformatics.ca/gmeow/slices/zoo",
        vec![
            markdown_artifact("design/A.md", "# First\n"),
            // Distinct input path, identical after `./`-stripping normalization.
            markdown_artifact("./design/A.md", "# Second\n"),
        ],
    );
    let err = DocMarkdownDocument::collect(
        &record,
        "https://blackcatinformatics.ca/gmeow/slices/zoo",
        "zoo",
    )
    .expect_err("normalized-path collision must hard-fail");
    match &err {
        DocsError::MarkdownPathCollision { source_path, .. } => {
            assert_eq!(source_path, "design/A.md");
        }
        other => panic!("expected MarkdownPathCollision, got {other:?}"),
    }
    assert!(err.to_string().contains("design/A.md"));
}

/// [`extract_worked_instances`] is generic: it finds every subject carrying
/// `math:hasDimension` — not just the individuals in the real
/// `measure-and-dimension.ttl` — resolves a `math:DerivedDimension`'s ℚ⁷
/// SI base-dimension exponent vector (sorted, negative numerators handled),
/// and honestly emits an EMPTY exponent vector (not a hard fail) for a
/// dimensionless subject whose dimension object carries no
/// `math:baseDimensionExponent` breakdown.
#[test]
fn extract_worked_instances_resolves_exponents_and_honest_dimensionless() {
    let ttl = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/demo/> .

ex:restEnergy a math:Quantity ;
    rdfs:label "rest energy"@x-gmeow-english ;
    math:hasDimension ex:energyDimension ;
    gmeow:unit <http://qudt.org/vocab/unit/J> ;
    gmeow:hasReferenceFrame gmeow:referenceFrameSI ;
    math:quantityValue "8.187e-14"^^xsd:double .

ex:energyDimension a math:DerivedDimension ;
    rdfs:label "energy dimension (M*L^2*T^-2)"@x-gmeow-english ;
    math:baseDimensionExponent ex:eMass1 , ex:eTimeMinus2 .

ex:eMass1 a math:DimensionExponent ;
    math:exponentOfDimension math:massDimension ;
    math:exponentNumerator 1 ; math:exponentDenominator 1 .
ex:eTimeMinus2 a math:DimensionExponent ;
    math:exponentOfDimension math:timeDimension ;
    math:exponentNumerator -2 ; math:exponentDenominator 1 .

ex:uniformProbability a math:ProbabilityMeasure ;
    rdfs:label "uniform probability measure"@x-gmeow-english ;
    math:hasDimension math:dimensionless .
"#;
    let store = store_from(ttl);
    let artifact = example_artifact("examples/demo.ttl", ttl);
    let mut rows = extract_worked_instances(&store, &artifact, "https://example.org/slice/demo");
    rows.sort_by(|a, b| a.subject.cmp(&b.subject));
    assert_eq!(rows.len(), 2, "both dimensioned subjects are picked up");

    let rest_energy = &rows[0];
    assert_eq!(rest_energy.subject, "restEnergy");
    assert_eq!(rest_energy.logical_path, "examples/demo.ttl");
    assert_eq!(rest_energy.slice, "https://example.org/slice/demo");
    assert_eq!(rest_energy.types, vec!["Quantity".to_string()]);
    assert_eq!(rest_energy.label.as_deref(), Some("rest energy"));
    assert_eq!(
        rest_energy.dimension_label.as_deref(),
        Some("energy dimension (M*L^2*T^-2)")
    );
    assert_eq!(
        rest_energy.unit.as_deref(),
        Some("http://qudt.org/vocab/unit/J")
    );
    assert_eq!(rest_energy.quantity_value.as_deref(), Some("8.187e-14"));
    assert_eq!(
        rest_energy.dimension_exponents,
        vec![
            DocDimExponent {
                base_dimension: "massDimension".to_string(),
                numerator: 1,
                denominator: 1,
            },
            DocDimExponent {
                base_dimension: "timeDimension".to_string(),
                numerator: -2,
                denominator: 1,
            },
        ],
        "sorted by base_dimension local name; negative numerator preserved"
    );
    assert!(rest_energy.turtle.contains("math:exponentNumerator -2"));
    assert!(
        rest_energy
            .turtle
            .contains("gmeow:unit <http://qudt.org/vocab/unit/J>")
    );

    let uniform = &rows[1];
    assert_eq!(uniform.subject, "uniformProbability");
    assert_eq!(
        uniform.dimension_exponents,
        Vec::new(),
        "dimensionless subject honestly renders an EMPTY exponent vector, not a hard fail"
    );
    assert_eq!(
        uniform.dimension_label, None,
        "math:dimensionless carries no local label in this file — an honest absence"
    );
    assert!(uniform.unit.is_none());
    assert!(uniform.quantity_value.is_none());
    assert!(
        !uniform.turtle.contains("baseDimensionExponent"),
        "no fabricated exponent breakdown for the dimensionless case"
    );
}
