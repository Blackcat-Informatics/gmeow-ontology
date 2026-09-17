// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use gmeow_logic_compile::frontend::{parse_logic_dataset, parse_logic_str};
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom};
use purrdf::ContentDigest;

fn source_theory(text: &str) -> Arc<CompiledTheory> {
    let dataset = parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    Arc::new(
        gmeow_logic_compile::frontend::PreparedLogicSource::new(&dataset)
            .unwrap()
            .into_compiled(None)
            .unwrap(),
    )
}

#[test]
fn source_presentation_merges_reach_the_production_executor() {
    const SOURCE: &str =
        include_str!("../../../logic-compile/tests/fixtures/presentation-merge.ttl");
    let theory = source_theory(SOURCE);
    let (_, merges, _) = compile_source_projections(&theory, theory.program()).unwrap();
    let result = &merges.merges()["urn:presentation-example:merge"];
    assert_eq!(result.output().symbols().len(), 4);
    assert_eq!(result.output().sentences().len(), 3);
    assert!(Arc::ptr_eq(merges.source(), &theory));
    for source in [
        SOURCE.replace(
            "logic:mergeRight ex:rightMap",
            "logic:mergeRight ex:missing",
        ),
        SOURCE.replace(
            "logic:generatorBinding ex:pImage, ex:xImage",
            "logic:generatorBinding ex:pImage",
        ),
        SOURCE.replace(
            "logic:sentenceContext ex:context",
            "logic:sentenceContext ex:missing",
        ),
    ] {
        let malformed = source_theory(&source);
        assert!(compile_source_projections(&malformed, malformed.program()).is_err());
    }
}

#[test]
fn source_composition_obligations_reach_the_production_gate() {
    let mut source = String::from(
        "@prefix ex: <https://example.org/> . @prefix logic: <https://blackcatinformatics.ca/logic/> .\n",
    );
    for (name, from, to) in [
        ("first", "A", "B"),
        ("second", "B", "C"),
        ("result", "A", "C"),
    ] {
        source.push_str(&format!("ex:{name} a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ; logic:sourceEndpoint ex:{from} ; logic:targetEndpoint ex:{to} .\n"));
    }
    source.push_str("ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .");
    let theory = source_theory(&source);
    let (artifacts, _, _) = compile_source_projections(&theory, theory.program()).unwrap();
    assert_eq!(
        artifacts
            .correspondence_gates
            .unwrap()
            .per_composition
            .len(),
        1
    );
    let malformed = source_theory(&source.replace("logic:compositionSecond ex:second ;", ""));
    let error = compile_source_projections(&malformed, malformed.program()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rejected authored CorrespondenceComposition"),
        "{error}"
    );
    let disconnected = source_theory(&source.replace(
        "logic:compositionSecond ex:second",
        "logic:compositionSecond ex:first",
    ));
    let error = compile_source_projections(&disconnected, disconnected.program()).unwrap_err();
    assert!(error.to_string().contains("endpoint references"), "{error}");
    // With no correspondence members at all the compiler must still emit a RED.
    let empty = source_theory(
        "@prefix ex: <https://example.org/> . @prefix logic: <https://blackcatinformatics.ca/logic/> . ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .",
    );
    assert!(compile_source_projections(&empty, empty.program()).is_err());
}

#[test]
fn shared_compilation_rejects_malformed_correspondence_and_lawless_missing_leg() {
    for (body, family) in [
        ("ex:cell a logic:Correspondence .", "Correspondence"),
        (
            "ex:cell a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ; logic:getLeg ex:missing .",
            "TransactionProgram",
        ),
    ] {
        let theory = source_theory(&format!(
            r#"
                @prefix logic: <https://blackcatinformatics.ca/logic/> .
                @prefix ex: <https://example.org/> . {body}
            "#
        ));
        let error = compile_source_projections(&theory, theory.program()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("rejected authored {family}")),
            "{error}"
        );
    }
}

fn compile_logic_fixture() -> StageProduct {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repository root");
    crate::fixture::stage_fixture(&root, 0, "stage-compile-logic")
        .expect("authenticated compile-logic fixture; tests never produce it")
        .outcome
        .product
}

/// A small clean program whose canonical RDF-1.2 projection is an EXACT round-trip
/// (the documented ExactPreservation case): only graph-derivable constructs —
/// `rdf:type → logic:Class` axioms (the form the reverse parser re-extracts) — no
/// modal reifiers, no rule-structural re-emission, no contract facet loss, and a
/// `None` source (`source_iri` is program provenance the canonical graph does not
/// carry, so a graph round-trip can only preserve it when it is absent).
fn clean_program() -> LogicProgram {
    let ax = |s: &str, o: &str| {
        LogicAxiom::new(
            s,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            gmeow_logic_compile::ir::AtomicTerm::resource(o),
            false,
            ContextualScope::default(),
        )
        .expect("valid axiom")
    };
    LogicProgram::new(
        vec![
            ax(
                "https://blackcatinformatics.ca/gmeow/Animal",
                "https://blackcatinformatics.ca/logic/Kind",
            ),
            ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                "https://blackcatinformatics.ca/logic/Subkind",
            ),
        ],
        vec![],
        vec![],
        None,
    )
}

/// P17 round-trip identity (C6): the canonical RDF-1.2 projection of a
/// LogicProgram parses back — BOTH via the string reverse parser AND via the
/// dataset reverse parser the cache uses on a hit — to a canonical-key-EQUAL
/// program. This is the identity the typed Logic handle relies on: a consumer can
/// re-derive the program from `graph/logic` and get the same content.
#[test]
fn canonical_rdf12_round_trips_to_equal_canonical_key() {
    let program = clean_program();
    let arts = compile_program(&program, |_| Default::default()).expect("compile clean program");

    // Via the string reverse parser.
    let (rp_str, diags) = parse_logic_str(
        &arts.canonical_rdf12.clone().into_text().content,
        program.source_iri.clone(),
    )
    .expect("reparse str");
    assert!(
        diags.is_empty(),
        "clean round-trip emits no diagnostics: {diags:?}"
    );
    assert_eq!(
        program.canonical_key(),
        rp_str.canonical_key(),
        "string round-trip must preserve the canonical key"
    );

    // Native reverse parsing consumes the same projection without text.
    let ds = Arc::clone(&arts.canonical_rdf12.dataset);
    let (rp_ds, _d) =
        parse_logic_dataset(ds.as_ref(), program.source_iri.clone()).expect("reparse dataset");
    assert_eq!(
        program.canonical_key(),
        rp_ds.canonical_key(),
        "dataset round-trip (cache re-derivation) must preserve the canonical key"
    );
}

/// The compile-logic stage pins a REAL typed [`PipelineHandle::CompiledLogic`] handle to
/// `graph/logic`, whose governed projection reconstructs rules and contracts
/// isomorphic to the original. The cache separately retains the complete native IR. The
/// full real-module canonical key is NOT asserted equal: the canonical RDF-1.2
/// projection re-emits rules as `logic:rule/...` structural triples that the
/// reverse parser reads back as BOTH rules and plain axioms (and the module's
/// `ProbabilisticProfile` contract intentionally drops its `ProbabilityModel` on
/// projection) — both are documented projection characteristics, not C6 defects.
/// The rule/contract IR isomorphism is the round-trip identity that holds whole.
#[test]
fn stage_pins_logic_handle_re_derivable_to_isomorphic_ir() {
    use crate::bundle::PipelineHandle;
    let product = compile_logic_fixture();
    let bundle = product.bundle();
    let entry = bundle
        .handle(GRAPH_LOGIC)
        .expect("the stage pins a Logic handle to graph/logic");
    // The pin is digest-valid: the pinned digest equals the live graph/logic digest.
    assert_eq!(
        entry.content_digest,
        bundle.graph_digest(GRAPH_LOGIC),
        "the Logic handle is digest-pinned to its backing graph/logic"
    );
    let PipelineHandle::CompiledLogic(publication) = &entry.payload else {
        panic!("the compiler publishes the program and mandatory report inputs");
    };
    let program = &publication.program;

    let szs_iri = "https://blackcatinformatics.ca/logic/corrSzsToVerdict";
    let szs = program
        .correspondences
        .iter()
        .find(|cell| cell.iri == szs_iri)
        .expect("the canonical SZS correspondence is present");
    let expected_loss = [
            "logic:SzsContradictoryAxioms and logic:SzsUnsatisfiable collapse to logic:ConfInconsistent; the CAX≠UNS distinction survives only via logic:rawStatusToken",
            "logic:SzsCounterSatisfiable and logic:SzsSatisfiable collapse to logic:ConfConsistent; the CSA≠SAT distinction survives only via logic:rawStatusToken",
        ].map(|note| purrdf::RdfLiteral::language_tagged(note, "x-gmeow-english"));
    assert_eq!(
        szs.loss_evidence, expected_loss,
        "the native publication retains exactly the two source-owned SZS literals"
    );
    let szs_target_prefix = format!("correspondence:{szs_iri}:");
    let rows: Vec<_> = publication
        .report
        .projections
        .iter()
        .filter(|row| row.target.starts_with(&szs_target_prefix))
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "SZS has one cell-owned preservation judgment"
    );
    let expected_drops: Vec<_> = expected_loss
        .iter()
        .map(|literal| (literal.lexical_form.clone(), szs_iri.to_owned()))
        .collect();
    assert_eq!(
        publication.report.loss.term_source_drops(&rows[0].target),
        expected_drops,
        "every reported SZS loss belongs to its actual source correspondence"
    );

    // Check the governed projection independently of complete native cache hydration.
    let canonical_ttl = product
        .artifact(CANONICAL_RDF12_PATH)
        .expect("canonical rdf12 artifact");
    let ds = parse_dataset(canonical_ttl, "text/turtle", None).expect("parse backing graph");
    let (re_derived, _d) =
        parse_logic_dataset(ds.as_ref(), program.source_iri.clone()).expect("re-derive program");

    // rules + contracts round-trip isomorphic (whole-program identity).
    let rc = |p: &LogicProgram| {
        LogicProgram::new(
            vec![],
            p.rules.clone(),
            p.contracts.clone(),
            p.source_iri.clone(),
        )
    };
    gmeow_logic_compile::adapter::assert_ir_isomorphic(&rc(program), &rc(&re_derived))
        .expect("the re-derived handle program is rule/contract-isomorphic to the original");
}

/// `pin_handle` HARD-fails when the Logic handle's pinned digest disagrees with
/// its backing graph (no-optionality, fail-closed) — the bundle never carries a
/// Logic handle that disagrees with `graph/logic`.
#[test]
fn pin_logic_handle_hard_fails_on_digest_mismatch() {
    let program = clean_program();
    let arts = compile_program(&program, |_| Default::default()).expect("compile clean program");
    let dataset =
        crate::stages::carrier::rooted_in_graph(&arts.canonical_rdf12.dataset, GRAPH_LOGIC)
            .expect("graph/logic dataset");
    let mut bundle = bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
    // A deliberately WRONG digest (the all-zero digest never equals a real graph).
    let wrong = ContentDigest::of(b"not the graph/logic canonical bytes");
    let err = bundle
        .pin_handle(GRAPH_LOGIC, PipelineHandle::Logic(Arc::new(program)), wrong)
        .expect_err("a mismatched pin must HARD-fail");
    assert!(
        matches!(
            err,
            purrdf::PipelineBundleError::HandleDigestMismatch { .. }
        ),
        "the Logic handle pin must fail closed on a digest mismatch, got {err:?}"
    );
}

// ── C8: the relational-core carrier lane ──────────────────────────────

/// The compile-logic stage pins a REAL typed [`PipelineHandle::RelationalCore`]
/// handle to `graph/relational-core`, and that handle re-derives (via the SAME
/// reverse parser the cache uses) from its backing graph to a content-key-EQUAL
/// dialect, including the full Formula layer and its explicit lowering residue.
#[test]
fn stage_pins_relational_core_handle_re_derivable_to_equal_dialect() {
    use gmeow_logic_compile::relational_core::parse_relational_core;
    let product = compile_logic_fixture();
    let bundle = product.bundle();
    let entry = bundle
        .handle(GRAPH_RELATIONAL_CORE)
        .expect("the stage pins a RelationalCore handle to graph/relational-core");
    // The pin is digest-valid: the pinned digest equals the live backing digest.
    assert_eq!(
        entry.content_digest,
        bundle.graph_digest(GRAPH_RELATIONAL_CORE),
        "the RelationalCore handle is digest-pinned to its backing graph/relational-core"
    );
    let PipelineHandle::RelationalCore(program) = &entry.payload else {
        panic!("the handle is the RelationalCore arm carrying the typed dialect");
    };
    // Re-derive the dialect from the backing graph exactly as the cache does, off
    // the committed N-Triples projection artifact.
    let nt = product
        .artifact(RELATIONAL_CORE_PATH)
        .expect("relational-core artifact");
    let ds = parse_dataset(nt, "application/n-triples", None).expect("parse backing graph");
    let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive dialect");
    assert_eq!(
        re_derived.content_key().unwrap(),
        program.content_key().unwrap(),
        "the cache re-derivation yields a content-key-equal relational-core dialect"
    );
}

/// `pin_handle` HARD-fails when the RelationalCore handle's pinned digest disagrees
/// with its backing graph (no-optionality, fail-closed).
#[test]
fn pin_relational_core_handle_hard_fails_on_digest_mismatch() {
    use gmeow_logic_compile::relational_core::{lower_program, project_relational_core_dataset};
    let program = clean_program();
    let lowered = lower_program(&program);
    let projection = project_relational_core_dataset(&lowered).expect("native projection");
    let dataset = crate::stages::carrier::rooted_in_graph(&projection, GRAPH_RELATIONAL_CORE)
        .expect("graph/relational-core dataset");
    let mut bundle = bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
    let wrong = ContentDigest::of(b"not the relational-core canonical bytes");
    let err = bundle
        .pin_handle(
            GRAPH_RELATIONAL_CORE,
            PipelineHandle::RelationalCore(Arc::new(lowered)),
            wrong,
        )
        .expect_err("a mismatched pin must HARD-fail");
    assert!(
        matches!(
            err,
            purrdf::PipelineBundleError::HandleDigestMismatch { .. }
        ),
        "the RelationalCore handle pin must fail closed on a digest mismatch, got {err:?}"
    );
}

/// No-second-lowering proof: the relational-core lowering runs EXACTLY ONCE (in the
/// producing stage). A downstream consumer reads the typed handle's already-lowered
/// dialect — it does NOT call `lower_program` again. This test exercises the consumer
/// path (`bundle.handle(...).payload`) and asserts it is the typed dialect, equal to
/// the producer's lowering of the SAME program, without invoking a fresh lowering on
/// the consumer side.
#[test]
fn downstream_consumer_reads_the_handle_without_re_lowering() {
    let product = compile_logic_fixture();
    let bundle = product.bundle();

    // The CONSUMER path: take the typed handle. This is the ONLY way the dialect is
    // obtained downstream — there is no second `lower_program` call here.
    let entry = bundle
        .handle(GRAPH_RELATIONAL_CORE)
        .expect("handle present");
    let PipelineHandle::RelationalCore(consumer_view) = &entry.payload else {
        panic!("consumer reads the RelationalCore handle");
    };

    // It carries a real lowered dialect (facts/rules present), proving the consumer
    // did not have to re-lower to read the rules.
    assert!(
        !consumer_view.facts.is_empty() || !consumer_view.rules.is_empty(),
        "the handle carries the already-lowered dialect (facts and/or rules)"
    );
    // And it is the SAME content as the committed projection the producer emitted —
    // i.e. the producer lowered once and that single result rides both faces.
    let nt = product
        .artifact(RELATIONAL_CORE_PATH)
        .expect("projection artifact");
    let re_derived = gmeow_logic_compile::relational_core::parse_relational_core(
        parse_dataset(nt, "application/n-triples", None)
            .expect("parse")
            .as_ref(),
    )
    .expect("re-derive");
    assert_eq!(
        consumer_view.content_key().unwrap(),
        re_derived.content_key().unwrap(),
        "the consumer handle and the folded projection are one content identity"
    );
}

// ── C10: the correspondence carrier lane ──────────────────────────────

/// Correspondence is shipped and digest-pinned, but it is a meta-formula envelope:
/// target vocabulary IRIs must never be scanned as object-level OWL commitments.
#[test]
fn correspondence_is_carried_but_not_reasoned() {
    assert!(
        CARRIER_GRAPHS.contains(&GRAPH_CORRESPONDENCE),
        "the shipped carrier must retain graph/correspondence"
    );
    assert!(
        !OBJECT_LEVEL_GRAPHS.contains(&GRAPH_CORRESPONDENCE),
        "the meta-level correspondence graph must stay outside object-level closure"
    );
    assert!(
        carrier_entity_list().contains(&GRAPH_CORRESPONDENCE.to_string()),
        "validation/cache dataflow must still see the complete compiled carrier"
    );
    assert!(
        !object_level_entity_list().contains(&GRAPH_CORRESPONDENCE.to_string()),
        "reasoning dataflow must not consume correspondence target IRIs"
    );
}

/// One authenticated action supplies the complete typed correspondence program,
/// its terminal projection and its certified physical plans. The distinguished
/// affine cell still matches an independent synthetic expectation, while every
/// artifact is bound to the same complete program.
#[test]
fn stage_pins_correspondence_handle_re_derivable_with_no_overclaim() {
    use gmeow_logic::correspondence_exec::physical_plan::{
        CompositionPhysicalPlan, CompositionPlanLimits, CompositionPlanReport,
        PreparedCompositionProgram, verify_composition_report,
    };

    let product = compile_logic_fixture();
    let bundle = product.bundle();
    let entry = bundle
        .handle(GRAPH_CORRESPONDENCE)
        .expect("the stage pins a Correspondence handle to graph/correspondence");
    // The pin is digest-valid: the pinned digest equals the live backing digest.
    assert_eq!(
        entry.content_digest,
        bundle.graph_digest(GRAPH_CORRESPONDENCE),
        "the Correspondence handle is digest-pinned to its backing graph/correspondence"
    );
    let PipelineHandle::Correspondence(program) = &entry.payload else {
        panic!("the handle is the Correspondence arm carrying the typed program");
    };

    let expected_affine = synthetic_affine_program()
        .correspondences
        .into_iter()
        .next()
        .expect("synthetic affine cell");
    let affine = program
        .correspondences
        .iter()
        .find(|candidate| candidate.iri == expected_affine.iri)
        .expect("the complete program retains the canonical affine cell");
    assert_eq!(
        affine, &expected_affine,
        "the authored affine cell must match its independent typed expectation"
    );
    for iri in [
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/rootToEvent",
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/eventToItem",
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/rootToItem",
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/itemToValue",
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/rootToValue",
    ] {
        assert!(
            program
                .correspondences
                .iter()
                .any(|candidate| candidate.iri == iri),
            "the complete program omits executable worked correspondence <{iri}>"
        );
    }
    let expected_projection =
        project_correspondence_dataset(program).expect("native complete-program projection");

    // The committed projection artifact: the load-bearing alignment correctness point.
    let nt = product
        .artifact(CORRESPONDENCE_PATH)
        .expect("correspondence artifact");
    assert_eq!(
        nt,
        crate::stages::superset::canonical_ntriples(&expected_projection)
            .expect("canonical synthetic expectation")
            .as_slice(),
        "the producer must preserve the complete correspondence projection bytes",
    );
    let nt_str = std::str::from_utf8(nt).expect("utf8");
    // Check the alignment PREDICATE position (`<...#relatedMatch>` as a predicate),
    // not a bare substring — the loss-ledger prose mentions the forbidden predicates
    // by name (that prose is the disclosure, not an emitted alignment edge).
    assert!(
        nt_str.contains("<http://www.w3.org/2004/02/skos/core#relatedMatch>"),
        "the affine overlap stays at skos:relatedMatch:\n{nt_str}"
    );
    assert!(
        !nt_str.contains("<http://www.w3.org/2004/02/skos/core#exactMatch>"),
        "a caveated overlap MUST NOT emit a skos:exactMatch edge:\n{nt_str}"
    );
    assert!(
        !nt_str.contains("<http://www.w3.org/2002/07/owl#equivalentClass>"),
        "a caveated overlap MUST NOT emit an owl:equivalentClass edge:\n{nt_str}"
    );
    assert!(
        nt_str.contains("lossyDrop"),
        "the lane carries an explicit loss-ledger row:\n{nt_str}"
    );

    correspondence_roundtrip::assert_roundtrip(&product);

    let emitted: CompositionPlanReport = serde_json::from_slice(
        product
            .artifact(CORRESPONDENCE_PLANS_PATH)
            .expect("certified correspondence physical plans"),
    )
    .expect("typed correspondence physical-plan report");
    let prepared = PreparedCompositionProgram::prepare(program, CompositionPlanLimits::default())
        .expect("re-prepare the exact typed correspondence program");
    assert_eq!(
        &emitted,
        prepared.report(),
        "the physical-plan artifact must be reproducible from the carried typed program"
    );
    verify_composition_report(&emitted, program)
        .expect("the transported report and every certificate independently recheck");
    for declaration in [
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/rootToItemComposition",
        "https://blackcatinformatics.ca/gmeow/example/openehr/blood-pressure/rootToValueComposition",
    ] {
        let certificate = emitted
            .certificates
            .iter()
            .find(|certificate| certificate.declaration == declaration)
            .unwrap_or_else(|| panic!("missing certified worked composition <{declaration}>"));
        assert_eq!(
            certificate.selected,
            CompositionPhysicalPlan::FusedWitnessJoin,
            "the admitted pure read composition must use its certified fused plan"
        );
    }

    let rchops21: gmeow_logic::correspondence_exec::plan_projection::PlanProjection =
        serde_json::from_slice(
            product
                .artifact(RCHOPS21_PLAN_PROJECTION_PATH)
                .expect("certified RCHOPS21 plan projection"),
        )
        .expect("typed RCHOPS21 plan projection");
    gmeow_logic::correspondence_exec::plan_projection::verify_plan_projection_certificate(
        &rchops21,
    )
    .expect("the transported plan projection certificate independently rechecks");
    assert_eq!(rchops21.plan, "urn:gmeow:plan:rchops21");
    assert_eq!(rchops21.loop_count, 6);
    assert_eq!(
        rchops21
            .loop_variable
            .as_ref()
            .map(|literal| literal.lexical_form.as_str()),
        Some("cycle")
    );
    assert_eq!(rchops21.steps.len(), 30);
    assert_eq!(rchops21.freshness.len(), 4);
    assert_eq!(rchops21.preconditions.len(), 1);
    assert!(!rchops21.complement.source_facts.is_empty());
    let patient_fit = rchops21
        .guards
        .iter()
        .find(|guard| guard.guard == "urn:gmeow:guard:patient-fit")
        .expect("patient-fit guarded branch");
    assert_eq!(
        patient_fit.condition.tracked_states,
        [
            "https://blackcatinformatics.ca/gmeow/neutrophils".to_owned(),
            "https://blackcatinformatics.ca/gmeow/platelets".to_owned(),
        ]
    );
    let administer = rchops21
        .actions
        .iter()
        .find(|action| action.schema == "urn:gmeow:action:administer-regime")
        .expect("administer-regime action schema");
    assert_eq!(administer.outcomes.len(), 2);
    assert_eq!(administer.conditional_steps.len(), 1);
    assert_eq!(administer.conditional_steps[0].then_steps.len(), 2);
    assert!(rchops21.actions.iter().any(|action| {
        action.schema == "urn:gmeow:action:manage-reaction" && !action.effects.is_empty()
    }));
    let rchops21_recovery: gmeow_logic::correspondence_exec::plan_projection::PlanRecovery =
        serde_json::from_slice(
            product
                .artifact(RCHOPS21_PLAN_RECOVERY_PATH)
                .expect("certified RCHOPS21 plan recovery"),
        )
        .expect("typed RCHOPS21 plan recovery");
    gmeow_logic::correspondence_exec::plan_projection::verify_plan_recovery_receipt(
        &rchops21_recovery,
        &rchops21,
    )
    .expect("the transported recovery receipt independently rechecks");
    assert_eq!(rchops21_recovery.occurrences.len(), 3);
    assert!(rchops21_recovery.off_plan_occurrences.is_empty());
    assert!(
        rchops21_recovery
            .unobserved_planned_schemas
            .contains(&"urn:gmeow:action:defer-cycle".to_owned())
    );
}

/// The overclaim gate is a BUILD FAILURE for an attempt to emit a class equivalence
/// for the §14 affine/overlaps correspondence the stage carries (the gate fires).
#[test]
fn stage_correspondence_overclaim_gate_rejects_equivalence() {
    use gmeow_logic_compile::projections::correspondence::assert_no_overclaim_correspondence;
    let product = compile_logic_fixture();
    let bundle = product.bundle();
    let entry = bundle.handle(GRAPH_CORRESPONDENCE).expect("handle present");
    let PipelineHandle::Correspondence(program) = &entry.payload else {
        panic!("Correspondence arm");
    };
    let correspondence = program
        .correspondences
        .iter()
        .find(|candidate| {
            candidate.iri
                == "https://blackcatinformatics.ca/gmeow/example/gmeowContactCorrespondence"
        })
        .expect("canonical affine correspondence");
    // Asking for equivalence over this caveated affine overlap is an overclaim → red.
    assert_no_overclaim_correspondence(correspondence, true)
        .expect_err("emitting equivalence for the §14 affine overlap must HARD-fail");
    // The related-match surface (what the lane actually emits) is NOT an overclaim.
    assert_no_overclaim_correspondence(correspondence, false)
        .expect("the related-match surface is not an overclaim");
}

/// The five-gate `assert_gates` is now a STAGE hard-fail (not merely recorded): the
/// lawful affine triangle passes all five, and a constructed RED report errors — the
/// exact `?` the stage propagates to abort the build around an unlawful correspondence.
#[test]
fn stage_asserts_five_correspondence_gates_as_hard_fail() {
    use gmeow_logic_compile::ir::{
        Correspondence, CorrespondenceRelation, MorphismClass, MorphismKind, PreservationKind,
    };
    use gmeow_logic_compile::projections::correspondence_gates::{assert_gates, evaluate_gates};

    // The production affine triangle passes the five gates: the wiring will not spuriously
    // fail the build (and the full stage `run` succeeds in the sibling tests).
    let (gated, _) = synthetic_affine_program()
        .with_derived_puts()
        .expect("derive affine put legs");
    let verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
    assert_gates(&evaluate_gates(&gated, &[], &verdicts))
        .expect("the §14 affine triangle is lawful");

    // A bridge view declaring equivalence is an overclaim RED → `assert_gates` errors,
    // which is precisely what the stage propagates as a `gmeow_errors::Diag` build failure.
    let bridge = Correspondence::new(
        "https://gmeow.example/corr/bridge".to_owned(),
        CorrespondenceRelation::Equiv,
        MorphismClass::BridgeView,
        MorphismKind::CommitmentShiftingBridge,
        false,
        None,
        Some("https://gmeow.example/corr/bridgeGet".to_owned()),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("well-formed bridge correspondence");
    let red = CorrespondenceProgram::new(vec![bridge], PreservationKind::SoundUnder);
    let red_verdicts = gmeow_logic::correspondence_exec::program_verdicts(&red);
    assert_gates(&evaluate_gates(&red, &[], &red_verdicts))
        .expect_err("a bridge-view equivalence overclaim must HARD-fail the build");
}

/// Production-path negative control for recovery/leg semantic coupling. The source is
/// parsed through the real Turtle frontend, receives the production derived put, executes
/// through `program_verdicts`, compiles through `compile_program`, and reaches the
/// exact `assert_gates` boundary used by this stage. Changing only the authored get-leg
/// body while holding the RecoveryCase fixed must therefore red both recovery gates.
#[test]
fn stage_recovery_gates_consume_the_resolved_get_leg_body() {
    use gmeow_logic_compile::frontend::Severity;
    use gmeow_logic_compile::projections::correspondence_gates::GateVerdict;

    const SOURCE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:correspondence a logic:Correspondence ;
    logic:correspondenceRelation logic:Subsumes ;
    logic:morphismClass logic:SectionRetraction ;
    logic:morphismKind logic:InstitutionMorphism ;
    logic:mnemomorphic true ;
    logic:getLeg ex:get ;
    logic:recoveryCase ex:case .

ex:get a logic:TransactionProgram ;
    gmeow:path ex:sourceRel .

ex:case a logic:RecoveryCase ;
    logic:recoveryTransform ex:transform .

ex:transform a logic:Formula ;
    logic:quantifiedVariable
        [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
        [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ] ;
    logic:forall [
        a logic:Formula ;
        logic:antecedent [
            a logic:Formula ;
            logic:relation ex:sourceRel ;
            logic:argument
                [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
                [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ]
        ] ;
        logic:consequent [
            a logic:Formula ;
            logic:relation ex:viewRel ;
            logic:argument
                [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
                [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ]
        ]
    ] .
"#;

    let parse = |source: &str| {
        let (program, diagnostics) = parse_logic_str(
            source,
            Some("https://example.org/recovery-leg-regression".to_owned()),
        )
        .expect("parse recovery correspondence");
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error),
            "unexpected frontend diagnostics: {diagnostics:#?}"
        );
        program
    };

    let baseline = parse(SOURCE);
    let baseline_theory = source_theory(SOURCE);
    compile_source_projections(&baseline_theory, baseline_theory.program())
        .expect("the production boundary accepts the body-aligned recovery");
    assert!(baseline.correspondences[0].put_leg.is_none());
    let mut executed_program = None;
    let baseline_artifacts = compile_program(&baseline, |derived| {
        let put = derived.correspondences[0]
            .put_leg
            .as_deref()
            .expect("the law executor must receive the derived put");
        assert!(derived.resolve_leg(put).is_some());
        executed_program = Some(derived.clone());
        gmeow_logic::correspondence_exec::program_verdicts(derived)
    })
    .expect("compile baseline recovery correspondence");
    assert_eq!(
        baseline_artifacts.correspondence_program, executed_program,
        "gates and output must use the program whose laws were executed"
    );
    assert_eq!(baseline_artifacts.correspondence_verdicts.len(), 1);
    let baseline_gates = baseline_artifacts
        .correspondence_gates
        .as_ref()
        .expect("baseline correspondence gates");
    assert_gates(baseline_gates).expect("the body-aligned recovery case must pass");
    assert_eq!(
        evaluate_gates(
            baseline_artifacts.correspondence_program.as_ref().unwrap(),
            &[],
            &baseline_artifacts.correspondence_verdicts,
        ),
        *baseline_gates,
        "additional composition grading must consume the retained executed evidence"
    );

    let mutated_source = SOURCE.replacen("gmeow:path ex:sourceRel", "gmeow:path ex:mutatedRel", 1);
    let mutated = parse(&mutated_source);
    let mutated_theory = source_theory(&mutated_source);
    let error = compile_source_projections(&mutated_theory, mutated_theory.program())
        .expect_err("the production boundary must enforce the recorded red gates");
    assert!(
        error.to_string().contains("authored correspondence gate"),
        "{error}"
    );
    assert_eq!(
        baseline.correspondences[0].recovery_cases, mutated.correspondences[0].recovery_cases,
        "the mutation must hold the canonical RecoveryCase fixed"
    );
    let mutated_artifacts =
        compile_program(&mutated, gmeow_logic::correspondence_exec::program_verdicts)
            .expect("compile mutated recovery correspondence");
    let mutated_gates = mutated_artifacts
        .correspondence_gates
        .as_ref()
        .expect("mutated correspondence gates");
    let report = &mutated_gates.per_correspondence[0];
    assert!(matches!(report.round_trip, GateVerdict::Red { .. }));
    assert!(matches!(report.mnemomorphism, GateVerdict::Red { .. }));
    assert_gates(mutated_gates).expect_err(
        "changing only the formerly inert LegPath body must hard-fail the production gates",
    );
}

/// Native relational transport binds the same program and terminal graph identity.
#[test]
fn native_relational_carrier_and_terminal_share_the_complete_program() {
    let mut program = lower_program_with_formulas(&clean_program());
    program.facts[0].negated = true;
    program.push_residue("source obligation retained as unsupported residue");
    let projection = project_relational_core_dataset(&program).unwrap();
    let carrier =
        crate::stages::carrier::rooted_in_graph(&projection, GRAPH_RELATIONAL_CORE).unwrap();
    let carried = carrier.project_named_graph(GRAPH_RELATIONAL_CORE);
    let restored = gmeow_logic_compile::relational_core::parse_relational_core(&carried).unwrap();
    assert_eq!(restored, program);
    assert_eq!(
        crate::stages::superset::canonical_ntriples(&projection).unwrap(),
        crate::stages::superset::canonical_ntriples(&carried).unwrap(),
    );
}

/// Native transport retains the correspondence IR and the terminal graph identity.
#[test]
fn native_correspondence_carrier_and_terminal_share_the_complete_program() {
    let program = synthetic_affine_program();
    let projection = project_correspondence_dataset(&program).expect("native projection");
    let restored = parse_correspondence(&projection).expect("typed correspondence inverse");
    assert_eq!(restored.correspondences, program.correspondences);
    assert_eq!(restored.compositions, program.compositions);
    let carrier = crate::stages::carrier::rooted_in_graph(&projection, GRAPH_CORRESPONDENCE)
        .expect("native carrier placement");
    let carried = carrier.project_named_graph(GRAPH_CORRESPONDENCE);
    assert_eq!(
        parse_correspondence(&carried).unwrap().correspondences,
        program.correspondences
    );
    assert_eq!(
        crate::stages::superset::canonical_ntriples(&projection).unwrap(),
        crate::stages::superset::canonical_ntriples(&carried).unwrap(),
        "the terminal artifact and typed handle must bind the same graph"
    );
}

/// A changed graph identity cannot receive the original correspondence handle.
#[test]
fn pin_correspondence_handle_hard_fails_on_digest_mismatch() {
    let program = synthetic_affine_program();
    let projection =
        project_correspondence_dataset(&program).expect("native correspondence projection");
    let dataset = crate::stages::carrier::rooted_in_graph(&projection, GRAPH_CORRESPONDENCE)
        .expect("graph/correspondence dataset");
    let mut bundle = bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
    let wrong = ContentDigest::of(b"not the correspondence canonical bytes");
    let err = bundle
        .pin_handle(
            GRAPH_CORRESPONDENCE,
            PipelineHandle::Correspondence(Arc::new(program)),
            wrong,
        )
        .expect_err("a mismatched pin must HARD-fail");
    assert!(
        matches!(
            err,
            purrdf::PipelineBundleError::HandleDigestMismatch { .. }
        ),
        "the Correspondence handle pin must fail closed on a digest mismatch, got {err:?}"
    );
}
