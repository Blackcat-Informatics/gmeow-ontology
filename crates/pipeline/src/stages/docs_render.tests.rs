// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn docs_source_files_includes_new_inputs() {
    let root = repo_root();
    let files = docs_source_files(&root).expect("docs_source_files");
    let has_shapes_ttl = files
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl"));
    assert!(
        has_shapes_ttl,
        "docs_source_files must include at least one per-slice shapes.ttl"
    );
    let has_competency = files.iter().any(|p| {
        p.file_name().and_then(|n| n.to_str()) == Some("competency.ttl")
            && p.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|n| n.to_str())
                == Some("tests")
    });
    assert!(
        has_competency,
        "docs_source_files must include at least one per-slice tests/competency.ttl"
    );
    let shapes_dir = root.join("shapes");
    let has_root_shapes = files.iter().any(|p| {
        p.extension().and_then(|s| s.to_str()) == Some("ttl")
            && p.parent()
                .map(|parent| parent == shapes_dir)
                .unwrap_or(false)
    });
    assert!(
        has_root_shapes,
        "docs_source_files must include root shapes/*.ttl files"
    );
    // A CQ's `cqQueryFile` may resolve into a slice's own `queries/competency/`
    // tree (`slices/grounding/logic/queries/competency/named-parametric-paths.rq`)
    // or the shared repo-root tree (`queries/competency/citation-intents.rq`) —
    // both must be cache-salted (`gmeow_docs::model::apply_competency_query_text`).
    let has_slice_query = files
        .iter()
        .any(|p| p.ends_with("queries/competency/named-parametric-paths.rq"));
    assert!(
        has_slice_query,
        "docs_source_files must include at least one per-slice queries/competency/*.rq"
    );
    let root_query = root.join("queries/competency/citation-intents.rq");
    assert!(
        files.contains(&root_query),
        "docs_source_files must include the shared root queries/competency/*.rq tree"
    );
    // The notation-grammar exhibits (`gmeow_docs::model::DocGrammar`) — a
    // `grammars/*.ebnf` edit must bust the docs cache.
    let has_grammar = files
        .iter()
        .any(|p| p.ends_with("slices/grounding/lang/grammars/gmn.ebnf"));
    assert!(
        has_grammar,
        "docs_source_files must include the lang slice's grammars/*.ebnf files"
    );
    // A recursively-discovered per-slice `design/*.md` — the canonical-Markdown soundness
    // fix: editing a design doc must bust the docs cache, exactly as editing the
    // top-level `docs.md` does. Before this, only the top-level `docs.md` was
    // declared, so a `design/*.md` edit silently missed the key.
    let has_design_md = files.iter().any(|p| {
        p.extension().and_then(|s| s.to_str()) == Some("md")
            && p.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|n| n.to_str())
                == Some("design")
    });
    assert!(
        has_design_md,
        "docs_source_files must include recursively-discovered per-slice design/*.md sources"
    );
    // The vendored documentation site assets (`crates/docs/assets/**`) — the
    // `SnapshotStage` embeds them into the rendered site, so a
    // `maint-refresh-*-asset` swap must bust the docs cache (it does not change
    // any `.rs`, the only thing `GMEOW_BUILD_FINGERPRINT` folds).
    let has_vendored_asset = files
        .iter()
        .any(|p| p.ends_with("crates/docs/assets/query/gmeow_query_wasm_bg.wasm"));
    assert!(
        has_vendored_asset,
        "docs_source_files must include the vendored crates/docs/assets/** site assets"
    );
    let assets_root = root.join("crates/docs/assets");
    for derived in [
        "console/pkg/gmeow.gts",
        "console/node_modules/package/index.js",
        "console/smoke/node_modules/playwright/index.js",
        "console/blackcatinformatics-gmeow-console-0.2.0.tgz",
    ] {
        assert!(
            is_derived_docs_asset(&assets_root, &assets_root.join(derived)),
            "the producer-owned docs asset {derived} must be classified as derived"
        );
    }
    assert!(
        files
            .iter()
            .all(|path| !is_derived_docs_asset(&assets_root, path)),
        "docs_source_files must never hash producer-owned console package/install outputs"
    );
    // F4/F5: each interactive engine's native↔wasm witness-ATTESTATION is itself a
    // declared consumed input of this render leaf (it lives under crates/docs/assets/**
    // and is walked here), so re-blessing an attestation busts the docs cache and the
    // interactive preservation-kind is causally downstream of the proven parity — not
    // a decorative gate.
    //
    // The edge is unchanged in kind and narrower in extent: it used to also name
    // `assets/purrdf/WITNESS.describe.nt`, because `Capability::LiveSparql` was backed
    // by two engines. It is backed by the core segment alone now, so the core segment's
    // attestation carries the whole edge for live SPARQL. The describe property that
    // witness attested is still proven — by `crates/mcp/src/tests/witness_explore.rs`,
    // against the engine that now answers it.
    for witness in [
        "crates/docs/assets/query/WITNESS.describe.nt",
        "crates/docs/assets/validate/WITNESS.validate.json",
        "crates/docs/assets/reason/WITNESS.reason.nq",
        "crates/docs/assets/gmn/WITNESS.gmn1.txt",
        "crates/docs/assets/mcp-core/WITNESS.core-deferral.json",
        "crates/docs/assets/mcp/WITNESS.mcp.json",
    ] {
        assert!(
            files.iter().any(|p| p.ends_with(witness)),
            "docs_source_files must consume the interactive witness-attestation {witness} \
                 (the F4/F5 attestation→capability dataflow edge)"
        );
    }
}

/// Supply the same final native report publication as a real producer, over
/// a tiny explicit Report. Terminal JSON exists but the consumer never reads it.
fn native_report_product(stage_id: &str, report: &gmeow_errors::Report) -> StageProduct {
    let owner = match stage_id {
        "stage-validate" => crate::bundle::DiagnosticReportOwner::Validate,
        "stage-compile-logic" => crate::bundle::DiagnosticReportOwner::CompileLogic,
        _ => panic!("only the two declared diagnostic producers are valid here"),
    };
    crate::bundle::diagnostic_test_support::product(owner, report.clone())
}

/// A minimal synthetic [`gmeow_errors::Finding`] with the given code/
/// category/term-location/slice-attribution — a plain builder, since (unlike
/// the retired `DiagNode` lane) the native report join needs no ledger
/// fingerprint.
fn synthetic_finding(
    code: &str,
    category: gmeow_errors::FindingCategory,
    term_iri: Option<&str>,
    slice_iri: Option<&str>,
) -> gmeow_errors::Finding {
    let mut finding = gmeow_errors::Finding::new(
        gmeow_errors::Severity::Warning,
        code,
        format!("synthetic finding for {code}"),
    )
    .with_category(category);
    if let Some(term_iri) = term_iri {
        finding.add_location(gmeow_errors::Location::new(
            None,
            None,
            None,
            Some(term_iri.to_string()),
        ));
    }
    if let Some(slice_iri) = slice_iri {
        finding
            .attributions
            .push(gmeow_errors::DiagnosticAttribution {
                slice_iri: slice_iri.to_string(),
                role: "focus-origin".to_string(),
                evidence: None,
            });
    }
    finding
}

#[test]
fn diagnostics_digest_joins_on_documented_term_attribution_not_abox_focus() {
    // The PRIMARY join leg: a SHACL-shaped finding whose FOCUS is an ABox data
    // individual (names no documented term) but whose `documented_terms` carries
    // the constrained property (a documented term) joins by_term on the PROPERTY,
    // never on the focus. This is the exact real-repo shape: the MinCount
    // violations' focus nodes are fixture individuals; their constrained
    // `gmeow:hasReferenceFrame` property is the documented term the panel lights up.
    let property = "https://blackcatinformatics.ca/gmeow/hasReferenceFrame";
    let mut known_terms: BTreeSet<String> = BTreeSet::new();
    known_terms.insert(property.to_string());

    let mut shacl_report = gmeow_errors::Report::new("shacl");
    let mut finding = synthetic_finding(
        "shacl.MinCountConstraintComponent",
        gmeow_errors::FindingCategory::DataShapeViolation,
        // The focus node is an ABox fixture individual — NOT a documented term.
        Some("https://blackcatinformatics.ca/gmeow/fixtureRagaYamanImprovised1975"),
        None,
    );
    finding = finding.with_documented_term(property);
    shacl_report.findings.push(finding);

    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-validate".to_string(),
        native_report_product("stage-validate", &shacl_report),
    );
    upstream.insert(
        "stage-compile-logic".to_string(),
        native_report_product(
            "stage-compile-logic",
            &gmeow_errors::Report::new("logic-compile"),
        ),
    );

    let digest =
        diagnostics_digest_from_upstream(&upstream, &known_terms, &[]).expect("digest folds");
    // Joined on the documented PROPERTY, exactly once, via the primary leg.
    assert_eq!(
        digest.by_term.get(property).map(Vec::len),
        Some(1),
        "the finding joins by_term on its documented constrained property"
    );
    assert_eq!(
        digest.by_term[property][0].code,
        "shacl.MinCountConstraintComponent"
    );
    // The ABox focus individual never fabricates a by_term key.
    assert!(
        !digest
            .by_term
            .contains_key("https://blackcatinformatics.ca/gmeow/fixtureRagaYamanImprovised1975"),
        "the ABox focus node must never enter by_term"
    );
}

#[test]
fn diagnostics_digest_joins_term_and_slice_and_hard_fails_on_missing_upstream() {
    let term_iri = "https://blackcatinformatics.ca/gmeow/Cat";
    let mut known_terms: BTreeSet<String> = BTreeSet::new();
    known_terms.insert(term_iri.to_string());

    let mut shacl_report = gmeow_errors::Report::new("shacl");
    shacl_report.findings.push(synthetic_finding(
        "shacl.MinCountConstraintComponent",
        gmeow_errors::FindingCategory::DataShapeViolation,
        Some(term_iri),
        Some("https://blackcatinformatics.ca/gmeow/slices/core"),
    ));
    // A finding whose location names no KNOWN term: honestly absent from
    // `by_term`, never a fuzzy/heuristic join.
    shacl_report.findings.push(synthetic_finding(
        "shacl.NodeKindConstraintComponent",
        gmeow_errors::FindingCategory::DataShapeViolation,
        Some("https://example.test/not-a-known-term"),
        None,
    ));

    let mut compile_report = gmeow_errors::Report::new("logic-compile");
    compile_report.findings.push(synthetic_finding(
        "logic-compile.UNKNOWN_PROFILE",
        gmeow_errors::FindingCategory::ModelingDisciplineViolation,
        None,
        None,
    ));

    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-validate".to_string(),
        native_report_product("stage-validate", &shacl_report),
    );
    upstream.insert(
        "stage-compile-logic".to_string(),
        native_report_product("stage-compile-logic", &compile_report),
    );

    let rule = gmeow_docs::model::ConstraintRule {
        code: "shacl.MinCountConstraintComponent".to_string(),
        slug: "shacl-min-count-constraint-component".to_string(),
        category: "https://blackcatinformatics.ca/gmeow/FindingDataShapeViolation".to_string(),
        severity: "binding".to_string(),
        help_uri: "https://blackcatinformatics.ca/gmeow/rules#shacl-min-count-constraint-component"
            .to_string(),
        label: None,
        definition: None,
        applies_to_terms: Vec::new(),
        formalizes: None,
    };

    let digest =
        diagnostics_digest_from_upstream(&upstream, &known_terms, std::slice::from_ref(&rule))
            .expect("digest folds from synthetic upstream");
    assert_eq!(digest.total, 3, "3 findings folded across both producers");
    assert_eq!(
        digest.by_term.get(term_iri).map(Vec::len),
        Some(1),
        "only the finding whose location names a KNOWN term joins by_term"
    );
    let joined = &digest.by_term[term_iri][0];
    assert_eq!(joined.code, "shacl.MinCountConstraintComponent");
    assert_eq!(joined.help_uri.as_deref(), Some(rule.help_uri.as_str()));
    assert_eq!(
        digest
            .by_slice
            .get("https://blackcatinformatics.ca/gmeow/slices/core")
            .map(Vec::len),
        Some(1)
    );
    // The unattributed / unresolved-code findings never fabricate a slice or help_uri.
    assert!(
        !digest
            .by_slice
            .values()
            .flatten()
            .any(|f| f.code == "logic-compile.UNKNOWN_PROFILE" && f.help_uri.is_some()),
        "an unresolved code must never carry a fabricated help_uri"
    );

    // Missing EITHER declared upstream product hard-fails (never a silent empty digest).
    let mut only_validate: BTreeMap<String, StageProduct> = BTreeMap::new();
    only_validate.insert(
        "stage-validate".to_string(),
        native_report_product("stage-validate", &gmeow_errors::Report::new("shacl")),
    );
    assert!(
        diagnostics_digest_from_upstream(&only_validate, &known_terms, &[]).is_err(),
        "missing stage-compile-logic must hard-fail"
    );
    assert!(
        diagnostics_digest_from_upstream(&BTreeMap::new(), &known_terms, &[]).is_err(),
        "missing both upstream products must hard-fail"
    );

    // A declared upstream product present but MISSING the native publication (e.g. a
    // stale/partial product) hard-fails too — never silently treated as empty.
    let mut missing_publication: BTreeMap<String, StageProduct> = BTreeMap::new();
    missing_publication.insert(
        "stage-validate".to_string(),
        StageProduct::from_artifacts("stage-validate", BTreeMap::new()),
    );
    missing_publication.insert(
        "stage-compile-logic".to_string(),
        native_report_product(
            "stage-compile-logic",
            &gmeow_errors::Report::new("logic-compile"),
        ),
    );
    assert!(
        diagnostics_digest_from_upstream(&missing_publication, &known_terms, &[]).is_err(),
        "a stage-validate product missing the native diagnostic publication must hard-fail"
    );
}

/// Build a synthetic `stage-mappings` product whose `GRAPH_PROJECTION_LEDGER`
/// named graph carries EXACTLY the given Turtle `body`, parsed and re-rooted
/// via [`crate::stages::carrier::parse_into_graph`] — the SAME producer-
/// attached-graph lane the real `stage-mappings` stage rides
/// (`mappings::run`'s own `parse_into_graph(..., GRAPH_PROJECTION_LEDGER)`
/// call), so this test exercises the real production read path
/// (`term_loss_digest_from_upstream` → `producer_graph`) rather than a stub.
fn mappings_product_with_ledger(turtle_body: &str) -> StageProduct {
    let dataset = crate::stages::carrier::parse_into_graph(
        turtle_body.as_bytes(),
        "text/turtle",
        crate::stages::carrier::GRAPH_PROJECTION_LEDGER,
    )
    .expect("parse synthetic projection-ledger turtle");
    StageProduct::from_artifacts_over("stage-mappings", dataset, BTreeMap::new())
}

#[test]
fn term_loss_digest_joins_property_path_rows_and_hard_fails_on_missing_upstream() {
    use gmeow_docs::model::{DocShape, DocTerm, DocTermCategory};

    // (a) resolves via a DocShape whose shape_iri matches the ledger row and
    // whose target_term names a documented term.
    let shape_a = "https://blackcatinformatics.ca/gmeow/examples/logic/nearbyOrgs";
    let term_a = "https://blackcatinformatics.ca/gmeow/PredicatePath";
    // (b) a property-path row whose shape IRI resolves to NEITHER a DocShape
    // NOR a known DocTerm — honestly absent from `by_term`.
    let shape_b = "https://example.test/shapes/unresolvable";
    // (c) resolves via the FALLBACK: no DocShape claims it, but the bare shape
    // IRI itself names a known DocTerm.
    let shape_c = "https://blackcatinformatics.ca/gmeow/AncestorsTo3";
    let term_c = shape_c;

    let preservation_kind_val = format!("{LOGIC_NS}SoundUnderApproximation");
    let turtle = format!(
        "<https://example.test/target/a> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/a> <{label}> \"property-path:{shape_a}\" .\n\
             <https://example.test/target/a> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/a> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/a> <{drop}> \"structural note B\" .\n\
             <https://example.test/target/a> <{drop}> \"structural note A\" .\n\
             <https://example.test/target/b> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/b> <{label}> \"property-path:{shape_b}\" .\n\
             <https://example.test/target/b> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/b> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/c> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/c> <{label}> \"property-path:{shape_c}\" .\n\
             <https://example.test/target/c> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/c> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/whole-program> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/whole-program> <{label}> \"owl-dl\" .\n\
             <https://example.test/target/whole-program> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/whole-program> <{cc}> \"PTIME\" .\n",
        rdf_type = RDF_TYPE,
        pt = LOGIC_PROJECTION_TARGET_TYPE,
        label = RDFS_LABEL,
        pk = LOGIC_PRESERVATION_KIND,
        pk_val = preservation_kind_val,
        cc = LOGIC_COMPLEXITY_CLASS,
        drop = GMEOW_LOSSY_DROP,
        shape_a = shape_a,
        shape_b = shape_b,
        shape_c = shape_c,
    );

    let shapes = vec![DocShape {
        shape_iri: shape_a.to_string(),
        target_term: term_a.to_string(),
        messages: Vec::new(),
        owner_slice: "test-slice".to_string(),
    }];
    let terms = vec![
        DocTerm {
            iri: term_a.to_string(),
            curie: "gmeow:PredicatePath".to_string(),
            category: DocTermCategory::Class,
            owner_slice: "test-slice".to_string(),
            ..Default::default()
        },
        DocTerm {
            iri: term_c.to_string(),
            curie: "gmeow:AncestorsTo3".to_string(),
            category: DocTermCategory::Class,
            owner_slice: "test-slice".to_string(),
            ..Default::default()
        },
    ];

    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-mappings".to_string(),
        mappings_product_with_ledger(&turtle),
    );

    let digest = term_loss_digest_from_upstream(&upstream, &shapes, &terms)
        .expect("digest folds from synthetic stage-mappings upstream");

    assert_eq!(
        digest.total_property_path_rows, 3,
        "3 property-path rows (a, b, c) counted; the whole-program row must not count"
    );
    assert_eq!(
        digest.by_term.get(term_a).map(Vec::len),
        Some(1),
        "shape_a joins via DocShape.shape_iri -> target_term"
    );
    let row_a = &digest.by_term[term_a][0];
    assert_eq!(row_a.target, format!("property-path:{shape_a}"));
    assert_eq!(row_a.preservation_kind, "SoundUnderApproximation");
    assert_eq!(row_a.complexity_class, "PTIME");
    assert_eq!(
        row_a.lossy_drops,
        vec![
            "structural note A".to_string(),
            "structural note B".to_string()
        ],
        "lossy_drops must be sorted"
    );
    assert_eq!(
        digest.by_term.get(term_c).map(Vec::len),
        Some(1),
        "shape_c joins via the bare-shape-IRI == DocTerm.iri fallback"
    );
    assert!(
        !digest
            .by_term
            .values()
            .flatten()
            .any(|r| r.target.contains("unresolvable")),
        "shape_b names no DocShape and no DocTerm — honestly absent from by_term"
    );
    assert!(
        !digest
            .by_term
            .values()
            .flatten()
            .any(|r| r.target == "owl-dl"),
        "a whole-program row must never enter by_term"
    );

    // Missing the declared `stage-mappings` upstream product hard-fails (never a
    // silent empty digest).
    assert!(
        term_loss_digest_from_upstream(&BTreeMap::new(), &shapes, &terms).is_err(),
        "missing stage-mappings must hard-fail"
    );
}

/// The GENERAL per-term attribution join (the source-term-attribution correction): a
/// `logic:TermProjectionLoss` node on ANY projection target attributes its drops to the
/// DOCUMENTED source term named by its structured `gmeow:lossySourceTerm` IRI — the
/// canonical core's loss when projected DOWN lands on the term's page. A node whose
/// source term is NOT documented is honestly absent (never forced onto a term).
#[test]
fn term_loss_digest_attributes_term_projection_loss_nodes_to_documented_source_terms() {
    use gmeow_docs::model::{DocTerm, DocTermCategory};

    let core_term = "https://blackcatinformatics.ca/gmeow/Agent";
    let undocumented = "https://blackcatinformatics.ca/gmeow/NotDocumented";
    let preservation_kind_val = format!("{LOGIC_NS}SoundUnderApproximation");
    // Two term-loss nodes: one attributing to a documented CORE term (joins), one to an
    // undocumented term (honest absence). Each carries the projection target label, the
    // preservation kind, the complexity class, and a dropped feature.
    let turtle = format!(
        "<https://example.test/target/sssom:abc/termloss/agent> <{rdf_type}> <{tpl}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{src}> <{core}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{label}> \"sssom:abc\" .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{cc}> \"1:1 lattice band\" .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{drop}> \"gmeow:Agent equivalentClass prov:Agent loses the caveat structure\" .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{rdf_type}> <{tpl}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{src}> <{nd}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{label}> \"sssom:def\" .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{drop}> \"orphan drop\" .\n",
        rdf_type = RDF_TYPE,
        tpl = LOGIC_TERM_PROJECTION_LOSS_TYPE,
        src = GMEOW_LOSSY_SOURCE_TERM,
        label = RDFS_LABEL,
        pk = LOGIC_PRESERVATION_KIND,
        pk_val = preservation_kind_val,
        cc = LOGIC_COMPLEXITY_CLASS,
        drop = GMEOW_LOSSY_DROP,
        core = core_term,
        nd = undocumented,
    );

    let terms = vec![DocTerm {
        iri: core_term.to_string(),
        curie: "gmeow:Agent".to_string(),
        category: DocTermCategory::Class,
        owner_slice: "test-slice".to_string(),
        ..Default::default()
    }];

    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-mappings".to_string(),
        mappings_product_with_ledger(&turtle),
    );

    let digest = term_loss_digest_from_upstream(&upstream, &[], &terms)
        .expect("digest folds from synthetic term-loss upstream");

    // The documented CORE term carries the attributed row (target = the projection
    // target label, preservation kind + complexity + dropped feature all present).
    let rows = digest
        .by_term
        .get(core_term)
        .expect("documented source term must carry its attributed projection-loss row");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target, "sssom:abc");
    assert_eq!(rows[0].preservation_kind, "SoundUnderApproximation");
    assert_eq!(rows[0].complexity_class, "1:1 lattice band");
    assert_eq!(
        rows[0].lossy_drops,
        vec!["gmeow:Agent equivalentClass prov:Agent loses the caveat structure".to_string()]
    );
    // The undocumented source term is honestly absent — never fabricated onto a term.
    assert!(
        !digest.by_term.contains_key(undocumented),
        "a term-loss node whose source term is undocumented must not enter by_term"
    );
    // Whole-program `property-path` count is untouched by the general attribution join.
    assert_eq!(digest.total_property_path_rows, 0);
}
