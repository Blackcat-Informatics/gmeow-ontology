// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::stages::source_load::rdf_bytes_to_dataset;

/// A native publication is mandatory even when the graph and program exist.
/// Reading it borrows the exact report and leaves final-count ownership here.
#[test]
fn mappings_require_and_borrow_complete_native_report_inputs() {
    use crate::bundle::{CompiledLogicPublication, bundle_from_artifacts_over};
    use gmeow_logic_compile::ir::{LogicProgram, PreservationKind};
    use gmeow_logic_compile::projections::report::ProjectionReportRow;
    use purrdf::provenance::DatasetProvenance;
    use std::sync::Arc;
    let dataset = purrdf::parse_dataset(
            format!("<https://example.org/s> <https://example.org/p> <https://example.org/o> <{GRAPH_LOGIC}> .").as_bytes(),
            "application/n-quads",
            None,
        ).unwrap();
    let program = Arc::new(LogicProgram::new(vec![], vec![], vec![], None));
    let publication = Arc::new(CompiledLogicPublication {
        report: LogicReportInputs {
            header: ReportHeader::of_program(&program),
            base_correspondence_count: 5,
            base_lawful_uplift_count: 3,
            projections: vec![ProjectionReportRow {
                target: "canonical-rdf12".into(),
                is_rdf: true,
                preservation: PreservationKind::Exact,
                complexity: "P".into(),
            }],
            loss: LossLedger::new(),
        },
        program: Arc::clone(&program),
    });
    let upstream = |handle| {
        let mut bundle = bundle_from_artifacts_over(
            Arc::clone(&dataset),
            BTreeMap::new(),
            DatasetProvenance::new(),
        );
        let digest = bundle.graph_digest(GRAPH_LOGIC);
        bundle.pin_handle(GRAPH_LOGIC, handle, digest).unwrap();
        BTreeMap::from([(
            "stage-compile-logic".into(),
            StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle)),
        )])
    };
    assert!(logic_report_inputs(&BTreeMap::new()).is_err());
    assert!(logic_report_inputs(&upstream(PipelineHandle::Logic(program))).is_err());
    let products = upstream(PipelineHandle::CompiledLogic(Arc::clone(&publication)));
    let report = logic_report_inputs(&products).unwrap();
    assert!(std::ptr::eq(report, &publication.report));
    let mut final_header = report.header;
    final_header.correspondence_count = report.base_correspondence_count + 2;
    final_header.lawful_uplift_count = report.base_lawful_uplift_count + 1;
    final_header.claimed_uplift_count = 1;
    let rendered = build_union_report(final_header, report, &[], &LossLedger::new()).unwrap();
    assert_eq!(report.header.correspondence_count, 0);
    assert_eq!(report.header.lawful_uplift_count, 0);
    assert_eq!(report.header.claimed_uplift_count, 0);
    assert!(
        std::str::from_utf8(&rendered)
            .unwrap()
            .contains("claimedUpliftCount")
    );
    let released = products
        .into_iter()
        .map(|(stage, product)| (stage, product.into_carrier_released().unwrap()))
        .collect();
    assert!(logic_report_inputs(&released).is_err());
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn fixture_jobs() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

fn triple_set(bytes: &[u8], media_type: &str) -> std::collections::BTreeSet<String> {
    let dataset = rdf_bytes_to_dataset(bytes, media_type, "triple_set").unwrap();
    purrdf::flat_rdf_quads_from_dataset(&dataset)
        .iter()
        .map(|q| {
            // The predicate is a bare IRI string; wrap it as an IRI term so its
            // Display renders `<iri>` — matching the old oxigraph `NamedNode` form
            // the substring assertions key on.
            let predicate = purrdf::RdfTerm::iri(q.predicate.clone());
            format!("{} {} {} .", q.subject, predicate, q.object)
        })
        .collect()
}

#[test]
fn sssom_diagnostics_surface_parse_and_validation_errors() {
    let mut report = Report::new("mapping-compile");
    fold_sssom_findings(
        &mut report,
        "generated/mappings/bad.sssom.tsv",
        b"# mapping_set_id: https://example.org/missing-body\n",
    );
    let parse = report
        .findings
        .iter()
        .find(|finding| finding.detail.as_deref() == Some("check=parse code=sssom-tsv-parse"))
        .expect("parse failure finding");
    assert_eq!(parse.code, "mapping-compile.sssom");
    assert_eq!(
        parse
            .primary_location()
            .and_then(|location| location.path.as_deref()),
        Some("generated/mappings/bad.sssom.tsv")
    );

    let invalid = "\
# mapping_set_id: https://example.org/mapping\n\
# mapping_set_version: 0.1.0\n\
# license: https://creativecommons.org/licenses/by/4.0/\n\
# curie_map:\n\
#   gmeow: https://blackcatinformatics.ca/gmeow/\n\
#   skos: http://www.w3.org/2004/02/skos/core#\n\
#   semapv: https://w3id.org/semapv/vocab/\n\
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment\n\
nope:Foo\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.7\tmissing prefix\n";
    fold_sssom_findings(
        &mut report,
        "generated/mappings/prefix.sssom.tsv",
        invalid.as_bytes(),
    );
    let validation = report
        .findings
        .iter()
        .find(|finding| {
            finding.detail.as_deref() == Some("check=PrefixMapCompleteness code=prefix validation")
        })
        .expect("validation failure finding");
    assert_eq!(validation.code, "mapping-compile.sssom");
    assert_eq!(
        validation
            .primary_location()
            .and_then(|location| location.path.as_deref()),
        Some("generated/mappings/prefix.sssom.tsv")
    );
}

/// Post-lang-graft: the BCP-47 projection consumers reach a language's tag THROUGH
/// its carrier variety (the folded `gmeow:bcp47Tag` rides the variety IRI, joined via
/// `lang:varietyOf`), the `@x-gmeow-*` retag reads `lang:carrierTag` on the variety,
/// and the schema.org cells are re-expressed against the migrated shape
/// (`gmeow:Language` + `lang:signSystemKind`; `lang:Orthography` binding). Computed
/// FRESH from the DSL, so it verifies the rewiring independently of the committed
/// (current) `.rq` bytes.
#[test]
fn bcp47_projection_queries_join_through_variety() {
    let root = repo_root();
    let artifacts =
        crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
    let query = |name: &str| -> String {
        let path = format!("{QUERIES_DIR}/{name}");
        let (_, bytes) = artifacts
            .iter()
            .find(|(p, _)| p.as_str() == path.as_str())
            .unwrap_or_else(|| panic!("missing generated query {path}"));
        String::from_utf8_lossy(bytes).into_owned()
    };

    // ontolex: the name/lexical-item language tag is reached THROUGH the variety.
    let ontolex = query("ontolex.rq");
    assert!(
        ontolex.contains("lang:varietyOf ?lang") && ontolex.contains("gmeow:bcp47Tag ?langTag"),
        "ontolex must join the language tag through its variety:\n{ontolex}"
    );

    let schema = query("schema-org.rq");
    // inLanguage joins through the content language's variety.
    assert!(
        schema.contains("lang:varietyOf ?ilLang") && schema.contains("gmeow:bcp47Tag ?ilTag"),
        "schema inLanguage must join through the variety"
    );
    // The @x-gmeow-* retag reads lang:carrierTag on the variety, not the removed
    // gmeow:languageTag on the language.
    assert!(
        schema.contains("?_variety lang:varietyOf ?_lang")
            && schema.contains("?_variety lang:carrierTag ?_intTag")
            && schema.contains("?_variety gmeow:bcp47Tag ?_extTag"),
        "schema retag must reach carrierTag + bcp47Tag through the variety"
    );
    // The removed authored properties/classes are gone from the emitted queries.
    assert!(
        !schema.contains("gmeow:languageTag") && !schema.contains("gmeow:usesWritingSystem"),
        "no removed authored language properties survive in the schema query"
    );
    assert!(
        !schema.contains("a gmeow:ProgrammingLanguage"),
        "the removed gmeow:ProgrammingLanguage class must not survive"
    );
    // The migrated programming-language shape: gmeow:Language of programmingLanguageKind.
    assert!(
        schema.contains("lang:programmingLanguageKind"),
        "programming languages are gmeow:Language of lang:programmingLanguageKind"
    );
}

/// Load the exact authenticated mappings and upstream artifact lanes. A cache miss
/// hard-fails; this helper cannot execute any producer.
fn load_mappings_with_authenticated_upstream() -> (StageProduct, BTreeMap<String, StageProduct>) {
    let root = repo_root();
    let jobs = fixture_jobs();
    let mappings = crate::fixture::stage_artifacts(&root, jobs, "stage-mappings")
        .expect("exact mappings artifact fixture");
    let compile_logic = crate::fixture::stage_artifacts(&root, jobs, "stage-compile-logic")
        .expect("exact compile-logic artifact fixture");
    let constraint_shapes =
        crate::fixture::stage_artifacts(&root, jobs, "stage-export-constraint-shapes")
            .expect("exact constraint-shapes artifact fixture");
    (
        StageProduct::from_artifacts("stage-mappings", mappings),
        BTreeMap::from([
            (
                "stage-compile-logic".to_string(),
                StageProduct::from_artifacts("stage-compile-logic", compile_logic),
            ),
            (
                "stage-export-constraint-shapes".to_string(),
                StageProduct::from_artifacts("stage-export-constraint-shapes", constraint_shapes),
            ),
        ]),
    )
}

#[test]
fn projection_report_unions_logic_and_correspondence_rows() {
    // Consume the exact producer-authenticated mapping product. The test never
    // recompiles either the mappings or compile-logic stages.
    let (out, _upstream) = load_mappings_with_authenticated_upstream();
    let report = std::str::from_utf8(out.artifact(PROJECTION_REPORT_PATH).expect("report"))
        .expect("utf8 report");

    // The report carries the correspondence rows (the whole point of the union): at
    // least one row per alignment dialect.
    for dialect in ["sssom:", "fno:", "edoal:", "sparql:"] {
        assert!(
            report.contains(&format!("/target/{dialect}")),
            "projection report missing {dialect} correspondence rows"
        );
    }

    assert!(
        report.contains("/target/owl-dl>") || report.contains("/target/datalog>"),
        "projection report must retain at least one logic projection row"
    );
}

/// The shape-grounding certificate ledger: one re-derived certificate per
/// `logic:formalizes` record on THIS run's projected constraint surfaces, with the
/// entry count EQUAL to the surfaces' record count (count-consistency — no record is
/// skipped, none is invented), every entry carrying a re-derived
/// `logic:preservationKind`, and the artifact emitted as its own canonical fold
/// (structural idempotence: re-canonicalizing is a byte no-op).
#[test]
fn shape_grounding_ledger_covers_every_formalizes_record() {
    let (out, upstream) = load_mappings_with_authenticated_upstream();
    let ledger = std::str::from_utf8(
        out.artifact(SHAPE_GROUNDING_LEDGER_PATH)
            .expect("shape-grounding ledger artifact"),
    )
    .expect("utf8 ledger");

    // Count the formalizes records on the SAME fresh surfaces the stage consumed
    // (read off the upstream products the helper already ran — no re-run).
    let constraint_ttl = upstream["stage-export-constraint-shapes"]
        .artifact(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH)
        .expect("constraint shapes");
    let procedural_ttl = upstream["stage-compile-logic"]
        .artifact(crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH)
        .expect("procedural constraints");
    let mut expected = 0usize;
    for bytes in [constraint_ttl, procedural_ttl] {
        let ds = purrdf::parse_dataset(bytes, "text/turtle", None).expect("surface parses");
        expected += gmeow_validate::shape_grounding::formalizes_records(&ds)
            .values()
            .map(std::collections::BTreeSet::len)
            .sum::<usize>();
    }
    assert!(expected > 0, "the surfaces must carry formalizes records");
    // Count-consistency, quad-exact: the ledger re-states EVERY surface
    // logic:formalizes record (no record skipped, none invented) and carries exactly
    // one re-derived judgment per record subject.
    let ledger_ds =
        purrdf::parse_dataset(ledger.as_bytes(), "text/turtle", None).expect("ledger parses");
    let ledger_records = gmeow_validate::shape_grounding::formalizes_records(&ledger_ds);
    assert_eq!(
        ledger_records
            .values()
            .map(std::collections::BTreeSet::len)
            .sum::<usize>(),
        expected,
        "the ledger must carry EXACTLY one certificate entry per surface \
             logic:formalizes record (count-consistency)"
    );
    assert_eq!(
        ledger.matches("logic:preservationKind logic:").count(),
        ledger_records.len(),
        "every record carries exactly one re-derived preservation judgment"
    );
    // The committed bytes ARE the canonical fold: re-canonicalizing is a byte no-op
    // (the structural idempotence guarantee — a second regenerate cannot differ).
    let recanon = purrdf::turtle_normalize::canonical_turtle(
        ledger.as_bytes(),
        &crate::stages::superset::rdf_prefixes(),
    )
    .expect("re-canonicalize");
    assert_eq!(
        recanon.as_bytes(),
        ledger.as_bytes(),
        "the ledger must be emitted as exactly its own canonical fold"
    );
}

#[test]
fn fno_is_well_formed_ntriples() {
    // Wiring check: the FnO correspondence lowering produces a non-empty FnO
    // catalog that parses. (Committed-byte/iso parity is the CI strict-sync
    // gate, env-matched.)
    let root = repo_root();
    let artifacts =
        crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
    let fno = artifacts.get(FNO_PATH).expect("fno artifact");
    let triples = triple_set(fno, "text/turtle");
    assert!(
        triples.len() > 20,
        "FnO catalog unexpectedly small: {} triples",
        triples.len()
    );
}

#[test]
fn prefix_set_projections_are_emitted_and_parse() {
    // Wiring check (§2): the mappings stage emits the importable prefix
    // set + JSON-LD context, and the Turtle parses with the importable node
    // carrying the generalized sh:declare surface.
    let root = repo_root();
    let artifacts =
        crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");

    let core = artifacts
        .get(CORE_PREFIXES_PATH)
        .expect("core-prefixes artifact");
    let triples = triple_set(core, "text/turtle");
    // owl:Ontology declaration + at least one sh:declare per registry entry.
    let has_node = triples.iter().any(|t| {
        t.contains("CorePrefixes")
            && t.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
            && t.contains("http://www.w3.org/2002/07/owl#Ontology")
    });
    assert!(has_node, "core-prefixes missing owl:Ontology node");
    let declares = triples
        .iter()
        .filter(|t| t.contains("http://www.w3.org/ns/shacl#prefix>"))
        .count();
    assert!(
        declares > 100,
        "expected one sh:prefix per registry entry, got {declares}"
    );

    let ctx = artifacts
        .get(JSONLD_CONTEXT_PATH)
        .expect("context.jsonld artifact");
    let text = std::str::from_utf8(ctx).expect("utf8 context");
    assert!(
        text.contains("\"@context\""),
        "context.jsonld has no @context"
    );
    assert!(text.contains("\"@vocab\""), "context.jsonld has no @vocab");
    assert!(text.ends_with("}\n}\n"), "context.jsonld malformed tail");
}

#[test]
fn list_functions_are_emitted_and_parse() {
    // Wiring check (§5): the mappings stage emits the six list functions
    // as well-formed FnO Turtle (routed through the shared
    // `purrdf::fno::to_quads` serializer, §19 one-path), each typed via
    // fno:Output and fno:Function.
    let root = repo_root();
    let artifacts =
        crate::fixture::mapping_artifacts(&root, fixture_jobs()).expect("mapping fixture");
    let lf = artifacts
        .get(LIST_FUNCTIONS_PATH)
        .expect("list-functions artifact");
    let triples = triple_set(lf, "text/turtle");
    let functions = triples
        .iter()
        .filter(|t| t.contains("https://w3id.org/function/ontology#Function"))
        .count();
    assert_eq!(functions, 6, "expected six fno:Function declarations");
    // Primitives are NOT gmeow:ProjectionFunction.
    assert!(
        !triples
            .iter()
            .any(|t| t.contains("https://blackcatinformatics.ca/gmeow/ProjectionFunction")),
        "list functions must not be gmeow:ProjectionFunction"
    );
    // Primitives bind no fno:predicate.
    assert!(
        !triples
            .iter()
            .any(|t| t.contains("<https://w3id.org/function/ontology#predicate>")),
        "list functions must bind no fno:predicate"
    );
    // Each issue-named function is present.
    for name in [
        "listLength",
        "listGet",
        "listIndexOf",
        "listSlice",
        "listConcat",
        "listContains",
    ] {
        assert!(
            triples
                .iter()
                .any(|t| t.contains(&format!("gmeow/{name}>"))),
            "missing function {name}"
        );
    }
}

/// Errors from `structural_lint_dataset` whose message names one of the four
/// A-Box structural-annotation predicates this task completes — deliberately
/// excludes the (separately tracked) language-tag-discipline codes, so this
/// stays a precise "zero A-Box findings" check, not "zero findings at all".
fn abox_annotation_errors<'a>(errors: &'a [String], subject: &str) -> Vec<&'a String> {
    const MISSING_MARKERS: [&str; 4] = [
        "is missing rdfs:label",
        "is missing skos:definition",
        "is missing rdfs:isDefinedBy",
        "is missing gmeow:graphBoxRole",
    ];
    errors
        .iter()
        .filter(|e| e.contains(subject) && MISSING_MARKERS.iter().any(|m| e.contains(m)))
        .collect()
}

/// Shift-left: drive the SAME native structural lint the whole-bundle SHACL
/// validation (`make validate` / the pipeline stage-validate) runs
/// (`gmeow_validate::lint::structural_lint_dataset`) over `complete_core_prefixes_abox`'s
/// real output, so a missing/incorrect A-Box annotation on the minted
/// `gmeow:CorePrefixes` T-Box header reds HERE — a fast `cargo nextest -p
/// gmeow-pipeline` — rather than only surfacing at the next expensive
/// whole-bundle SHACL validation. Mirrors `provenance_graph.rs`'s
/// `minted_individuals_satisfy_the_assertional_abox_contract`.
#[test]
fn core_prefixes_completion_satisfies_the_structural_contract() {
    use gmeow_validate::lint::{
        LintConfig, default_annotation_predicates, structural_lint_dataset,
    };

    let vocab = gmeow_ns::gmeow_slice_vocab();
    let subject = vocab.core_prefixes_iri();
    let completed = complete_core_prefixes_abox(&emit_core_prefixes(&vocab), &vocab);
    // The real bundle supplies `gmeow:boxTBox a gmeow:GraphBoxRole` from the
    // kernel slice (`slices/vocabulary.ttl` et al. already reference
    // `gmeow:boxTBox` as a role); add it here so the role-typing check has its
    // declaration to resolve against (same pattern as `release.rs`'s
    // `minted_attestations_satisfy_the_assertional_contract`).
    let doc = format!(
        "{completed}<{}> <{}> <https://blackcatinformatics.ca/gmeow/GraphBoxRole> .\n",
        gmeow_errors::abox::BOX_TBOX,
        gmeow_errors::abox::RDF_TYPE,
    );
    let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse the completed core-prefixes turtle");

    let cfg = LintConfig {
        namespace: gmeow_ns::GMEOW_NS.to_string(),
        ontology_iri: gmeow_ns::GMEOW_NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };
    let report = structural_lint_dataset(&ds, &cfg);
    let errors = report.errors();
    let core_prefixes_errors = abox_annotation_errors(&errors, &subject);
    assert!(
        core_prefixes_errors.is_empty(),
        "gmeow:CorePrefixes must satisfy the T-Box structural-annotation contract \
             (rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole): \
             {core_prefixes_errors:?}"
    );
    // Do NOT add gmeow:boxABox to a T-Box owl:Ontology header.
    assert!(
        !completed.contains(gmeow_errors::abox::BOX_ABOX),
        "gmeow:CorePrefixes is a T-Box header — it must never carry gmeow:boxABox"
    );
    // The label/comment `emit_core_prefixes` mints with a bare `@en` must be
    // retagged to the carrier tag — CorePrefixes is an internal ontology, not an
    // external lowering, so it owes the carrier-tag discipline.
    assert!(
        !errors
            .iter()
            .any(|e| e.contains(&subject) && e.contains("external language tag 'en'")),
        "gmeow:CorePrefixes label/comment must carry x-gmeow-english, not bare @en: {errors:?}"
    );
    assert!(
        !completed.contains("\"@en"),
        "no bare @en may survive on the completed CorePrefixes A-Box: {completed}"
    );
}

/// Shift-left: same pattern as `core_prefixes_completion_satisfies_the_structural_contract`,
/// over `complete_list_functions_abox`'s real output — every A-Box
/// `fno:Function`/`fno:Output`/`fno:Parameter` individual it mints must satisfy
/// the full four-predicate contract.
#[test]
fn list_functions_completion_satisfies_the_structural_contract() {
    use gmeow_validate::lint::{LintConfig, structural_lint_dataset};

    let vocab = gmeow_ns::gmeow_slice_vocab();
    let catalog = purrdf::slice::list_functions::list_functions_catalog(&vocab);
    let completed = complete_list_functions_abox(&emit_list_functions(&vocab), &vocab);
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
    // kernel slice; add it here so the role-typing check has its declaration.
    let doc = format!(
        "{completed}<{}> <{}> <https://blackcatinformatics.ca/gmeow/GraphBoxRole> .\n",
        gmeow_errors::abox::BOX_ABOX,
        gmeow_errors::abox::RDF_TYPE,
    );
    let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse the completed list-functions turtle");

    let cfg = LintConfig {
        namespace: gmeow_ns::GMEOW_NS.to_string(),
        ontology_iri: gmeow_ns::GMEOW_NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: Default::default(),
    };
    let report = structural_lint_dataset(&ds, &cfg);
    let errors = report.errors();

    let mut subjects: Vec<String> = Vec::new();
    for func in &catalog.functions {
        subjects.push(func.iri.clone());
        subjects.push(func.output.iri.clone());
    }
    for param in &catalog.params {
        subjects.push(param.iri.clone());
    }
    assert_eq!(
        subjects.len(),
        6 + 6 + 7,
        "expected 6 functions + 6 outputs + 7 params"
    );

    for subject in &subjects {
        let subject_errors = abox_annotation_errors(&errors, subject);
        assert!(
            subject_errors.is_empty(),
            "{subject} must satisfy the A-Box structural-annotation contract \
                 (rdfs:label / skos:definition / rdfs:isDefinedBy / gmeow:graphBoxRole): \
                 {subject_errors:?}"
        );
    }
}
