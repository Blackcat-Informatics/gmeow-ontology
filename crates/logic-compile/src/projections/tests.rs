// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection tests — unit checks plus the **insta snapshot goldens** (T8,
//! every projection of every `conformance/logic/cases/projections/*` case
//! is pinned by a committed `.snap` golden (text targets byte-for-byte; RDF
//! targets as a canonicalized sorted triple-set, since no golden uses blank
//! nodes). The `.snap` files ARE the byte-exact unit golden; cross-engine
//! semantic corpus parity over the same `expected/` files is owned by the native
//! `crates/conformance` harness (graph-isomorphism + bless), which is untouched.

use std::path::PathBuf;

use super::*;
use crate::frontend::parse_logic_str;
use crate::loss_ledger::LossLedger;

/// The per-run ACTUAL drop notes for `target`, recovered from the loss store with the
/// report's `actual: ` read-back prefix stripped — exactly the old
/// `ProjectionResult::actual_drops` (structural notes read back unprefixed are excluded, so a
/// structural note that happens to share a substring never masks an actual-drop assertion).
fn actual_drops(loss: &LossLedger, target: &str) -> Vec<String> {
    loss.projection_drops_for(target)
        .iter()
        .filter_map(|d| d.strip_prefix("actual: ").map(str::to_owned))
        .collect()
}

// ── Unit: helpers ────────────────────────────────────────────────────────────

#[test]
fn python_repr_matches_cpython() {
    assert_eq!(python_repr("0.9"), "'0.9'");
    assert_eq!(python_repr("hello"), "'hello'");
    // Contains a single quote but no double quote → switch to double quotes.
    assert_eq!(python_repr("it's"), "\"it's\"");
    // Contains both → stay single-quoted, escape the single quote.
    assert_eq!(python_repr("a'b\"c"), "'a\\'b\"c'");
    assert_eq!(python_repr("tab\there"), "'tab\\there'");
}

#[test]
fn overclaim_gate_fires_on_exact_with_drops() {
    use crate::ir::PreservationKind;
    assert!(assert_no_overclaim("demo-exact", PreservationKind::Exact, &[]).is_ok());
    let err = assert_no_overclaim(
        "demo-exact",
        PreservationKind::Exact,
        &["dropped something"],
    )
    .unwrap_err();
    assert!(err.0.contains("Overclaim"));
    // SoundUnder with drops is fine.
    assert!(assert_no_overclaim("owl-dl", PreservationKind::SoundUnder, &["x"]).is_ok());
}

#[test]
fn legalization_gate_enforces_unsupported_carries_residue() {
    use crate::ir::PreservationKind;
    // The legalization floor: Unsupported WITH flagged residue is legal (the construct
    // is carried and flagged, not silently dropped).
    assert!(
        assert_no_overclaim(
            "demo",
            PreservationKind::Unsupported,
            &["the construct is inexpressible in demo"],
        )
        .is_ok()
    );
    // Unsupported with an EMPTY residue is a silent under-disclosure — a build failure.
    let err = assert_no_overclaim("demo", PreservationKind::Unsupported, &[]).unwrap_err();
    assert!(
        err.0.contains("under-disclosure"),
        "Unsupported with no residue must hard-fail: {}",
        err.0
    );
}

#[test]
fn report_emits_unsupported_target_and_flags_residue() {
    use crate::ir::{LogicProgram, PreservationKind};
    let program = LogicProgram::new(vec![], vec![], vec![], None);
    // A target that declares Unsupported and carries its residue serializes the floor
    // kind and flags the residue — the reserved-machinery path the correspondence
    // up-lift / OWL-alignment lowering is the first production producer of.
    let mut loss = LossLedger::new();
    loss.record_projection_drops(
        "demo-unsupported",
        PreservationKind::Unsupported,
        &["the whole construct is inexpressible in demo".to_owned()],
        &[],
    );
    let unsupported = ProjectionResult {
        target: "demo-unsupported".to_owned(),
        content: String::new(),
        is_rdf: false,
        preservation: PreservationKind::Unsupported,
        complexity: "N/A".to_owned(),
    };
    let ttl = report::build_projection_report(&program, &[unsupported], &loss).unwrap();
    assert!(
        ttl.contains("Unsupported"),
        "declares the Unsupported kind:\n{ttl}"
    );
    assert!(ttl.contains("lossyDrop"), "flags the residue:\n{ttl}");
}

#[test]
fn report_rejects_unsupported_with_no_residue() {
    use crate::ir::{LogicProgram, PreservationKind};
    let program = LogicProgram::new(vec![], vec![], vec![], None);
    let silent = ProjectionResult {
        target: "demo-silent".to_owned(),
        content: String::new(),
        is_rdf: false,
        preservation: PreservationKind::Unsupported,
        complexity: "N/A".to_owned(),
    };
    // No drops interned for this target → the store returns an empty residue, which is the
    // silent under-disclosure the gate must reject.
    let err = report::build_projection_report(&program, &[silent], &LossLedger::new()).unwrap_err();
    assert!(err.0.contains("under-disclosure"), "got: {}", err.0);
}

#[test]
fn projection_ledger_rows_are_sorted_and_classified() {
    let rows = projection_ledger_rows();
    assert!(!rows.is_empty(), "the static ledger must carry rows");

    // Deterministic: sorted by target, no duplicates.
    let mut sorted = rows.clone();
    sorted.sort_by(|a, b| a.target.cmp(&b.target));
    assert_eq!(rows, sorted, "rows must be returned sorted by target");

    let find = |t: &str| {
        rows.iter()
            .find(|r| r.target == t)
            .unwrap_or_else(|| panic!("ledger must carry the {t:?} target"))
    };

    // The identity serialization preserves everything (no drops).
    let canonical = find("canonical-rdf12");
    assert_eq!(canonical.preservation_kind, "ExactPreservation");
    assert!(
        canonical.lossy_drops.is_empty(),
        "canonical-rdf12 is exact: {:?}",
        canonical.lossy_drops
    );

    // A lossy down-projection records its structural drops.
    let owl_dl = find("owl-dl");
    assert!(
        !owl_dl.lossy_drops.is_empty(),
        "owl-dl is a lossy projection and must declare lossy_drops"
    );

    // The EmotionML emitter is a many-to-one lossy projection: it must appear in the static
    // ledger and its structural drops must name the collapsed affect families (rule 9).
    let emotionml = find("emotionml");
    assert!(
        emotionml
            .lossy_drops
            .iter()
            .any(|d| d.contains("AffectClassifierOutput") && d.contains("envelope")),
        "emotionml must declare its many-to-one <emotion> envelope collapse: {:?}",
        emotionml.lossy_drops
    );
}

// ── Unit: rule emission (text targets, not exercised by the axiom-only cases) ──

#[test]
fn datalog_rule_emits_world_var_and_guard() {
    let prog = parse(
        "ex:r a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:distinctBody [ rdf:subject \"?x\" ; rdf:object \"?y\" ] .",
    );
    let dl = text::project_datalog(&prog, &mut LossLedger::new());
    assert!(dl.content.contains("rel(?x, ?y, ?C) :-"), "{}", dl.content);
    assert!(dl.content.contains("?x != ?y"), "{}", dl.content);
}

// ── The parity gate ──────────────────────────────────────────────────────────

fn conformance_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/logic/cases/projections")
}

fn parse(ttl: &str) -> LogicProgram {
    let prefixes = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";
    parse_logic_str(&format!("{prefixes}{ttl}"), None)
        .expect("parse ok")
        .0
}

/// Canonical sorted triple lines of a Turtle document (default graph), for
/// triple-set equality (valid because no golden uses blank nodes).
///
/// Wasm-clean: native codec parse → frozen IR → canonical N-Triples of the
/// default graph, one sorted line per triple (the trailing ` .` is stripped so a
/// line reads `<s> <p> <o>`). No oxigraph Store — the compiler crate's test harness
/// rides the same `gmeow-rdf` `gts` surface the projections themselves use.
fn triple_set(turtle: &str) -> Vec<String> {
    use purrdf::{SerializeGraph, parse_dataset, serialize_dataset};
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("turtle parse failed: {e}\n---\n{turtle}"));
    let nt = serialize_dataset(
        dataset.as_ref(),
        "application/n-triples",
        SerializeGraph::DefaultGraph,
    )
    .unwrap_or_else(|e| panic!("n-triples serialize failed: {e}\n---\n{turtle}"));
    let nt = String::from_utf8(nt).expect("n-triples is valid UTF-8");
    let mut lines: Vec<String> = nt
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.strip_suffix(" .").unwrap_or(l).to_owned())
        .collect();
    lines.sort();
    lines
}

/// Canonical sorted triple-set rendering of a Turtle document, as a single
/// newline-joined string suitable for a byte-stable insta snapshot. Reuses
/// [`triple_set`] (default graph, blank-node-free goldens) so quoted/reifier
/// terms of `canonical-rdf12` / the projection report are captured as object
/// terms in deterministic sorted order — NEVER raw oxigraph Turtle, whose blank
/// labels and statement order are non-deterministic.
fn rdf_snapshot(turtle: &str) -> String {
    triple_set(turtle).join("\n")
}

/// Pin every projection of one conformance case with insta snapshot goldens.
///
/// The `.snap` files are the byte-exact unit golden; the native
/// `crates/conformance` harness owns cross-engine semantic parity over the same
/// `expected/` corpus, so this only re-compiles the case `input.logic.ttl` (it
/// no longer reads the `expected/projections/` files).
fn run_case(case: &str) {
    let dir = conformance_dir().join(case);
    let input = std::fs::read_to_string(dir.join("input.logic.ttl")).expect("read input");
    let (program, diags) = parse_logic_str(&input, None).expect("parse conformance input");
    assert!(
        diags.is_empty(),
        "[{case}] unexpected parse diagnostics: {diags:?}"
    );
    let arts = compile_program(&program, &Default::default()).expect("compile");

    // One `.snap` per (case, target). The per-case suffix keeps the goldens
    // discoverable and avoids a single mega-snapshot.
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(case);
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        // Text targets: byte-identical (the front-end canonicalizes blank labels).
        insta::assert_snapshot!("datalog", arts.datalog);
        insta::assert_snapshot!("n3", arts.n3);

        // RDF targets: canonicalized sorted triple-set.
        insta::assert_snapshot!("owl-dl", rdf_snapshot(&arts.owl_dl));
        insta::assert_snapshot!("owl-el", rdf_snapshot(&arts.owl_el));
        insta::assert_snapshot!("gufo", rdf_snapshot(&arts.gufo));
        insta::assert_snapshot!("canonical-rdf12", rdf_snapshot(&arts.canonical_rdf12));
        insta::assert_snapshot!("projection-report", rdf_snapshot(&arts.report));
    });
}

// ── ReasoningContract round-trip ─────────────────────────────────────────────
//
// The canonical RDF 1.2 projection of a ReasoningContract must be LOSSLESS:
// re-parsing the emitted triples through `extract_contracts` (via parse_logic_str)
// reconstructs the byte-identical contract (same `sort_key()`), whether the
// contract originated from a preset or from direct facets.

use crate::ir::{ReasoningContract, SemanticProfileId};

/// Build a fully-loaded preset contract exercising every facet kind.
fn loaded_preset_contract() -> ReasoningContract {
    let mut c = ReasoningContract::from_preset(SemanticProfileId::ProceduralProlog);
    c.formula_fragment = Some("HornFragment".to_owned());
    c.model_semantics = Some("LeastModelSemantics".to_owned());
    c.truth_algebra = Some("BooleanAlgebra".to_owned());
    c.admissible_valuation = Some("ForbidGap".to_owned());
    c.designated_values = Some("TrueDesignated".to_owned());
    c.evolution = Some("StaticEvolution".to_owned());
    c.argumentation = Some("GroundedSemantics".to_owned());
    // MonotonicRevision (not EntrenchmentRevision) so the counterfactual-coupling
    // compatibility rules do not fire — this contract must be SUPPORTED so the
    // round-trip reparse is diagnostic-free.
    c.revision = Some("MonotonicRevision".to_owned());
    c.equality_policy = Some("UniqueNameAssumption".to_owned());
    c.default_closure = Some("OpenWorldClosure".to_owned());
    c.negation_operators.insert("DefaultNegation".to_owned());
    c.negation_operators.insert("ExplicitNegation".to_owned());
    c.context_axes.insert("StandpointContextAxis".to_owned());
    c.uncertainty_measures
        .insert("PossibilityMeasure".to_owned());
    c.resource_policies.insert("ProceduralExecution".to_owned());
    c.resource_policies
        .insert("BudgetBoundedResource".to_owned());
    c.projection_targets.insert("OwlProjection".to_owned());
    c.closure_entries
        .insert("ex:closedPred".to_owned(), "ClosedWorldClosure".to_owned());
    c.closure_entries
        .insert("ex:openPred".to_owned(), "OpenWorldClosure".to_owned());
    c.complexity = Some(crate::ir::ComplexityClass::new("PTIME").unwrap());
    c
}

/// Reconstruct the program from the canonical RDF 1.2 projection bytes.
fn reparse_canonical(program: &LogicProgram) -> LogicProgram {
    let proj = rdf::project_canonical_rdf12(program).expect("project ok");
    let (reparsed, diags) = parse_logic_str(&proj.content, None).expect("reparse ok");
    assert!(
        diags.is_empty(),
        "round-trip reparse produced diagnostics: {diags:?}"
    );
    reparsed
}

#[test]
fn annotations_round_trip_through_canonical_rdf12_at_exact_preservation() {
    // R1/R3/AC1: the lifted RDFS/SKOS annotation surface projects back out through the
    // canonical RDF-1.2 put leg and re-parses to the IDENTICAL annotation axioms — put ∘ get
    // = id at ExactPreservation. The carrier tag must survive both legs.
    let ttl = "@prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
               @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
               @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
               gm:Widget rdfs:label \"Widget\"@x-gmeow-english ;\n\
                   rdfs:comment \"A widget, canonically.\"@x-gmeow-english ;\n\
                   skos:definition \"The canonical widget concept.\"@x-gmeow-english ;\n\
                   skos:prefLabel \"widget\"@x-gmeow-english ;\n\
                   skos:altLabel \"gadget\"@x-gmeow-english ;\n\
                   skos:scopeNote \"Use for gizmos.\"@x-gmeow-english .";
    let (program, _d) = parse_logic_str(ttl, None).expect("parse ok");

    let ann = |p: &LogicProgram| -> Vec<crate::ir::LogicAxiom> {
        let mut v: Vec<_> = p
            .axioms
            .iter()
            .filter(|a| a.node_kind == crate::ir::NodeKind::Annotation)
            .cloned()
            .collect();
        v.sort_by_key(crate::ir::LogicAxiom::sort_key);
        v
    };
    let before = ann(&program);
    assert_eq!(before.len(), 6, "get: six annotation axioms lifted");

    // The projected Turtle carries the carrier tag, NOT an untyped literal.
    let proj = rdf::project_canonical_rdf12(&program).expect("project ok");
    assert!(
        proj.content.contains("@x-gmeow-english"),
        "the carrier language tag must be preserved in the canonical RDF-1.2 projection"
    );

    // put ∘ get = id: re-parsing the projection yields the identical annotation axioms.
    let reparsed = reparse_canonical(&program);
    assert_eq!(
        ann(&reparsed),
        before,
        "annotation axioms must round-trip identically (ExactPreservation)"
    );
}

#[test]
fn contract_round_trip_preset_with_full_facets() {
    let original = loaded_preset_contract();
    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1, "exactly one contract survives");
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "preset contract sort_key must round-trip exactly"
    );
    assert_eq!(
        *got, original,
        "preset contract must reconstruct identically"
    );
}

#[test]
fn aggregation_rule_round_trips_through_canonical_rdf12() {
    use crate::ir::{AggregateSpec, ContextualScope, LogicAxiom, LogicRule};
    // ?g gmeow:total ?sum :- ?g gmeow:hasItem ?x  [ SUM(?x) AS ?sum GROUP BY ?g ]
    let head = LogicAxiom::new(
        "?g",
        "https://blackcatinformatics.ca/gmeow/total",
        "?sum",
        false,
        false,
        ContextualScope::default(),
    )
    .unwrap();
    let body = vec![
        LogicAxiom::new(
            "?g",
            "https://blackcatinformatics.ca/gmeow/hasItem",
            "?x",
            false,
            false,
            ContextualScope::default(),
        )
        .unwrap(),
    ];
    let rule = LogicRule::new(head, body, vec![], ContextualScope::default()).with_aggregation(
        AggregateSpec::new("SUM", "?x", "?sum", vec!["?g".to_owned()]),
    );
    let program = LogicProgram::new(vec![], vec![rule.clone()], vec![], None);

    let reparsed = reparse_canonical(&program);
    assert_eq!(reparsed.rules.len(), 1, "exactly one rule survives");
    let got = &reparsed.rules[0];
    // The aggregation spec must round-trip exactly through the Exact canonical RDF 1.2 target —
    // this is the C5 invariant (the head/body atoms carry the standard variable-as-literal
    // convention, orthogonal to aggregation).
    assert_eq!(
        got.aggregation, rule.aggregation,
        "the aggregation spec must round-trip through canonical RDF 1.2"
    );
    assert_eq!(
        got.head.predicate, rule.head.predicate,
        "the reduce rule head predicate must round-trip"
    );
}

#[test]
fn contract_round_trip_anonymous_faceted_contract() {
    // No preset — exercises the minted logic:contract/_NNNNNN subject node.
    let mut original = ReasoningContract::new();
    original.model_semantics = Some("WellFoundedSemantics".to_owned());
    original
        .negation_operators
        .insert("DefaultNegation".to_owned());
    original.default_closure = Some("OpenWorldClosure".to_owned());
    original
        .closure_entries
        .insert("ex:k".to_owned(), "ClosedWorldClosure".to_owned());
    original.complexity = Some(crate::ir::ComplexityClass::new("PTIME").unwrap());

    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1);
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "anonymous contract sort_key must round-trip exactly"
    );
    assert_eq!(*got, original);
}

#[test]
fn contract_round_trip_custom_iri_facet_value() {
    // The open facet-value vocabulary admits a value that is a full
    // CUSTOM IRI (not under the logic: namespace). The projection must emit it
    // verbatim (never re-prefixed to a corrupt `…/logic/https://…`) and storage
    // must keep the full IRI, so project → reparse round-trips identically.
    let mut original = ReasoningContract::new();
    original.model_semantics = Some("https://example.org/MyModelSemantics".to_owned());

    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1);
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.model_semantics.as_deref(),
        Some("https://example.org/MyModelSemantics"),
        "custom-IRI facet value must round-trip verbatim (no double-prefix)"
    );
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "custom-IRI facet value contract sort_key must round-trip exactly"
    );
    assert_eq!(*got, original);
}

#[test]
fn contract_round_trip_passes_ir_isomorphism_gate_on_contracts() {
    // Reviewer B1: exercise the IR-isomorphism gate's CONTRACT branch with a
    // NON-empty contract set (the adapter fixtures only ever carry empty
    // contracts). The gate keys contracts on `sort_key()` (the `contract_key`
    // helper), so a program built from only the original contracts and one built
    // from only the round-tripped contracts must compare contract-isomorphic.
    //
    // (The full program is intentionally NOT asserted equal: like the rule
    // projection, the canonical-RDF12 facet triples are themselves logic:-predicate
    // triples that the axiom extractor also reads, so the round-trip is an
    // EXACT reconstruction of the *contract*, not a no-op over the whole graph.)
    use crate::adapter::assert_ir_isomorphic;
    let contracts = vec![loaded_preset_contract(), {
        let mut c = ReasoningContract::new();
        c.model_semantics = Some("WellFoundedSemantics".to_owned());
        c.negation_operators.insert("DefaultNegation".to_owned());
        c
    }];
    let program = LogicProgram::new(vec![], vec![], contracts, None);
    let reparsed = reparse_canonical(&program);

    let contracts_only = LogicProgram::new(vec![], vec![], program.contracts.clone(), None);
    let reparsed_contracts_only =
        LogicProgram::new(vec![], vec![], reparsed.contracts.clone(), None);
    assert_ir_isomorphic(&contracts_only, &reparsed_contracts_only)
        .expect("contracts must round-trip through the isomorphism gate's contract branch");
}

#[test]
fn contract_round_trip_is_exact_preservation_no_drops() {
    // The canonical-rdf12 target is ExactPreservation: projecting a contract-bearing
    // program must not trip the overclaim gate (zero actual drops on that target).
    let program = LogicProgram::new(vec![], vec![], vec![loaded_preset_contract()], None);
    let proj = rdf::project_canonical_rdf12(&program).expect("project ok");
    // canonical-rdf12 is Exact: it never touches a loss store, so its residue is empty.
    let canon_loss = LossLedger::new();
    assert!(
        canon_loss.projection_drops_for(&proj.target).is_empty(),
        "canonical-rdf12 must drop nothing"
    );
    // The lossy targets, by contrast, RECORD the contract as a drop.
    let mut loss = LossLedger::new();
    let _owl = rdf::project_owl_dl(&program, &mut loss).expect("owl-dl ok");
    let owl_drops = actual_drops(&loss, "owl-dl");
    assert!(
        owl_drops.iter().any(|d| d.contains("reasoning contract")),
        "owl-dl must record the dropped contract: {owl_drops:?}"
    );
}

#[test]
fn parity_confidence_scoped_axiom() {
    run_case("confidence-scoped-axiom");
}

#[test]
fn parity_kind_hierarchy() {
    run_case("kind-hierarchy");
}

#[test]
fn parity_relator_mediation() {
    run_case("relator-mediation");
}

// ── Validation-shape projection goldens (shacl-core / shex) ───────────────────
//
// Byte-exact + graph-isomorphic + well-formedness + determinism goldens over the
// REAL `compile_program` emit path for the two closed-world shape surfaces. The
// three `run_case` conformance cases carry no `logic:ValidationShape`, so these
// surfaces need a purpose-built, full-surface fixture to be pinned non-vacuously.

/// A fixture `LogicProgram` whose ONLY populated content is `validation_shapes`
/// (no axioms/rules/vocabulary/FoldView), exercising every `ConstraintComponent`
/// variant and every `ShapeTarget` kind, so the projection goldens saturate the
/// emitter and a non-empty ShEx block PROVES the surface is derived from the shared
/// shape node — not from a vocabulary domain/range re-derivation.
fn full_surface_validation_program() -> crate::ir::LogicProgram {
    use crate::ir::{
        ConstraintComponent as C, ConstraintProvenance, LogicProgram, PropertyConstraintIr as P,
        ShaclNodeKind, ShaclSeverity, ShapeTarget, ShapeValue, ValidationShapeIr as V,
    };
    let base = "https://example.org/test/validation-full-surface/";
    let x = |frag: &str| format!("{base}{frag}");

    // Shape A: a class target exercising the faithful + lossy component surface.
    let a = V::new(
        x("A-shape"),
        ShapeTarget::Class(x("A")),
        vec![
            // Faithful string facets on a string-typed property (min/max LENGTH, not numeric).
            P::new(
                x("p-scalar"),
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![
                    C::Datatype("http://www.w3.org/2001/XMLSchema#string".into()),
                    C::MinLength(2),
                    C::MaxLength(64),
                ],
            )
            .unwrap(),
            // A whole-valued numeric interval on a numeric-typed property (numeric facets belong on
            // a numeric datatype — the ShEx well-formedness gate rejects them on xsd:string).
            P::new(
                x("p-numeric"),
                None,
                None,
                None,
                vec![
                    C::Datatype("http://www.w3.org/2001/XMLSchema#decimal".into()),
                    C::NumericRange {
                        min: Some(0.0),
                        max: Some(10.0),
                        min_inclusive: true,
                        max_inclusive: false,
                    },
                ],
            )
            .unwrap(),
            // An out-of-i64 numeric bound to hit the decimal branch of `format_bound`.
            P::new(
                x("p-bignum"),
                None,
                None,
                None,
                vec![
                    C::Datatype("http://www.w3.org/2001/XMLSchema#decimal".into()),
                    C::NumericRange {
                        min: Some(1.0e16),
                        max: None,
                        min_inclusive: true,
                        max_inclusive: false,
                    },
                ],
            )
            .unwrap(),
            // A closed value set of mixed term kinds, plus OPT-native cardinality.
            P::new(
                x("p-in"),
                Some(0),
                Some(3),
                Some(ConstraintProvenance::OptNative),
                vec![C::In(vec![
                    ShapeValue::Iri(x("v1")),
                    ShapeValue::Literal {
                        lexical: "typed".into(),
                        datatype: Some("http://www.w3.org/2001/XMLSchema#token".into()),
                        lang: None,
                    },
                    ShapeValue::Literal {
                        lexical: "bonjour".into(),
                        datatype: None,
                        lang: Some("fr".into()),
                    },
                ])],
            )
            .unwrap(),
            // Faithful-in-SHACL / lossy-in-ShEx facets (DateTimeRange, LanguageIn), + a lossy
            // regex-dialect Pattern; carried at Warning severity.
            P::new(
                x("p-mixed"),
                None,
                None,
                None,
                vec![
                    C::Pattern {
                        regex: "^[A-Z]".into(),
                        flags: Some("i".into()),
                    },
                    C::LanguageIn(vec!["en".into(), "fr".into()]),
                    C::DateTimeRange {
                        min: Some("2020-01-01T00:00:00Z".into()),
                        max: None,
                        min_inclusive: true,
                        max_inclusive: false,
                    },
                ],
            )
            .unwrap()
            .with_severity(ShaclSeverity::Warning),
            // hasValue + qualified value-shape + negation, with a bespoke message.
            P::new(
                x("p-logical"),
                None,
                None,
                None,
                vec![
                    C::HasValue(ShapeValue::Iri(x("fixed"))),
                    C::QualifiedValueShape {
                        shape: vec![C::Class(x("Q"))],
                        min: Some(1),
                        max: None,
                    },
                    C::Not(Box::new(C::Class(x("Disjoint")))),
                ],
            )
            .unwrap()
            .with_message("p-logical must satisfy the qualified value shape")
            .unwrap(),
            // An inverse path with a node-kind constraint (owl:InverseFunctionalProperty reading).
            P::new(
                x("p-inv"),
                Some(0),
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![C::NodeKindShacl(ShaclNodeKind::Iri)],
            )
            .unwrap()
            .inverted(),
            // OPT-lossy family: precision, terminology, ordinal, datetime pattern.
            P::new(
                x("p-opt"),
                None,
                None,
                None,
                vec![
                    C::PrecisionRange {
                        min: Some(0.0),
                        max: Some(2.0),
                        min_inclusive: true,
                        max_inclusive: true,
                    },
                    C::TerminologyBinding {
                        terminology_id: "SNOMED-CT".into(),
                        codes: vec!["12345".into(), "67890".into()],
                    },
                    C::OrdinalSet {
                        pairs: vec![(0, x("low")), (1, x("high"))],
                    },
                    C::DateTimePattern("yyyy-mm-ddTHH:MM:SS".into()),
                ],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
    .with_label("Full-surface validation shape A")
    .unwrap();

    // Shape B: a subjects-of (rdfs:domain closed-world) target with focus-node components.
    let b = V::new(
        x("p-domain-shape"),
        ShapeTarget::SubjectsOf(x("p-domain")),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![
        C::Class(x("DomainClass")),
        C::Not(Box::new(C::Class(x("ExcludedClass")))),
    ])
    .unwrap();

    // Shape C: an objects-of (rdfs:range closed-world) target.
    let c = V::new(
        x("p-range-shape"),
        ShapeTarget::ObjectsOf(x("p-range")),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(vec![C::Datatype(
        "http://www.w3.org/2001/XMLSchema#anyURI".into(),
    )])
    .unwrap();

    // Shape D: a value-keyed (sh:SPARQLTarget) selection — no ShEx form.
    let d = V::new(
        x("D-shape"),
        ShapeTarget::ValueKeyed {
            predicate: x("kind"),
            value: x("SpecialKind"),
        },
        vec![
            P::new(
                x("p-d"),
                Some(1),
                None,
                Some(ConstraintProvenance::OptNative),
                vec![C::Class(x("DTarget"))],
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap();

    // Shape E: RDF-1.2 reifier + reification-required (property-level) + standpoint-indexed (all
    // ShEx residue). The reifier component is keyed to the path `p-e`, where the native SHACL 1.2
    // engine reads it.
    let e = V::new(
        x("E-shape"),
        ShapeTarget::Class(x("E")),
        vec![
            P::new(
                x("p-e"),
                None,
                None,
                None,
                vec![C::NodeKindShacl(ShaclNodeKind::Iri)],
            )
            .unwrap()
            .with_reifier(Some(x("E-reifier-shape")), true)
            .unwrap(),
        ],
        Some(x("standpoint-clinical")),
    )
    .unwrap();

    LogicProgram::new(vec![], vec![], vec![], Some(x("program")))
        .with_validation_shapes(vec![a, b, c, d, e])
}

#[test]
fn validation_shape_projection_goldens() {
    use crate::ir::PreservationKind;
    use std::collections::BTreeSet;

    let program = full_surface_validation_program();

    // The REAL production path: the ledgered projections the pipeline writes to
    // generated/shapes/validation-shapes.{ttl,shex}. Hard-fail if a target is absent.
    let arts = compile_program(&program, &Default::default()).expect("compile");
    let find = |t: &str| {
        arts.logic_projections
            .iter()
            .find(|p| p.target == t)
            .unwrap_or_else(|| panic!("{t} projection present"))
    };
    let shacl = find("shacl-core");
    let shex = find("shex");

    // C1 determinism: compiling the same program twice yields byte-identical surfaces.
    let arts2 = compile_program(&program, &Default::default()).expect("recompile");
    let find2 = |t: &str| {
        arts2
            .logic_projections
            .iter()
            .find(|p| p.target == t)
            .unwrap()
            .content
            .clone()
    };
    assert_eq!(
        shacl.content,
        find2("shacl-core"),
        "shacl-core is non-deterministic"
    );
    assert_eq!(shex.content, find2("shex"), "shex is non-deterministic");

    // C2 ledger polarity: both surfaces validate, never entail.
    assert_eq!(shacl.preservation, PreservationKind::ValidationOnly);
    assert_eq!(shex.preservation, PreservationKind::ValidationOnly);

    // C3 shared shape node: the fixture set ONLY validation_shapes (no vocabulary/FoldView),
    // so a non-empty ShEx shape block proves the surface is derived from the shape node.
    assert!(!shex.content.trim().is_empty(), "shex document is empty");
    assert!(
        shex.content.contains('{'),
        "shex carries no shape block:\n{}",
        shex.content
    );
    assert!(
        shacl.content.contains("sh:targetClass") && shacl.content.contains("sh:reifierShape"),
        "shacl-core missing target/reifier:\n{}",
        shacl.content
    );

    // Well-formedness, independent of the blessed bytes: the SHACL parses as Turtle and the
    // ShEx parses through purrdf's conformance-tested ShExC parser.
    purrdf::parse_dataset(shacl.content.as_bytes(), "text/turtle", None).unwrap_or_else(|e| {
        panic!(
            "emitted SHACL Core is not valid Turtle: {e}\n{}",
            shacl.content
        )
    });
    purrdf::shex::parse_shexc(&shex.content, None)
        .unwrap_or_else(|e| panic!("emitted ShEx is not valid ShExC: {e}\n{}", shex.content));

    // Per-shape residue: shex_residue ⊇ shacl_residue by MEMBERSHIP for every shape (the
    // "different residue sets" acceptance claim), and a rendering that localizes a flip.
    let mut residue_render = String::new();
    for s in &program.validation_shapes {
        let sc: BTreeSet<String> = shapes::shacl_residue(s).into_iter().collect();
        let sx: BTreeSet<String> = shapes::shex_residue(s).into_iter().collect();
        assert!(
            sc.is_subset(&sx),
            "shex residue must be a superset of shacl residue for {}:\nshacl={sc:#?}\nshex={sx:#?}",
            s.iri
        );
        residue_render.push_str(&format!("== {} ==\n-- shacl-core --\n", s.iri));
        for line in &sc {
            residue_render.push_str(line);
            residue_render.push('\n');
        }
        residue_render.push_str("-- shex (delta over shacl) --\n");
        for line in sx.difference(&sc) {
            residue_render.push_str(line);
            residue_render.push('\n');
        }
        residue_render.push('\n');
    }

    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        // Text surfaces (is_rdf:false) → raw byte golden, like datalog/n3.
        insta::assert_snapshot!("validation-shacl-core", shacl.content);
        insta::assert_snapshot!("validation-shex", shex.content);
        // Graph-isomorphic view of the SHACL — pins semantic content independent of ordering.
        insta::assert_snapshot!("validation-shacl-core-graph", rdf_snapshot(&shacl.content));
        // Per-shape residue — pins the ledger drops and localizes a classification flip.
        insta::assert_snapshot!("validation-residue", residue_render);
    });
}

#[test]
fn derived_validation_shapes_project_golden() {
    // Dogfood the FULL derive→project spine (not only the emit half): an authored OWL/RDFS
    // constraint fragment is lowered by the real `derive_validation_shapes` derivation — the same
    // one the pipeline runs to produce generated/shapes/validation-shapes.{ttl,shex} — and the
    // resulting shapes are projected and pinned. This proves the authored ground → SHACL/ShEx
    // path end-to-end, so a regression anywhere from derivation to emit trips a golden.
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/validation-derive.ttl");
    let ttl = std::fs::read(&fixture).expect("read validation-derive.ttl fixture");
    let dataset =
        purrdf::parse_dataset(&ttl, "text/turtle", None).expect("parse the authored fragment");
    let shapes =
        crate::frontend::derive_validation_shapes(dataset.as_ref()).expect("derive shapes");
    assert!(
        !shapes.is_empty(),
        "the authored fragment must derive at least one validation shape"
    );
    assert!(
        shapes.iter().any(|s| !shapes::shacl_residue(s).is_empty()),
        "the fragment must derive a residue-bearing shape (the faceted-datatype regex on g:bic), \
         so the dogfood exercises the loss ledger end-to-end"
    );

    let program = crate::ir::LogicProgram::new(
        vec![],
        vec![],
        vec![],
        Some("urn:test:validation-derive".into()),
    )
    .with_validation_shapes(shapes);
    let arts = compile_program(&program, &Default::default()).expect("compile");
    let content = |t: &str| {
        arts.logic_projections
            .iter()
            .find(|p| p.target == t)
            .unwrap_or_else(|| panic!("{t} projection present"))
            .content
            .clone()
    };
    let shacl = content("shacl-core");
    let shex = content("shex");

    // Well-formedness independent of the blessed bytes.
    purrdf::parse_dataset(shacl.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("derived SHACL Core is not valid Turtle: {e}\n{shacl}"));
    purrdf::shex::parse_shexc(&shex, None)
        .unwrap_or_else(|e| panic!("derived ShEx is not valid ShExC: {e}\n{shex}"));

    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_snapshot!("derived-shacl-core", shacl);
        insta::assert_snapshot!("derived-shacl-core-graph", rdf_snapshot(&shacl));
        insta::assert_snapshot!("derived-shex", shex);
    });
}

#[test]
fn shacl_af_projection_golden() {
    // The adjacent emit-only surface: the SHACL-AF (derivation `sh:SPARQLRule`) projection shares
    // the identical structural gap as the validation-shape surfaces — `is_rdf:false`, byte-compared,
    // previously pinned by no focused insta golden. The three `run_case` conformance cases carry no
    // rules, so a rule-bearing fixture is required. Built via the `parse(...)` helper so the fixture
    // is authored as declarative `logic:` Turtle, not hand-built IR.
    let program = parse(
        "ex:r a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:distinctBody [ rdf:subject \"?x\" ; rdf:object \"?y\" ] .",
    );
    let arts = compile_program(&program, &Default::default()).expect("compile");
    assert!(
        arts.shacl_af.contains("sh:SPARQLRule"),
        "shacl-af must project the rule to a sh:SPARQLRule:\n{}",
        arts.shacl_af
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_snapshot!("shacl-af", arts.shacl_af);
    });
}

// ── G1: path-projection wiring ───────────────────────────────────────────────
//
// Verifies that `compile_program` genuinely populates `path_projections` (not
// dead code) and extends `preservation_ledger` with a `"property-path:<iri>"`
// entry for every declared `logic:PathShape`.  Uses the same nearbyOrgs fixture
// the per-function tests in paths/tests.rs use.

#[test]
fn compile_program_wires_path_projections_and_ledger() {
    let ttl = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:nearbyOrgs a logic:PathShape ;
    logic:pathWildcard true ;
    logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
    logic:pathMinDepth 1 ; logic:pathMaxDepth 2 ; logic:pathDepthParam \"maxDepth\" .";
    let (program, _diags) = parse_logic_str(ttl, None).expect("parse");
    let arts = compile_program(&program, &Default::default()).expect("compile");

    // G1a: path_projections is non-empty and carries both surfaces.
    assert_eq!(
        arts.path_projections.len(),
        1,
        "one PathShape → one PathProjection"
    );
    let pp = &arts.path_projections[0];
    assert_eq!(pp.shape_iri, "https://example.org/test/nearbyOrgs");
    assert!(
        !pp.property_path.is_empty(),
        "property_path surface must be non-empty"
    );
    assert!(!pp.datalog.is_empty(), "datalog surface must be non-empty");

    // G1b: preservation_ledger contains a property-path row keyed by IRI.
    let expected_key = format!("property-path:{}", pp.shape_iri);
    let ledger_entry = arts
        .preservation_ledger
        .iter()
        .find(|e| e.target == expected_key)
        .unwrap_or_else(|| {
            panic!(
                "preservation_ledger must contain a \"{expected_key}\" entry; \
                 found: {:?}",
                arts.preservation_ledger
                    .iter()
                    .map(|e| &e.target)
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(ledger_entry.preservation, "SoundUnderApproximation");
    assert!(
        !ledger_entry.lossy_drops.is_empty(),
        "property-path ledger entry must declare lossy_drops"
    );
}

// ── CR5: the projection report includes the property-path targets ────────────
//
// build_projection_report runs over the SAME target list the preservation ledger
// is built from (path projections are folded into `owned` BEFORE the report), so
// the report Turtle and the ledger must AGREE — both carry a property-path row for
// every declared logic:PathShape (maximal information flow; no suppression).

#[test]
fn projection_report_includes_property_path_targets() {
    let ttl = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:nearbyOrgs a logic:PathShape ;
    logic:pathWildcard true ;
    logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
    logic:pathMinDepth 1 ; logic:pathMaxDepth 2 ; logic:pathDepthParam \"maxDepth\" .";
    let (program, _diags) = parse_logic_str(ttl, None).expect("parse");
    let arts = compile_program(&program, &Default::default()).expect("compile");

    // The report Turtle must carry the property-path target as an rdfs:label, in
    // lock-step with the ledger row keyed `property-path:<iri>`.
    let pp = &arts.path_projections[0];
    let expected_label = format!("property-path:{}", pp.shape_iri);
    assert!(
        arts.report.contains(&expected_label),
        "the projection report must include a property-path target labelled {expected_label:?}; \
         report:\n{}",
        arts.report
    );

    // And it must agree with the preservation ledger (same target list).
    assert!(
        arts.preservation_ledger
            .iter()
            .any(|e| e.target == expected_label),
        "report and ledger must agree on the property-path target {expected_label:?}"
    );
}

// ── AC#9 — Teleology-specific loss disclosure (compile-time) ────────────────
//
// Verify that `compile_program` injects `GOAL_EVAL_COLLAPSE_DROP` into the
// `preservation_ledger` for every LOSSY target when the program carries a
// `gmeow:satisfiedBy` axiom, and leaves exact-preservation targets untouched.

/// Build a minimal `LogicProgram` that carries exactly one axiom whose predicate
/// is `SATISFIED_BY_IRI` (a ground `goal gmeow:satisfiedBy situation` fact).
fn program_with_satisfied_by() -> crate::ir::LogicProgram {
    use crate::ir::{LogicAxiom, LogicProgram};
    let axiom = LogicAxiom::ground(
        "https://example.org/goal",
        SATISFIED_BY_IRI,
        "https://example.org/situation",
        false,
    )
    .expect("valid satisfied-by axiom");
    LogicProgram::new(vec![axiom], vec![], vec![], None)
}

#[test]
fn satisfied_by_axiom_injects_collapse_drop_on_lossy_targets() {
    let program = program_with_satisfied_by();
    let arts = compile_program(&program, &Default::default()).expect("compile ok");

    // Every GOAL_EVAL_COLLAPSE_TARGETS entry must carry the drop note.
    for target in GOAL_EVAL_COLLAPSE_TARGETS {
        let entry = arts
            .preservation_ledger
            .iter()
            .find(|e| e.target == *target)
            .unwrap_or_else(|| panic!("ledger must carry target {target:?}"));
        assert!(
            entry
                .lossy_drops
                .iter()
                .any(|d| d == GOAL_EVAL_COLLAPSE_DROP),
            "target {target:?} must carry the GoalEvaluation collapse drop;              got: {:?}",
            entry.lossy_drops
        );
    }

    // The exact canonical serialization must NOT be augmented.
    let entry = arts
        .preservation_ledger
        .iter()
        .find(|e| e.target == "canonical-rdf12")
        .expect("ledger must carry canonical-rdf12");
    assert!(
        !entry
            .lossy_drops
            .iter()
            .any(|d| d == GOAL_EVAL_COLLAPSE_DROP),
        "canonical-rdf12 must NOT carry the GoalEvaluation collapse drop; got: {:?}",
        entry.lossy_drops
    );
}

#[test]
fn no_satisfied_by_axiom_leaves_ledger_clean() {
    use crate::ir::{LogicAxiom, LogicProgram};
    // A program with a plain subClassOf fact — no satisfiedBy.
    let axiom = LogicAxiom::ground(
        "https://example.org/Bird",
        "https://blackcatinformatics.ca/logic/subClassOf",
        "https://example.org/Animal",
        false,
    )
    .expect("valid axiom");
    let program = LogicProgram::new(vec![axiom], vec![], vec![], None);
    let arts = compile_program(&program, &Default::default()).expect("compile ok");

    // No target in the ledger should carry the collapse drop when there is no
    // satisfiedBy axiom in the program.
    for entry in &arts.preservation_ledger {
        assert!(
            !entry
                .lossy_drops
                .iter()
                .any(|d| d == GOAL_EVAL_COLLAPSE_DROP),
            "target {:?} must NOT carry the GoalEvaluation collapse drop when there              is no satisfiedBy axiom; got: {:?}",
            entry.target,
            entry.lossy_drops
        );
    }
}

// ── Full first-order formula round-trip + residue disclosure ─────────────────

use crate::ir::{Formula, Term};

fn fml_var(name: &str) -> Term {
    Term::var(name).unwrap()
}

fn fml_rel(local: &str, args: Vec<Term>) -> Formula {
    Formula::atom(Term::iri(format!("{LOGIC_NS}{local}")).unwrap(), args).unwrap()
}

/// A representative spread of formula shapes, each non-trivially-Horn at top level:
/// nested quantifier + conjunction + strong negation, an existential disjunction, a
/// free-variable implication, a biconditional, and a sequence-marker predication with a
/// typed-literal argument.
fn sample_formulas() -> Vec<Formula> {
    vec![
        Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::And(vec![
                fml_rel("p", vec![fml_var("x")]),
                Formula::Not(Box::new(fml_rel("q", vec![fml_var("x")]))),
            ])),
        },
        Formula::Exists {
            vars: vec!["y".into()],
            body: Box::new(Formula::Or(vec![
                fml_rel("r", vec![fml_var("y")]),
                fml_rel("s", vec![fml_var("y")]),
            ])),
        },
        Formula::Implies(
            Box::new(fml_rel("p", vec![fml_var("x")])),
            Box::new(fml_rel("q", vec![fml_var("x")])),
        ),
        Formula::Iff(
            Box::new(fml_rel("p", vec![fml_var("x")])),
            Box::new(fml_rel("q", vec![fml_var("x")])),
        ),
        Formula::atom(
            Term::iri(format!("{LOGIC_NS}rel")).unwrap(),
            vec![
                Term::sequence_marker("xs").unwrap(),
                Term::literal("5", Some("http://www.w3.org/2001/XMLSchema#integer".into()))
                    .unwrap(),
            ],
        )
        .unwrap(),
    ]
}

#[test]
fn formulas_round_trip_through_canonical_rdf12() {
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(sample_formulas());
    let reparsed = reparse_canonical(&program);
    assert_eq!(
        reparsed.formulas.len(),
        program.formulas.len(),
        "every formula must survive the round-trip"
    );
    assert_eq!(
        reparsed.canonical_key(),
        program.canonical_key(),
        "formula program must round-trip canonically through canonical-rdf12"
    );
}

#[test]
fn canonical_rdf12_formula_projection_is_exact() {
    use crate::ir::PreservationKind;
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(sample_formulas());
    let proj = rdf::project_canonical_rdf12(&program).expect("project ok");
    // canonical-rdf12 is Exact: it never touches a loss store, so its residue is empty.
    let loss = LossLedger::new();
    assert!(
        loss.projection_drops_for(&proj.target).is_empty(),
        "the canonical formula projection drops nothing (ExactPreservation)"
    );
    assert_eq!(proj.preservation, PreservationKind::Exact);
}

#[test]
fn down_projections_disclose_formula_residue_and_gate_passes() {
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(sample_formulas());

    // The Horn-fragment targets disclose the full-FOL formula layer as a per-instance
    // actual drop — carried+flagged, never silently dropped (take1 §10.1 legalization).
    let mut loss = LossLedger::new();
    let owl = rdf::project_owl_dl(&program, &mut loss).expect("owl-dl ok");
    let owl_drops = actual_drops(&loss, "owl-dl");
    assert!(
        owl_drops.iter().any(|d| d.contains("logic:Formula")),
        "owl-dl must disclose the full-FOL formula residue when formulas are present: {owl_drops:?}"
    );

    // The whole report must still build (no overclaim): canonical-rdf12 is Exact and
    // carries them; owl-dl is SoundUnder and discloses them. The report reads each row's
    // residue from the same loss store the producers interned into.
    let canon = rdf::project_canonical_rdf12(&program).expect("canon ok");
    report::build_projection_report(&program, &[owl, canon], &loss).expect("report builds");
}

// ── Class-covering formula → OWL union / disjoint-union (H2) ──────────────────

/// A programmatic covering `∀x. whole(x) → (m₁(x) ∨ … ∨ mₙ(x))` over gmeow: IRIs.
fn covering_formula(whole: &str, members: &[&str]) -> Formula {
    let iri = |s: &str| format!("https://blackcatinformatics.ca/gmeow/{s}");
    let membership =
        |c: &str| Formula::atom(Term::Iri(iri(c)), vec![Term::var("x").unwrap()]).unwrap();
    Formula::Forall {
        vars: vec!["x".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(membership(whole)),
            Box::new(Formula::Or(members.iter().map(|m| membership(m)).collect())),
        )),
    }
}

/// A positive, unscoped `owl:disjointWith` axiom between two gmeow: classes.
fn disjoint_axiom(a: &str, b: &str) -> crate::ir::LogicAxiom {
    let iri = |s: &str| format!("https://blackcatinformatics.ca/gmeow/{s}");
    crate::ir::LogicAxiom::new(
        iri(a),
        "http://www.w3.org/2002/07/owl#disjointWith",
        iri(b),
        false,
        false,
        crate::ir::ContextualScope::default(),
    )
    .unwrap()
}

#[test]
fn covering_lowers_to_owl_union_for_overlapping_cover() {
    // No member disjointness → a plain owl:unionOf cover (exhaustiveness only), never a
    // partition — the SocialObject∩InformationObject overlap must survive.
    let prog =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![covering_formula(
            "Entity",
            &[
                "Agent",
                "InformationObject",
                "PhysicalObject",
                "SocialObject",
            ],
        )]);
    let mut loss = LossLedger::new();
    let dl = rdf::project_owl_dl(&prog, &mut loss).unwrap();
    assert!(
        dl.content.contains("unionOf"),
        "emits a union:\n{}",
        dl.content
    );
    assert!(
        !dl.content.contains("disjointUnionOf"),
        "an overlapping cover must NOT claim a partition:\n{}",
        dl.content
    );
    assert!(
        dl.content.contains("subClassOf"),
        "whole ⊑ union:\n{}",
        dl.content
    );
    // A faithfully-emitted covering is NOT a lossy drop.
    let dl_drops = actual_drops(&loss, "owl-dl");
    assert!(
        !dl_drops.iter().any(|d| d.contains("logic:Formula")),
        "a recognized covering is not disclosed as a drop: {dl_drops:?}"
    );
}

#[test]
fn covering_lowers_to_owl_disjoint_union_when_members_pairwise_disjoint() {
    // Covering + all three pairwise disjointness axioms → a partition (owl:disjointUnionOf).
    let prog = LogicProgram::new(
        vec![
            disjoint_axiom("A", "B"),
            disjoint_axiom("A", "C"),
            disjoint_axiom("B", "C"),
        ],
        vec![],
        vec![],
        None,
    )
    .with_formulas(vec![covering_formula("W", &["A", "B", "C"])]);
    let dl = rdf::project_owl_dl(&prog, &mut LossLedger::new()).unwrap();
    assert!(
        dl.content.contains("disjointUnionOf"),
        "a fully-disjoint cover lowers to a partition:\n{}",
        dl.content
    );
}

#[test]
fn functional_carrier_record_projects_owl_functional_property() {
    // The canonical `logic:PropertyCharacteristicAssertion` carrier — with NO direct
    // `?P rdf:type owl:FunctionalProperty` / `logic:functionalProperty` marker — must still make
    // the OWL grounding view emit `owl:FunctionalProperty` on the characterized property, exactly
    // as each carrier record's prose promises now that the deprecated marker is no longer an
    // authored source.
    let program = parse(
        "ex:hasLeadRecord a logic:PropertyCharacteristicAssertion ;\n\
         \x20   logic:characterizes ex:hasLead ;\n\
         \x20   logic:characteristicSort logic:functionalProperty .",
    );
    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    let triples = triple_set(&dl.content);
    let prop = "https://example.org/test/hasLead";
    assert!(
        triples.iter().any(|t| {
            t == &format!(
                "<{prop}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#FunctionalProperty>"
            )
        }),
        "the owl view must project owl:FunctionalProperty on the characterized property from the \
         carrier record:\n{}",
        triples.join("\n")
    );
    assert!(
        triples.iter().any(|t| {
            t == &format!("<{prop}> <{RDF_TYPE}> <http://www.w3.org/2002/07/owl#ObjectProperty>")
        }),
        "the projected functional property is typed as an owl:ObjectProperty:\n{}",
        triples.join("\n")
    );
}

#[test]
fn owl_restriction_round_trips_through_dl_projection() {
    // Parse a logic: restriction into the IR, then project OWL-DL: the owl:Restriction
    // graph must appear (anchored on the deterministic skolem node).
    let prefixes = "\
@prefix ex:    <https://example.org/test/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";
    let ttl = "ex:Bird logic:subClassOf [ a logic:Restriction ;
        logic:onProperty ex:hasBeak ; logic:someValuesFrom ex:Beak ] .";
    let (program, _) = parse_logic_str(&format!("{prefixes}{ttl}"), None).expect("parse ok");
    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    for needle in [
        "http://www.w3.org/2002/07/owl#Restriction",
        "http://www.w3.org/2002/07/owl#onProperty",
        "http://www.w3.org/2002/07/owl#someValuesFrom",
        "https://blackcatinformatics.ca/logic/restriction/",
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
    ] {
        assert!(
            dl.content.contains(needle),
            "missing {needle}:\n{}",
            dl.content
        );
    }
    // someValuesFrom is EL-safe, so the EL projection keeps it too (no drop).
    let mut el_loss = LossLedger::new();
    let el = rdf::project_owl_el(&program, &mut el_loss).unwrap();
    assert!(
        el.content
            .contains("http://www.w3.org/2002/07/owl#someValuesFrom"),
        "someValuesFrom is EL-safe:\n{}",
        el.content
    );
    let el_drops = actual_drops(&el_loss, "owl-el");
    assert!(
        !el_drops.iter().any(|d| d.contains("EL-safe")),
        "an all-EL-safe restriction is not dropped: {el_drops:?}"
    );
}

#[test]
fn non_el_restriction_drops_whole_in_el_projection() {
    // allValuesFrom is not EL-safe: the WHOLE restriction (node + the subClassOf edge
    // into it) must vanish from the EL projection, with a disclosed drop — no dangling
    // reference. DL keeps it.
    let prefixes = "\
@prefix ex:    <https://example.org/test/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";
    let ttl = "ex:VegDish logic:subClassOf [ a logic:Restriction ;
        logic:onProperty ex:hasIngredient ; logic:allValuesFrom ex:Vegetable ] .";
    let (program, _) = parse_logic_str(&format!("{prefixes}{ttl}"), None).expect("parse ok");

    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    assert!(
        dl.content
            .contains("http://www.w3.org/2002/07/owl#allValuesFrom"),
        "DL expresses allValuesFrom:\n{}",
        dl.content
    );

    let mut el_loss = LossLedger::new();
    let el = rdf::project_owl_el(&program, &mut el_loss).unwrap();
    assert!(
        !el.content.contains("allValuesFrom"),
        "EL must not carry the non-EL restriction:\n{}",
        el.content
    );
    assert!(
        !el.content
            .contains("http://www.w3.org/2002/07/owl#Restriction"),
        "the dropped restriction node must not appear in EL:\n{}",
        el.content
    );
    // The subClassOf edge into the dropped restriction must not dangle.
    assert!(
        !el.content.contains("restriction/"),
        "no dangling subClassOf into the dropped skolem node:\n{}",
        el.content
    );
    let el_drops = actual_drops(&el_loss, "owl-el");
    assert!(
        el_drops.iter().any(|d| d.contains("EL-safe")),
        "the EL drop must be disclosed: {el_drops:?}"
    );
}

#[test]
fn oneof_enumeration_round_trips_dl_and_drops_in_el() {
    let prefixes = "\
@prefix ex:    <https://example.org/test/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";
    let ttl = "ex:Season logic:equivalentClass [ a logic:Class ;
        logic:oneOf ( ex:Spring ex:Summer ) ] .";
    let (program, _) = parse_logic_str(&format!("{prefixes}{ttl}"), None).expect("parse ok");

    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    for needle in [
        "http://www.w3.org/2002/07/owl#oneOf",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
        "https://blackcatinformatics.ca/logic/enumeration/",
        "http://www.w3.org/2002/07/owl#equivalentClass",
    ] {
        assert!(
            dl.content.contains(needle),
            "missing {needle}:\n{}",
            dl.content
        );
    }

    // owl:oneOf is a nominal — not EL. The whole enumeration + its anchor drop.
    let mut el_loss = LossLedger::new();
    let el = rdf::project_owl_el(&program, &mut el_loss).unwrap();
    assert!(
        !el.content.contains("oneOf") && !el.content.contains("enumeration/"),
        "EL must not carry the nominal enumeration:\n{}",
        el.content
    );
    let el_drops = actual_drops(&el_loss, "owl-el");
    assert!(
        el_drops.iter().any(|d| d.contains("oneOf")),
        "the EL enumeration drop must be disclosed: {el_drops:?}"
    );
}

#[test]
fn withrestrictions_datarange_round_trips_dl_and_drops_in_el() {
    // A datatype restriction round-trips to DL as rdfs:Datatype + owl:onDatatype +
    // owl:withRestrictions( facet cells ); datatype facets are not OWL 2 EL, so the whole
    // datarange (node + its anchor) drops from EL with a disclosed loss.
    let prefixes = "\
@prefix ex:    <https://example.org/test/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";
    let ttl = "ex:PositiveScore logic:equivalentClass [ a rdfs:Datatype ;
        logic:onDatatype xsd:decimal ;
        logic:withRestrictions ( [ xsd:minInclusive \"0.0\"^^xsd:decimal ]
                                 [ xsd:maxInclusive \"1.0\"^^xsd:decimal ] ) ] .";
    let (program, _) = parse_logic_str(&format!("{prefixes}{ttl}"), None).expect("parse ok");

    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    for needle in [
        "http://www.w3.org/2000/01/rdf-schema#Datatype",
        "http://www.w3.org/2002/07/owl#onDatatype",
        "http://www.w3.org/2002/07/owl#withRestrictions",
        "http://www.w3.org/2001/XMLSchema#minInclusive",
        "http://www.w3.org/2001/XMLSchema#maxInclusive",
        "https://blackcatinformatics.ca/logic/datarange/",
        "http://www.w3.org/2002/07/owl#equivalentClass",
    ] {
        assert!(
            dl.content.contains(needle),
            "missing {needle}:\n{}",
            dl.content
        );
    }

    // Datatype facets are not EL: the whole datarange + its anchor drop.
    let mut el_loss = LossLedger::new();
    let el = rdf::project_owl_el(&program, &mut el_loss).unwrap();
    assert!(
        !el.content.contains("withRestrictions") && !el.content.contains("datarange/"),
        "EL must not carry the datarange:\n{}",
        el.content
    );
    assert!(
        !el.content
            .contains("http://www.w3.org/2001/XMLSchema#minInclusive"),
        "no orphan facet triple may remain in EL:\n{}",
        el.content
    );
    let el_drops = actual_drops(&el_loss, "owl-el");
    assert!(
        el_drops.iter().any(|d| d.contains("datatype facets")),
        "the EL datarange drop must be disclosed: {el_drops:?}"
    );
}

#[test]
fn cardinality_restriction_projects_typed_integer_in_dl() {
    let prefixes = "\
@prefix ex:    <https://example.org/test/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";
    let ttl = "ex:Parent logic:subClassOf [ a logic:Restriction ;
        logic:onProperty ex:hasChild ; logic:minCardinality 1 ] .";
    let (program, _) = parse_logic_str(&format!("{prefixes}{ttl}"), None).expect("parse ok");
    let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
    assert!(
        dl.content
            .contains("http://www.w3.org/2002/07/owl#minCardinality"),
        "DL emits minCardinality:\n{}",
        dl.content
    );
    assert!(
        dl.content
            .contains("http://www.w3.org/2001/XMLSchema#nonNegativeInteger"),
        "the count is a typed xsd:nonNegativeInteger:\n{}",
        dl.content
    );
}

#[test]
fn covering_owl_dl_is_deterministic() {
    let prog =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![covering_formula(
            "Entity",
            &["Agent", "SocialObject", "PhysicalObject"],
        )]);
    let a = rdf::project_owl_dl(&prog, &mut LossLedger::new())
        .unwrap()
        .content;
    let b = rdf::project_owl_dl(&prog, &mut LossLedger::new())
        .unwrap()
        .content;
    assert_eq!(a, b, "covering projection is byte-stable across runs");
}

#[test]
fn covering_dropped_in_el_and_gufo_with_shape_tag() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None)
        .with_formulas(vec![covering_formula("Entity", &["Agent", "SocialObject"])]);
    let mut loss = LossLedger::new();
    rdf::project_owl_el(&prog, &mut loss).unwrap();
    rdf::project_gufo(&prog, &mut loss).unwrap();
    for target in ["owl-el", "gufo"] {
        let drops = actual_drops(&loss, target);
        assert!(
            drops.iter().any(|d| d.contains("Disjunctive")),
            "EL/gUFO disclose the covering's disjunction as residue: {drops:?}"
        );
    }
}

#[test]
fn covering_roundtrips_from_authored_turtle() {
    // Author the covering as a reified logic:Formula tree (as the slices will), parse it,
    // and confirm it lowers to owl:unionOf — de-risking the hand-authored formula shape.
    let ttl = r#"
ex:cover a logic:Formula ;
    logic:forall ex:coverBody ;
    logic:quantifiedVariable ex:coverVar .
ex:coverVar logic:termIndex 0 ; logic:termVariable "x" .
ex:coverBody a logic:Formula ;
    logic:antecedent ex:ante ;
    logic:consequent ex:cons .
ex:ante a logic:Formula ; logic:relation ex:Whole ; logic:argument ex:anteArg .
ex:anteArg logic:termIndex 0 ; logic:termVariable "x" .
ex:cons a logic:Formula ; logic:or ex:d0 , ex:d1 .
ex:d0 a logic:Formula ; logic:relation ex:A ; logic:argument ex:d0a .
ex:d0a logic:termIndex 0 ; logic:termVariable "x" .
ex:d1 a logic:Formula ; logic:relation ex:B ; logic:argument ex:d1a .
ex:d1a logic:termIndex 0 ; logic:termVariable "x" .
"#;
    let prog = parse(ttl);
    assert_eq!(
        prog.formulas.len(),
        1,
        "one top-level covering formula parsed"
    );
    let dl = rdf::project_owl_dl(&prog, &mut LossLedger::new()).unwrap();
    assert!(
        dl.content.contains("unionOf"),
        "authored covering lowers to a union:\n{}",
        dl.content
    );
}

/// Every authored `slices/<tier>/<slice>/module.ttl`, sorted — the whole shipped
/// corpus the functional-carrier guard below sweeps.
///
/// The carriers are NOT all in one file: a `logic:PropertyCharacteristicAssertion`
/// about a term a non-grounding slice owns is authored in THAT slice's `module.ttl`
/// (`docs/GROUNDING.md`'s tier rule — a grounding slice never depends on a
/// non-grounding one), so `slices/grounding/logic/module.ttl` carries only the
/// characteristics of `logic:`'s own and its grounding peers' terms. Reading a
/// single file would therefore measure a fraction of the corpus and let a dropped
/// re-emission in every other slice pass unseen.
fn slice_module_ttls() -> Vec<PathBuf> {
    let slices = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../slices");
    let mut paths = Vec::new();
    for tier in std::fs::read_dir(&slices).expect("read slices/") {
        let tier = tier.expect("tier entry").path();
        if !tier.is_dir() {
            continue;
        }
        for slice in std::fs::read_dir(&tier).expect("read slices/<tier>/") {
            let module = slice.expect("slice entry").path().join("module.ttl");
            if module.is_file() {
                paths.push(module);
            }
        }
    }
    paths.sort();
    assert!(!paths.is_empty(), "expected at least one slice module.ttl");
    paths
}

/// Whole-set count-and-set parity guard for the functional-carrier → OWL-DL
/// re-projection over the REAL authored corpus (every `slices/*/*/module.ttl`).
///
/// Since the deprecation removed the authored `?P rdf:type owl:FunctionalProperty`
/// marker, functionality survives ONLY because `project_owl_dl` re-emits it from
/// each `logic:PropertyCharacteristicAssertion` carrier record (the
/// `functional_carrier_properties` join: `logic:characterizes ?P` +
/// `logic:characteristicSort logic:functionalProperty`). The existing synthetic
/// test proves this for a 3-triple program; this test proves it for the WHOLE
/// shipped corpus so a future refactor that silently drops the re-emission — the
/// exact failure the issue-1579 audit wrongly believed already existed — hard-fails
/// here instead of shipping an OWL-DL view with zero `owl:FunctionalProperty`.
///
/// Each `module.ttl` is parsed and projected INDEPENDENTLY (one program per file,
/// the same production `parse_logic_str` path), so no cross-file blank-node label
/// or prefix binding can collide; the per-file set-parity assertions are then
/// summed into a corpus-wide non-vacuity guard.
///
/// The guard is a SET equality (not just a count): every functional carrier
/// property must appear with an `owl:FunctionalProperty` type triple in the
/// projected view, and nothing else may. A wrong-property substitution (right
/// count, wrong IRIs) therefore also fails. The expected count is DERIVED from the
/// parsed corpus (719 carriers today), never hardcoded, so it tracks the corpus.
#[test]
fn functional_carriers_project_owl_functional_property_over_whole_corpus() {
    // Fully-qualified logic: IRIs (the `logic()` helper in rdf.rs is private; reproduce
    // its tiny join here rather than touch production code).
    let characterizes = format!("{LOGIC_NS}characterizes");
    let characteristic_sort = format!("{LOGIC_NS}characteristicSort");
    let functional_sort = format!("{LOGIC_NS}functionalProperty");
    let functional_type_obj = "http://www.w3.org/2002/07/owl#FunctionalProperty";

    // Corpus-wide totals, accumulated across the per-file sweeps below.
    let mut corpus_carrier_triples = 0usize;
    let mut corpus_carrier_props: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();

    for module_ttl in slice_module_ttls() {
        let text = std::fs::read_to_string(&module_ttl)
            .unwrap_or_else(|e| panic!("read {}: {e}", module_ttl.display()));
        let display = module_ttl.display().to_string();

        // Same production parse path the `parse` helper uses (frontend `parse_logic_str`).
        let (program, _diags) = parse_logic_str(&text, Some(format!("urn:gmeow:{display}")))
            .unwrap_or_else(|e| panic!("parse real {display}: {e:?}"));

        // Reproduce `functional_carrier_properties`: join `characterizes ?P` with the
        // `characteristicSort logic:functionalProperty` sort on the carrier record IRI.
        let mut rec_prop: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        let mut functional_recs: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        // Independent count of the carrier triples (`characteristicSort
        // logic:functionalProperty`), NOT going through the join — a second, orthogonal
        // witness of the corpus size.
        let mut source_carrier_triples = 0usize;
        for ax in &program.axioms {
            if ax.predicate == characterizes && !ax.obj_is_literal {
                rec_prop.insert(ax.subject.clone(), ax.obj.clone());
            } else if ax.predicate == characteristic_sort
                && !ax.obj_is_literal
                && ax.obj == functional_sort
            {
                functional_recs.insert(ax.subject.clone());
                source_carrier_triples += 1;
            }
        }
        let carrier_props: std::collections::BTreeSet<String> = functional_recs
            .iter()
            .filter_map(|rec| rec_prop.get(rec).cloned())
            .collect();

        // Every functional carrier record names a DISTINCT property, so the carrier-triple
        // count equals the distinct-property count. This ties the projected
        // `owl:FunctionalProperty` triple count to the FULL carrier count.
        assert_eq!(
            carrier_props.len(),
            source_carrier_triples,
            "each functional carrier record in {display} must characterize a distinct property"
        );

        // Project OWL-DL from the real program and collect the subjects typed
        // `owl:FunctionalProperty` in the resulting view.
        let dl = rdf::project_owl_dl(&program, &mut LossLedger::new()).unwrap();
        let projected_props: std::collections::BTreeSet<String> = triple_set(&dl.content)
            .iter()
            .filter_map(|t| {
                // Each canonical line reads `<s> <p> <o>` (trailing ` .` already stripped).
                let s = t.strip_prefix('<')?;
                let (subject, rest) = s.split_once("> <")?;
                let (predicate, object) = rest.split_once("> <")?;
                let object = object.strip_suffix('>')?;
                (predicate == RDF_TYPE && object == functional_type_obj).then(|| subject.to_owned())
            })
            .collect();

        // STRICT parity: the SET of re-emitted functional properties equals the SET of
        // functional-carrier properties. Catches a dropped re-emission (projected set
        // shrinks) AND a wrong-property substitution (same count, different IRIs).
        assert_eq!(
            projected_props, carrier_props,
            "the OWL-DL view of {display} must re-emit owl:FunctionalProperty for EXACTLY \
             the functional carrier properties — no drop, no substitution"
        );

        corpus_carrier_triples += source_carrier_triples;
        for prop in carrier_props {
            assert!(
                corpus_carrier_props.insert(prop.clone()),
                "{prop} is named functional by a carrier in more than one slice ({display})"
            );
        }
    }

    // The corpus must actually exercise the re-projection (guard against an empty
    // parse silently passing a vacuous set-equality).
    assert!(
        corpus_carrier_triples >= 700,
        "expected the shipped corpus to carry ~719 functional carrier records; \
         parsed only {corpus_carrier_triples} — the corpus likely failed to load"
    );
    assert_eq!(
        corpus_carrier_props.len(),
        corpus_carrier_triples,
        "each functional carrier record across the whole corpus must characterize a \
         distinct property"
    );
}
