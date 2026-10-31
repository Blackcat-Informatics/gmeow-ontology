// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Independently authored user DSL for this distribution seam. It contains
/// only the tiny fixture behavior below; no canonical source is loaded.
const TEST_DCAT_MAPPING: &str = r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix dcat: <http://www.w3.org/ns/dcat#> .
@prefix spdx: <http://spdx.org/rdf/terms#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:distribution-fixture:publish> a gm:ProjectionMapping ;
    gm:hasMappingPattern [ gm:anchor "collection" ; gm:atom (
        [ gm:subjectVar "collection" ; gm:predicate gm:corpusMember ; gm:objectVar "member" ]
        [ gm:subjectVar "member" ; gm:predicate gm:contentDigest ; gm:objectVar "hash" ]
        [ gm:optionalGroup ( [ gm:subjectVar "member" ; gm:predicate gm:sourceLocation ; gm:objectVar "location" ] ) ] ) ;
        gm:bind [ gm:bindVar "checksumValue" ; gm:bindExpr [ gm:exprOp gm:opStrDatatype ; gm:exprArgs (
            [ gm:exprOp gm:opStr ; gm:exprArgs ( [ gm:exprVar "hash" ] ) ] xsd:hexBinary ) ] ] ;
        gm:mint [ gm:bindVar "digestRecord" ; gm:bindExpr [ gm:exprOp gm:opIri ; gm:exprArgs (
            [ gm:exprOp gm:opConcat ; gm:exprArgs (
                [ gm:exprOp gm:opStr ; gm:exprArgs ( [ gm:exprVar "member" ] ) ] "-checksum-blake3-"
                [ gm:exprOp gm:opStr ; gm:exprArgs ( [ gm:exprVar "hash" ] ) ] ) ] ) ] ] ] ;
    gm:hasBinding [ gm:profile "dcat" ; gm:relation "<=" ; gm:templateAtoms (
        [ gm:tSubj "collection" ; gm:tPred rdf:type ; gm:tObjValue dcat:Dataset ]
        [ gm:tSubj "collection" ; gm:tPred dcat:distribution ; gm:tObj "member" ]
        [ gm:tSubj "member" ; gm:tPred rdf:type ; gm:tObjValue dcat:Distribution ]
        [ gm:tSubj "member" ; gm:tPred dcat:downloadURL ; gm:tObj "location" ]
        [ gm:tSubj "member" ; gm:tPred spdx:checksum ; gm:tObj "digestRecord" ]
        [ gm:tSubj "digestRecord" ; gm:tPred rdf:type ; gm:tObjValue spdx:Checksum ]
        [ gm:tSubj "digestRecord" ; gm:tPred spdx:algorithm ; gm:tObjValue spdx:checksumAlgorithm_blake3 ]
        [ gm:tSubj "digestRecord" ; gm:tPred spdx:checksumValue ; gm:tObj "checksumValue" ] ) ] .
"#;

/// Build only the typed native product and original transport for the authored
/// fixture. Missing-product controls retain the same valid catalog boundary.
fn synthetic_dcat_transport(include_query_product: bool) -> Vec<u8> {
    use gmeow_logic_compile::{
        ingest::DslView,
        projections::{correspondence, correspondence_frontend, sparql},
    };
    let dataset = purrdf::parse_dataset(TEST_DCAT_MAPPING.as_bytes(), "text/turtle", None).unwrap();
    let view = DslView::new(&dataset);
    let (program, analysis) =
        correspondence_frontend::transpile_correspondences_indexed(&view, &view, &[]).unwrap();
    let lowered = sparql::lower_sparql(&view, &analysis, &["dcat"]).unwrap();
    let profiles: Vec<_> = lowered.profiles.values().cloned().collect();
    let program = program
        .with_projection_bindings(lowered.bindings)
        .unwrap()
        .with_projection_profiles(profiles.clone())
        .unwrap();
    let native = correspondence::project_correspondence_dataset(&program).unwrap();
    // This fixture has no native source reifiers; every native quad is retained
    // in its required named correspondence graph.
    assert!(program.native_sources.is_empty());
    let quads: Vec<_> = native
        .owned_quads()
        .map(|quad| {
            quad.in_graph(purrdf::RdfTerm::iri(
                "https://blackcatinformatics.ca/gmeow/graph/correspondence-laws",
            ))
        })
        .collect();
    let native = purrdf::dataset_from_quads(&quads).unwrap();
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder.add_dataset(&native).unwrap();
    let mut blobs = vec![purrdf::gts_compose::BlobRow {
        data: purrdf::ustar::write_archive(&[]).unwrap(),
        media_type: "application/x-tar".to_owned(),
        rep: "cells-archive".to_owned(),
    }];
    if include_query_product {
        blobs.push(purrdf::gts_compose::BlobRow {
            data: purrdf::ustar::write_archive(&[(
                "dcat.projection.json".to_owned(),
                profiles[0].to_bytes(),
            )])
            .unwrap(),
            media_type: "application/x-tar".to_owned(),
            rep: "queries-archive".to_owned(),
        });
    }
    // gmeow-test-input: synthetic-only
    let emission = gmeow_gts_profile::emit_gmeow_gts(
        builder,
        blobs,
        Vec::new(),
        None,
        &gmeow_gts_profile::baseline_medium_plan(),
    )
    .expect("frame independent typed DCAT transport");
    assert!(emission.ingestion.declarations_omitted.is_empty());
    emission.bytes
}

fn synthetic_gts_with_dcat_query() -> Vec<u8> {
    synthetic_dcat_transport(true)
}

fn sample_entries() -> Vec<DistributionEntry> {
    vec![
        DistributionEntry {
            slug: "site".to_string(),
            rel_path: "dist/gmeow-docs/site".to_string(),
            blake3: blake3_of(b"site"),
            media_type: "text/html".to_string(),
        },
        DistributionEntry {
            slug: "okf".to_string(),
            rel_path: "dist/gmeow-docs/okf".to_string(),
            blake3: blake3_of(b"okf"),
            media_type: "application/json".to_string(),
        },
    ]
}

#[test]
fn distribution_blake3_is_deterministic_and_content_sensitive() {
    let mut tree = BTreeMap::new();
    tree.insert("a.txt".to_string(), b"hello".to_vec());
    tree.insert("b.txt".to_string(), b"world".to_vec());
    let d1 = distribution_blake3(&tree).expect("digest 1");
    let d2 = distribution_blake3(&tree).expect("digest 2");
    assert_eq!(d1, d2, "distribution_blake3 must be deterministic");
    assert!(
        d1.starts_with("blake3:"),
        "digest must carry the blake3: prefix: {d1}"
    );

    tree.insert("a.txt".to_string(), b"HELLO".to_vec());
    let d3 = distribution_blake3(&tree).expect("digest 3");
    assert_ne!(d1, d3, "changing content must change the digest");
}

#[test]
fn package_docs_dir_is_deterministic_and_content_sensitive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sub = tmp.path().join("sub");
    std::fs::create_dir_all(&sub).expect("create subdir");
    std::fs::write(tmp.path().join("a.txt"), b"hello").expect("write a.txt");
    std::fs::write(sub.join("b.txt"), b"world").expect("write sub/b.txt");

    let (archive1, digest1) = package_docs_dir(tmp.path()).expect("package 1");
    let (archive2, digest2) = package_docs_dir(tmp.path()).expect("package 2");
    assert_eq!(
        archive1, archive2,
        "package_docs_dir must be byte-reproducible"
    );
    assert_eq!(
        digest1, digest2,
        "package_docs_dir digest must be deterministic"
    );
    assert!(
        digest1.starts_with("blake3:"),
        "digest must carry the blake3: prefix: {digest1}"
    );

    std::fs::write(sub.join("b.txt"), b"WORLD").expect("mutate sub/b.txt");
    let (archive3, digest3) = package_docs_dir(tmp.path()).expect("package 3");
    assert_ne!(
        archive1, archive3,
        "changing content must change the archive bytes"
    );
    assert_ne!(digest1, digest3, "changing content must change the digest");
}

#[test]
fn package_docs_dir_fails_closed_on_missing_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("does-not-exist");
    let err = package_docs_dir(&missing).expect_err("a missing directory must hard-fail");
    assert!(
        format!("{err}").contains("missing"),
        "failure must name the missing directory: {err}"
    );
}

#[test]
fn package_docs_dir_fails_closed_on_empty_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let err = package_docs_dir(tmp.path()).expect_err("an empty directory must hard-fail");
    assert!(
        format!("{err}").contains("empty"),
        "failure must name the empty directory: {err}"
    );
}

#[test]
fn manifest_is_deterministic() {
    let gts_bytes = synthetic_gts_with_dcat_query();
    let entries = sample_entries();
    let m1 = build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest 1");
    let m2 = build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest 2");
    assert_eq!(
        m1, m2,
        "the release distribution manifest must be byte-reproducible"
    );
}

#[test]
fn manifest_carries_a_checksum_and_catalog_link_per_entry() {
    let gts_bytes = synthetic_gts_with_dcat_query();
    let entries = sample_entries();
    let manifest = build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("manifest");

    assert!(
        manifest.contains("http://spdx.org/rdf/terms#checksumValue"),
        "manifest must project at least one spdx:checksumValue triple:\n{manifest}"
    );
    assert!(manifest.contains(
        "<http://spdx.org/rdf/terms#algorithm> <http://spdx.org/rdf/terms#checksumAlgorithm_blake3>"
    ));
    for entry in &entries {
        let needle = format!(
            "\"{}\"^^<http://www.w3.org/2001/XMLSchema#hexBinary>",
            entry.blake3.trim_start_matches("blake3:")
        );
        assert!(
            manifest.contains(&needle),
            "manifest must carry the typed hexadecimal digest for {}: {needle}\n{manifest}",
            entry.slug
        );
        let catalog_iri = dist_iri(&entry.slug);
        assert!(
            manifest.contains(&format!("<{catalog_iri}>")),
            "manifest must link entry {} to its distribution-catalog subject {catalog_iri}:\n{manifest}",
            entry.slug
        );
    }
}

#[test]
fn manifest_fails_closed_without_a_bundled_dcat_query() {
    // Keep a valid independent native catalog and cells archive while withholding
    // the selected typed query archive. No other missing dependency masks refusal.
    let gts_bytes = synthetic_dcat_transport(false);
    let err = build_docs_distribution_manifest(&sample_entries(), &[], &gts_bytes)
        .expect_err("a bundle with no queries-archive blob must hard-fail");
    assert!(
        format!("{err}").contains("dcat.projection.json"),
        "failure must name the required typed DCAT product: {err}"
    );
}

#[test]
fn site_sub_asset_digests_ride_the_release_instance_on_the_sub_asset_subject() {
    use crate::stages::distribution_catalog::sub_asset_iri;
    let sub_assets = vec![DistributionEntry {
        slug: "mcp-wasm".to_string(),
        rel_path: "dist/gmeow-docs/site/assets/mcp/".to_string(),
        blake3: blake3_of(b"mcp-wasm"),
        media_type: "application/wasm".to_string(),
    }];
    let nt = release_instance_ntriples(&sample_entries(), &sub_assets).expect("release instance");
    let node = sub_asset_iri("mcp-wasm");
    // The sub-asset digest hangs off its site_sub_asset subject (NOT a dist_iri),
    // as a corpus member — so dcat.rq projects it exactly like a distribution.
    assert!(
        nt.contains(&triple(
            RELEASE_CORPUS_IRI,
            &iri(GMEOW_NS, "corpusMember"),
            &node
        )),
        "sub-asset must be a corpus member of the release: {nt}"
    );
    assert!(
        nt.contains(&format!(
            "<{node}> <{GMEOW_NS}contentDigest> \"{}\"^^<https://blackcatinformatics.ca/gmeow/blake3_256Digest> .",
            blake3_of(b"mcp-wasm").strip_prefix("blake3:").expect("computed asset digest")
        )),
        "sub-asset content digest must ride on its site_sub_asset subject: {nt}"
    );
}

#[test]
fn build_manifest_projects_site_sub_asset_digests_through_dcat_rq() {
    use crate::stages::distribution_catalog::sub_asset_iri;
    // Exercise the FULL projection (release_instance_ntriples -> dcat.rq), not just
    // the pre-projection input the sibling test checks: a dcat.rq regression that
    // dropped the SiteSubAsset corpus-member branch would publish no sub-asset digest
    // in the shipped manifest yet leave that pre-projection test green.
    let gts_bytes = synthetic_gts_with_dcat_query();
    let sub_assets = vec![DistributionEntry {
        slug: "mcp-wasm".to_string(),
        rel_path: "dist/gmeow-docs/site/assets/mcp/".to_string(),
        blake3: blake3_of(b"mcp-wasm"),
        media_type: "application/wasm".to_string(),
    }];
    let manifest = build_docs_distribution_manifest(&sample_entries(), &sub_assets, &gts_bytes)
        .expect("manifest with a site sub-asset");
    let node = sub_asset_iri("mcp-wasm");
    assert!(
        manifest.contains(&format!("<{node}>")),
        "the projected manifest must carry the site_sub_asset subject {node}:\n{manifest}"
    );
    assert!(
        manifest.contains(&format!(
            "\"{}\"^^<http://www.w3.org/2001/XMLSchema#hexBinary>",
            blake3_of(b"mcp-wasm")
                .strip_prefix("blake3:")
                .expect("computed asset digest")
        )),
        "the sub-asset digest must survive the dcat.rq projection into the shipped \
             manifest — a dropped SiteSubAsset row is a missing release digest:\n{manifest}"
    );
}

#[test]
fn release_instance_ntriples_is_sorted_and_deduped() {
    let nt = release_instance_ntriples(&sample_entries(), &[]).expect("release instance");
    let lines: Vec<&str> = nt.lines().collect();
    let mut sorted = lines.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        lines, sorted,
        "release instance N-Triples must be sorted+deduped"
    );
}

// ── read_distribution_matrix ────────────────────────────────────────────────

/// Fold the REAL [`crate::stages::distribution_catalog::build_distribution_catalog`]
/// output (a pure function of committed sources — no `make check` dependency) into a
/// minimal synthetic GTS snapshot carrying just that named graph, exactly as the
/// real bundle carries it.
fn synthetic_gts_with_catalog() -> Vec<u8> {
    let dataset = crate::stages::distribution_catalog::build_distribution_catalog()
        .expect("build distribution catalog");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder
        .add_dataset(&dataset)
        .expect("add catalog dataset to snapshot builder");
    // gmeow-test-input: synthetic-only
    {
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            Vec::new(),
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("frame the synthetic GTS snapshot");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    }
}

#[test]
fn read_distribution_matrix_over_a_synthetic_bundle_returns_all_nine_slugs() {
    use crate::stages::distribution_catalog::DISTRIBUTIONS;
    let gts_bytes = synthetic_gts_with_catalog();
    let rows = read_distribution_matrix(&gts_bytes).expect("read distribution matrix");
    let slugs: Vec<&str> = rows.iter().map(|r| r.slug.as_str()).collect();
    assert_eq!(
        slugs,
        vec![
            "console", "jsonld", "mdbook", "okf", "pdf", "pydantic", "site", "snippets", "yamlld"
        ],
        "matrix must carry exactly the nine declared distributions, sorted by slug"
    );

    // Every row's facets ARE the table's — no restated expectations here.
    for row in &rows {
        let declared = DISTRIBUTIONS
            .iter()
            .find(|d| d.slug == row.slug)
            .unwrap_or_else(|| panic!("matrix row {} is not a declared row", row.slug));
        assert_eq!(row.family, declared.family.slug(), "{}", row.slug);
        assert_eq!(row.media_type, declared.media_type, "{}", row.slug);
        assert_eq!(
            row.consumers,
            vec![declared.consumer.to_string()],
            "{}",
            row.slug
        );
        // The dropped set is DERIVED from the surface lattice, never authored — and
        // its SPELLING is read back off the emitter's own `capability_iri`, so a
        // renamed capability individual moves both sides at once. A local
        // capability→local-name table here would be a second authority: rename one
        // arm and this gate would go on asserting a spelling no emitter produces.
        let expected: Vec<String> = match declared.surface {
            Some(surface) => {
                let mut dropped: Vec<String> = gmeow_docs::formats::surface_capabilities(surface)
                    .dropped
                    .into_iter()
                    .map(|cap| {
                        gmeow_docs_catalog::identity::local_name(
                            &crate::stages::distribution_catalog::capability_iri(cap),
                        )
                    })
                    .collect();
                dropped.sort();
                dropped
            }
            None => Vec::new(),
        };
        assert_eq!(
            row.dropped_capabilities, expected,
            "{}: the matrix's dropped set must be the lattice's",
            row.slug
        );
    }

    // The console is a genuine row with a DERIVED, non-empty loss set — the property
    // that most easily regresses to "declared but empty".
    let console = rows
        .iter()
        .find(|r| r.slug == "console")
        .expect("console row");
    assert_eq!(console.family, "interactive-runtime");
    assert_eq!(console.media_type, "text/html");
    assert_eq!(
        console.consumers,
        vec!["consumerInteractiveConsole".to_string()]
    );
    assert_eq!(
        console.dropped_capabilities,
        vec![
            "capabilityCrossLinkFidelity".to_string(),
            "capabilitySearchIndex".to_string()
        ],
        "the console's dropped capabilities must be derived from the surface lattice"
    );

    let okf = rows.iter().find(|r| r.slug == "okf").expect("okf row");
    assert!(
        okf.dropped_capabilities.is_empty(),
        "serialization family declares no loss: {okf:?}"
    );
}

/// The EMITTER half of the concept-lattice pair, read back through the reader that was
/// written against the shape. Over the REAL catalog this returns the four concepts the
/// authored surface × capability incidence admits — an empty result would now mean the
/// emitter regressed, not that a lattice is optional.
#[test]
fn read_concept_lattice_over_a_synthetic_bundle_returns_the_derived_lattice() {
    let gts_bytes = synthetic_gts_with_catalog();
    let rows = gmeow_docs_catalog::read_concept_lattice(&gts_bytes).expect("read concept lattice");
    let rendered: Vec<(Vec<&str>, Vec<&str>)> = rows
        .iter()
        .map(|row| {
            (
                row.extent.iter().map(String::as_str).collect(),
                row.intent.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    // Rows sort by concept IRI, whose tail is the extent joined in surface order
    // (`site`, `site+mdbook`, `site+mdbook+console`, `site+mdbook+pdf+snippets+console`)
    // — so the rows arrive bottom-up along the lattice. Extents and intents come back
    // as ALPHABETICALLY sorted local names, which is the reader's own convention.
    assert_eq!(
        rendered,
        vec![
            (
                vec!["site"],
                vec![
                    "capabilityCrossLinkFidelity",
                    "capabilityDiagrams",
                    "capabilityInteractivity",
                    "capabilityLiveReasoning",
                    "capabilityLiveSparql",
                    "capabilitySearchIndex",
                ]
            ),
            (
                vec!["mdbook", "site"],
                vec![
                    "capabilityCrossLinkFidelity",
                    "capabilityDiagrams",
                    "capabilityInteractivity",
                    "capabilityLiveReasoning",
                    "capabilityLiveSparql",
                ]
            ),
            (
                vec!["console", "mdbook", "site"],
                vec![
                    "capabilityDiagrams",
                    "capabilityInteractivity",
                    "capabilityLiveReasoning",
                    "capabilityLiveSparql",
                ]
            ),
            (
                vec!["console", "mdbook", "pdf", "site", "snippets"],
                Vec::<&str>::new()
            ),
        ],
        "the emitted lattice must be the four concepts the incidence admits"
    );

    // Emitting the lattice must not widen the matrix: the matrix is exactly the
    // DECLARED table, no more. The console appears in both now — as an object of the
    // lattice and as the ninth shipped distribution — but a lattice concept node is
    // still never a matrix row.
    use crate::stages::distribution_catalog::DISTRIBUTIONS;
    let matrix = read_distribution_matrix(&gts_bytes).expect("read distribution matrix");
    assert_eq!(
        matrix.len(),
        DISTRIBUTIONS.len(),
        "the matrix must be exactly the declared table: {:?}",
        matrix.iter().map(|r| &r.slug).collect::<Vec<_>>()
    );
    assert!(
        matrix.iter().any(|row| row.slug == "console"),
        "the console is a shipped distribution and an object of the lattice"
    );
    for row in &rows {
        assert!(
            !matrix.iter().any(|m| m.slug == row.concept),
            "a formal concept must never surface as a distribution row: {}",
            row.concept
        );
    }
}

#[test]
fn read_distribution_matrix_fails_closed_without_the_catalog_graph() {
    let builder = purrdf::gts_compose::SnapshotBuilder::new();
    // gmeow-test-input: synthetic-only
    let gts_bytes = {
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"unrelated".to_vec(),
                media_type: "application/octet-stream".to_string(),
                rep: "not-catalog".to_string(),
            }],
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("frame a GTS snapshot with no distribution-catalog graph");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
    let err = read_distribution_matrix(&gts_bytes)
        .expect_err("a bundle with no distribution-catalog graph must hard-fail");
    assert!(
        format!("{err}").contains("distribution-catalog"),
        "failure must name the missing catalog graph: {err}"
    );
}

// ── verify_docs_distribution ────────────────────────────────────────────────

#[test]
fn verify_docs_distribution_passes_fresh_then_fails_on_tamper() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let docs_dir = tmp.path();
    let sample_dir = docs_dir.join("sample");
    std::fs::create_dir_all(&sample_dir).expect("mkdir sample");
    std::fs::write(sample_dir.join("a.txt"), b"hello").expect("write a.txt");

    let (_, digest) = package_docs_dir(&sample_dir).expect("package sample");
    let entries = vec![DistributionEntry {
        slug: "sample".to_string(),
        rel_path: "dist/gmeow-docs/sample".to_string(),
        blake3: digest.clone(),
        media_type: "text/plain".to_string(),
    }];
    let gts_bytes = synthetic_gts_with_dcat_query();
    let manifest =
        build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("build manifest");
    let manifest_dir = docs_dir.join("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    std::fs::write(manifest_dir.join("docs-manifest.ttl"), &manifest).expect("write manifest");

    let verdicts = verify_docs_distribution(docs_dir, None).expect("verify fresh package");
    assert_eq!(
        verdicts.len(),
        1,
        "expected exactly one verdict: {verdicts:?}"
    );
    assert_eq!(verdicts[0].slug, "sample");
    assert_eq!(verdicts[0].declared, digest);
    assert_eq!(verdicts[0].actual, digest);
    assert!(
        verdicts[0].ok,
        "a freshly packaged tree must verify clean: {:?}",
        verdicts[0]
    );

    // Flip a byte in the packaged tree — the recomputed digest must diverge and the
    // verdict must flip to failed, never silently pass.
    std::fs::write(sample_dir.join("a.txt"), b"HELLO").expect("tamper a.txt");
    let verdicts = verify_docs_distribution(docs_dir, None).expect("verify tampered package");
    assert_eq!(verdicts.len(), 1);
    assert_ne!(
        verdicts[0].actual, verdicts[0].declared,
        "tampering must change the recomputed digest"
    );
    assert!(
        !verdicts[0].ok,
        "a tampered tree must fail verification: {:?}",
        verdicts[0]
    );

    // `only` filtering on the sole declared slug still finds it.
    let filtered =
        verify_docs_distribution(docs_dir, Some("sample")).expect("verify filtered by slug");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].slug, "sample");
}

#[test]
fn verify_docs_distribution_fails_closed_on_unknown_only_slug() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let docs_dir = tmp.path();
    let sample_dir = docs_dir.join("sample");
    std::fs::create_dir_all(&sample_dir).expect("mkdir sample");
    std::fs::write(sample_dir.join("a.txt"), b"hello").expect("write a.txt");
    let (_, digest) = package_docs_dir(&sample_dir).expect("package sample");
    let entries = vec![DistributionEntry {
        slug: "sample".to_string(),
        rel_path: "dist/gmeow-docs/sample".to_string(),
        blake3: digest,
        media_type: "text/plain".to_string(),
    }];
    let gts_bytes = synthetic_gts_with_dcat_query();
    let manifest =
        build_docs_distribution_manifest(&entries, &[], &gts_bytes).expect("build manifest");
    let manifest_dir = docs_dir.join("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("mkdir manifest");
    std::fs::write(manifest_dir.join("docs-manifest.ttl"), &manifest).expect("write manifest");

    let err = verify_docs_distribution(docs_dir, Some("does-not-exist"))
        .expect_err("an unknown --format slug must hard-fail");
    assert!(
        format!("{err}").contains("does-not-exist"),
        "failure must name the unknown slug: {err}"
    );
}

#[test]
fn verify_docs_distribution_fails_closed_on_missing_manifest() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let err =
        verify_docs_distribution(tmp.path(), None).expect_err("a missing manifest must hard-fail");
    assert!(
        format!("{err}").contains("docs-manifest.ttl"),
        "failure must name the missing manifest: {err}"
    );
}

// ── read_build_serialization_tree (reference the build output, don't re-render) ──

#[test]
fn read_build_serialization_tree_carries_the_exact_build_output_bytes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let build_output = tmp.path().join("gmeow.jsonld");
    let bytes = b"{\"@context\": {}, \"@graph\": []}".to_vec();
    std::fs::write(&build_output, &bytes).expect("write fake build output");

    let tree = read_build_serialization_tree(&build_output, "gmeow.jsonld")
        .expect("read a present build output");
    assert_eq!(
        tree.get("gmeow.jsonld"),
        Some(&bytes),
        "the docs-distribution member must carry the build output's bytes byte-for-byte, \
             never a re-rendered copy: {tree:?}"
    );
    assert_eq!(
        tree.len(),
        1,
        "a single build output must yield exactly one tree member: {tree:?}"
    );
}

#[test]
fn read_build_serialization_tree_fails_closed_when_the_build_output_is_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("gmeow.yamlld");
    let err = read_build_serialization_tree(&missing, "gmeow.yamlld")
        .expect_err("a missing build output must hard-fail, never silently skip");
    let message = format!("{err}");
    assert!(
        message.contains("gmeow.yamlld") && message.contains("make build"),
        "failure must name the missing build output and point at `make build`: {message}"
    );
}
