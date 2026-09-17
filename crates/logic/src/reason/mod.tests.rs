// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::modal;
use crate::result::{CompletenessStatus, EvaluationStatus, InformationState};
use crate::store::WorldStore;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

/// Synthetic baseline for the ordinary fixed rules, excluding joint producers.
struct OrdinaryClosure {
    inferred: Vec<InferredAxiom>,
    consumed_steps: u64,
}

fn ordinary_rule_closure(
    edb: &RdfDataset,
    rules: Vec<crate::rule_ir::EvalRule>,
) -> gmeow_errors::Result<OrdinaryClosure> {
    let facts = build_edb_facts(edb)?;
    let (chase, frontier, status) =
        crate::oracle::native_forward_eval_rules_with_frontier(&facts, rules, None)?;
    assert_eq!(status, BudgetStatus::Ok);
    Ok(OrdinaryClosure {
        inferred: chase_rows_to_inferred(&chase)?,
        consumed_steps: frontier.consumed_steps,
    })
}

const W: &str = "http://gmeow.example/w";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const A: &str = "http://gmeow.example/A";
const B: &str = "http://gmeow.example/B";
const C: &str = "http://gmeow.example/C";
const X: &str = "http://gmeow.example/x";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

fn quad_in(world: &str, s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(world))
}

fn dataset_without_axiom(
    edb: &RdfDataset,
    axiom: &LeaveOneOutAxiom,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in edb.owned_quads() {
        if quad.predicate == axiom.predicate
            && matches!(&quad.subject, RdfTerm::Iri(subject) if subject == &axiom.subject)
            && matches!(&quad.object, RdfTerm::Iri(object) if object == &axiom.object)
        {
            continue;
        }
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .map_err(|error| reason_err(format!("freeze leave-one-out RDF dataset: {error}")))
}

fn scratch_leave_one_out(edb: &RdfDataset, axiom: &LeaveOneOutAxiom) -> bool {
    let reduced = dataset_without_axiom(edb, axiom).expect("reduced dataset freezes");
    reason_closure_axioms(
        crate::reason::prepare_reasoning_input(&reduced).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("scratch leave-one-out reasons")
    .iter()
    .any(|inferred| {
        inferred.subject == axiom.subject
            && inferred.predicate == axiom.predicate
            && inferred.object.as_iri() == Some(axiom.object.as_str())
    })
}

fn fact_surfaces(facts: &TypedFactSet) -> Vec<(String, Vec<String>)> {
    let mut rows = facts
        .facts()
        .map(|fact| {
            (
                fact.predicate.clone(),
                fact.args
                    .iter()
                    .map(|&id| facts.interner().display_of(id).to_owned())
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[test]
fn direct_edb_fold_is_fact_identical_to_the_world_store_adapter() {
    let p = "http://gmeow.example/p";
    let w2 = "urn:gmeow:test:world-2";
    let mut builder = RdfDatasetBuilder::new();
    for quad in [
        quad(A, p, B),
        RdfQuad::new(RdfTerm::blank_node("subject"), p, RdfTerm::iri(C)).in_graph(RdfTerm::iri(w2)),
        // Literal objects are outside the fixed EL/DL relation fragment.
        RdfQuad::new(
            RdfTerm::iri(A),
            p,
            RdfTerm::literal(RdfLiteral::simple("annotation")),
        )
        .in_graph(RdfTerm::iri(W)),
        // Default and blank-node graph names are not named-IRI worlds.
        RdfQuad::new(RdfTerm::iri(A), p, RdfTerm::iri(C)),
        RdfQuad::new(RdfTerm::iri(B), p, RdfTerm::iri(C)).in_graph(RdfTerm::blank_node("graph")),
    ] {
        builder.push_owned_quad(&quad);
    }
    let dataset = builder.freeze().expect("mixed-world fixture freezes");

    let direct = build_edb_facts(dataset.as_ref()).expect("direct frozen-IR fold");

    // The retired production shape is retained here as a semantic oracle: copy
    // through WorldStore, enumerate its named worlds, and build the same typed
    // facts. The optimized one-pass adapter must change cost, never membership.
    let store = WorldStore::new();
    store
        .load_dataset(dataset.as_ref())
        .expect("world-store oracle load");
    let mut via_store = TypedFactSet::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
            if !quad.o.is_iri() {
                continue;
            }
            let Some(predicate) = quad.p.as_iri() else {
                continue;
            };
            via_store.push_quad(&quad.s, predicate, &quad.o, &world);
        }
    }

    assert_eq!(
        fact_surfaces(&direct),
        fact_surfaces(&via_store),
        "the direct frozen-IR fold preserves the exact named-world fact set"
    );
}

#[test]
fn incremental_leave_one_out_matches_scratch_across_worlds_and_alternative_proofs() {
    const W2: &str = "urn:gmeow:test:leave-one-out-world-2";
    const D: &str = "http://gmeow.example/D";
    const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const P: &str = "http://gmeow.example/p";

    let store = dataset(vec![
        // World one gives A -> C two proofs: the direct assertion and A -> B -> C.
        quad(A, SUBCLASS, B),
        quad(B, SUBCLASS, C),
        quad(A, SUBCLASS, C),
        // The same A -> B assertion exists in another world but has an independent
        // alternate proof there. Leave-one-out removes BOTH asserted occurrences.
        quad_in(W2, A, SUBCLASS, B),
        quad_in(W2, A, SUBCLASS, D),
        quad_in(W2, D, SUBCLASS, B),
        // A predicate outside the fixed rule heads stays load-bearing.
        quad(P, DOMAIN, A),
    ]);
    let probes = vec![
        LeaveOneOutAxiom::new(A, SUBCLASS, C),
        LeaveOneOutAxiom::new(A, SUBCLASS, B),
        LeaveOneOutAxiom::new(B, SUBCLASS, C),
        LeaveOneOutAxiom::new(P, DOMAIN, A),
    ];

    let incremental = leave_one_out_rederived(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        &probes,
    )
    .expect("incremental leave-one-out reasons");
    let scratch = probes
        .iter()
        .map(|probe| scratch_leave_one_out(&store, probe))
        .collect::<Vec<_>>();
    assert_eq!(incremental, scratch);
    assert_eq!(incremental, vec![true, true, false, false]);
}

/// A leave-one-out probe spelled in the CANONICAL `logic:` subsumption vocabulary
/// is answered by the same analytic reachability index as its `rdfs:` projection,
/// and the two spellings compose into ONE taxonomy.
/// Every canonical `logic:` term the class-expression surface authors reaches its
/// fixed-calculus spelling through the ONE shared table, in both modes.
///
/// The local names are the complete `logic:` class-expression vocabulary — the same
/// set the compiler's restriction lifter enumerates. A term missing here is a slot a
/// slice can author and the reasoner cannot see, which is the exact defect the table
/// exists to foreclose, so the list is spelled out rather than derived from the table
/// under test.
#[test]
fn every_canonical_class_expression_term_lowers_onto_the_fixed_calculus_spelling() {
    const OWL: &str = "http://www.w3.org/2002/07/owl#";
    for local in [
        "Restriction",
        "onProperty",
        "someValuesFrom",
        "allValuesFrom",
        "hasValue",
        "onClass",
        "onDataRange",
        "onDatatype",
        "withRestrictions",
        "cardinality",
        "minCardinality",
        "maxCardinality",
        "qualifiedCardinality",
        "minQualifiedCardinality",
        "maxQualifiedCardinality",
        // The anchors that attach a body to the class it constrains. Without them
        // the body is read and never applied.
        "equivalentClass",
    ] {
        let canonical = format!("{}{local}", gmeow_ns::LOGIC_NS);
        let projected = format!("{OWL}{local}");
        assert_eq!(
            calculus_term(&canonical),
            projected,
            "the raw-scan waists must normalize logic:{local}"
        );
        assert_eq!(
            edb_predicate_spellings(&canonical).collect::<Vec<_>>(),
            vec![canonical.as_str(), projected.as_str()],
            "the typed EDB must carry logic:{local} canonical-first, projected-second"
        );
    }
    for (canonical, projected) in [
        (gmeow_ns::LOGIC_SUB_CLASS_OF, gmeow_ns::RDFS_SUB_CLASS_OF),
        (
            gmeow_ns::LOGIC_SUB_PROPERTY_OF,
            gmeow_ns::RDFS_SUB_PROPERTY_OF,
        ),
    ] {
        assert_eq!(calculus_term(canonical), projected);
        assert_eq!(
            edb_predicate_spellings(canonical).collect::<Vec<_>>(),
            vec![canonical, projected]
        );
    }
    // Everything else passes through untouched — the lowering ADDS a view of the
    // canonical vocabulary, it does not rewrite the world.
    for untouched in [
        "http://www.w3.org/2002/07/owl#onProperty",
        "https://blackcatinformatics.ca/math/normalizationStrength",
        "https://blackcatinformatics.ca/logic/keyProperty",
    ] {
        assert_eq!(calculus_term(untouched), untouched);
        assert_eq!(
            edb_predicate_spellings(untouched).collect::<Vec<_>>(),
            vec![untouched]
        );
    }
}

/// Independent expected answers cover canonical and projected operator spellings.
#[test]
fn leave_one_out_preserves_canonical_logic_subsumption() {
    const D: &str = "http://gmeow.example/D";
    const E: &str = "http://gmeow.example/E";
    const F: &str = "http://gmeow.example/F";
    const P: &str = "http://gmeow.example/p";
    const Q: &str = "http://gmeow.example/q";
    const R: &str = "http://gmeow.example/r";
    let logic_subclass = gmeow_ns::LOGIC_SUB_CLASS_OF;
    let logic_subproperty = gmeow_ns::LOGIC_SUB_PROPERTY_OF;

    let store = dataset(vec![
        // A purely canonical chain: A ⊑ B ⊑ C makes the direct A ⊑ C redundant.
        quad(A, logic_subclass, B),
        quad(B, logic_subclass, C),
        quad(A, logic_subclass, C),
        // A MIXED chain: the canonical edge and its rdfs: projection are the same
        // edge of the taxonomy, so D ⊑ E (rdfs) ⊑ F (logic) re-derives D ⊑ F.
        quad(D, SUBCLASS, E),
        quad(E, logic_subclass, F),
        quad(D, logic_subclass, F),
        // The property side takes the same lowering.
        quad(P, logic_subproperty, Q),
        quad(Q, logic_subproperty, R),
        quad(P, logic_subproperty, R),
    ]);
    let probes = vec![
        LeaveOneOutAxiom::new(A, logic_subclass, C),
        LeaveOneOutAxiom::new(A, logic_subclass, B),
        LeaveOneOutAxiom::new(D, logic_subclass, F),
        LeaveOneOutAxiom::new(D, SUBCLASS, E),
        LeaveOneOutAxiom::new(P, logic_subproperty, R),
        LeaveOneOutAxiom::new(P, logic_subproperty, Q),
    ];

    let rederived = leave_one_out_rederived(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        &probes,
    )
    .expect("batch reasons");
    assert_eq!(
        rederived,
        vec![true, false, true, false, true, false],
        "canonical logic: subsumption must preserve the native reachability answer, \
             composing with its rdfs: projection as one taxonomy"
    );
}

#[test]
fn incremental_leave_one_out_preserves_finite_dl_union_derivation() {
    const U: &str = "http://gmeow.example/U";
    const UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

    let list = RdfTerm::blank_node("union-list");
    let store = dataset(vec![
        quad(A, SUBCLASS, U),
        RdfQuad::new(RdfTerm::iri(U), UNION_OF, list.clone()).in_graph(RdfTerm::iri(W)),
        RdfQuad::new(list.clone(), RDF_FIRST, RdfTerm::iri(A)).in_graph(RdfTerm::iri(W)),
        RdfQuad::new(list, RDF_REST, RdfTerm::iri(RDF_NIL)).in_graph(RdfTerm::iri(W)),
    ]);
    let probe = LeaveOneOutAxiom::new(A, SUBCLASS, U);

    let incremental = leave_one_out_rederived(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        std::slice::from_ref(&probe),
    )
    .expect("incremental union leave-one-out reasons");
    assert_eq!(incremental, vec![scratch_leave_one_out(&store, &probe)]);
    assert_eq!(incremental, vec![true]);
}

#[test]
fn batched_leave_one_out_matches_scratch_for_every_fast_tbox_family() {
    const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
    const INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
    const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
    const FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
    const P: &str = "http://gmeow.example/p";
    const Q: &str = "http://gmeow.example/q";
    const R: &str = "http://gmeow.example/r";
    const S: &str = "http://gmeow.example/s";
    const T: &str = "http://gmeow.example/t";
    const D: &str = "http://gmeow.example/D";
    const E: &str = "http://gmeow.example/E";
    const MARKER: &str = "http://gmeow.example/FunctionalMarker";
    const LOGIC_DISJOINT: &str = "https://blackcatinformatics.ca/logic/disjointWith";
    const LOGIC_INVERSE: &str = "https://blackcatinformatics.ca/logic/inverseOf";
    const LOGIC_FUNCTIONAL: &str = "https://blackcatinformatics.ca/logic/functionalProperty";

    let store = dataset(vec![
        quad(P, SUBPROPERTY, R),
        quad(P, SUBPROPERTY, Q),
        quad(Q, SUBPROPERTY, R),
        quad(A, EQUIVALENT, B),
        quad(P, DOMAIN, A),
        quad(P, RANGE, B),
        quad(P, INVERSE, Q),
        quad(A, DISJOINT, C),
        quad(A, COMPLEMENT, C),
        quad(B, DISJOINT, C),
        // Canonical authoring must take the same batch negative filter as its
        // OWL projection. Missing this lowering made each such probe rebuild
        // and rerun the complete finite-DL dataset.
        quad(D, LOGIC_DISJOINT, E),
        quad(S, LOGIC_INVERSE, T),
        quad(P, TYPE, FUNCTIONAL),
        quad(S, TYPE, LOGIC_FUNCTIONAL),
        quad(Q, TYPE, MARKER),
        quad(MARKER, SUBCLASS, FUNCTIONAL),
        quad(Q, TYPE, FUNCTIONAL),
    ]);
    let probes = vec![
        LeaveOneOutAxiom::new(P, SUBPROPERTY, R),
        LeaveOneOutAxiom::new(Q, SUBPROPERTY, R),
        LeaveOneOutAxiom::new(A, EQUIVALENT, B),
        LeaveOneOutAxiom::new(P, DOMAIN, A),
        LeaveOneOutAxiom::new(P, RANGE, B),
        LeaveOneOutAxiom::new(P, INVERSE, Q),
        LeaveOneOutAxiom::new(A, DISJOINT, C),
        LeaveOneOutAxiom::new(B, DISJOINT, C),
        LeaveOneOutAxiom::new(D, LOGIC_DISJOINT, E),
        LeaveOneOutAxiom::new(S, LOGIC_INVERSE, T),
        LeaveOneOutAxiom::new(P, TYPE, FUNCTIONAL),
        LeaveOneOutAxiom::new(S, TYPE, LOGIC_FUNCTIONAL),
        LeaveOneOutAxiom::new(Q, TYPE, FUNCTIONAL),
    ];

    let batched = leave_one_out_rederived(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        &probes,
    )
    .expect("batch reasons");
    let scratch = probes
        .iter()
        .map(|probe| scratch_leave_one_out(&store, probe))
        .collect::<Vec<_>>();
    assert_eq!(batched, scratch);
    assert_eq!(
        batched,
        vec![
            true, false, false, false, false, false, true, false, false, false, false, false, true
        ]
    );
}

#[test]
fn native_contract_hash_frames_every_load_bearing_engine_component() {
    let names = NATIVE_CONTRACT_COMPONENTS
        .iter()
        .map(|(name, source)| {
            assert!(!source.is_empty(), "contract component {name} is empty");
            *name
        })
        .collect::<Vec<_>>();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let actual =
        gmeow_build_inputs::NativeSources::collect(&root).expect("complete native source owner");
    let expected: Vec<_> = actual.files.keys().map(String::as_str).collect();
    assert_eq!(
        names, expected,
        "every real production module is part of the contract"
    );
    let previous_components = vec![
        "reason/el.rs",
        "reason/rl.rs",
        "correspondence_exec/axes.rs",
        "correspondence_exec/presentation/source.rs",
        "correspondence_exec/presentation/source/report.rs",
        "logic-compile/frontend/presentation.rs",
        "logic-compile/frontend/presentation/reader.rs",
        "logic-compile/ir/presentation.rs",
        "logic-compile/term_serde.rs",
        "logic-compile/frontend.rs",
        "logic-compile/frontend/emission.rs",
        "logic-compile/frontend/prepared.rs",
        "logic-compile/frontend/source_graph.rs",
        "logic-compile/frontend/formula_reader.rs",
        "logic-compile/graphutil.rs",
        "reason/dataset.rs",
        "reason/value.rs",
        "reason/dl.rs",
        "reason/refute.rs",
        "reason/refute/proof.rs",
        "reason/refute/casesplit/proof.rs",
        "reason/refute/datatype.rs",
        "reason/refute/counting.rs",
        "reason/refute/casesplit.rs",
        "reason/mod.rs",
        "reason/program.rs",
        "reason/schema.rs",
        "reason/source_existentials.rs",
        "result.rs",
        "result/native.rs",
        "correspondence_exec.rs",
        "correspondence_exec/atomic_lens.rs",
        "correspondence_exec/atomic_lens/composition.rs",
        "correspondence_exec/presentation.rs",
        "correspondence_exec/presentation/syntax.rs",
        "correspondence_exec/presentation/pushout.rs",
        "reason/artifacts.rs",
        "modal.rs",
        "modal/evidence.rs",
        "modal/native.rs",
        "modal/composite.rs",
        "modal/composite/monitor_cache.rs",
        "modal/contextual/monitor.rs",
        "modal/contextual/path.rs",
        "modal/composite/temporal_lower.rs",
        "modal/composite/temporal_eval.rs",
        "modal/journal.rs",
        "runtime.rs",
        "runtime/session/journal.rs",
        "runtime/session/outcome.rs",
        "result_rdf.rs",
        "result_rdf/native.rs",
        "explain.rs",
        "error.rs",
        "gmeow-logic-compile/src/frontend/admission.rs",
        "gmeow-logic-compile/src/frontend/temporal.rs",
        "gmeow-logic-compile/src/error.rs",
        "gmeow-logic-compile/src/ir.rs",
        "gmeow-logic-compile/src/ir/constraint.rs",
        "gmeow-logic-compile/src/ir/validation.rs",
        "modal/contextual.rs",
        "modal/contextual/temporal.rs",
        "oracle.rs",
        "certify.rs",
        "lower.rs",
        "native_semantics.rs",
        "ns/lib.rs",
        "program_analysis.rs",
        "materialize.rs",
        "operator_rules.rs",
        "reason/enactment/refine.rs",
        "runtime/session.rs",
        "runtime/session/checkpoint.rs",
        "runtime/session/delta.rs",
        "runtime/session/facade.rs",
        "runtime/session/identity.rs",
        "relational_core.rs",
        "logic-compile/ir/axis_evidence.rs",
        "logic-compile/ir/composition.rs",
        "logic-compile/ir/numeric_literal.rs",
        "logic-compile/ir/literal_serde.rs",
        "logic-compile/relational_core.rs",
        "logic-compile/relational_core/formula_analysis.rs",
        "logic-compile/relational_core/numeric.rs",
        "logic-compile/relational_core/head.rs",
        "logic-compile/relational_core/identity.rs",
        "logic-compile/relational_core/projection.rs",
        "logic-compile/relational_core/reader.rs",
        "logic-compile/relational_core/term.rs",
        "stablemodel.rs",
        "wellfounded.rs",
        "physical/numeric.rs",
        "rule_ir.rs",
        "store.rs",
        "rule_ir/reduction.rs",
        "physical/seminaive/reduce.rs",
        "term_serde.rs",
        "physical/plan.rs",
        "physical/dependency.rs",
        "physical/effects.rs",
        "physical/effects/value_flow.rs",
        "physical/seminaive/joint/input.rs",
        "physical/seminaive/joint/input/admission.rs",
        "physical/seminaive.rs",
        "physical/chase.rs",
        "physical/chase/join.rs",
        "physical/seminaive/joint.rs",
        "physical/seminaive/property.rs",
        "physical/seminaive/property/list.rs",
        "physical/seminaive/property/cardinality.rs",
        "physical/seminaive/property/minimum.rs",
        "physical/seminaive/property/datatype.rs",
        "physical/seminaive/property/datatype/extent.rs",
        "physical/seminaive/property/datatype_constraint.rs",
        "physical/store.rs",
        "physical/store/witness.rs",
        "physical/store/semantic.rs",
        "physical/builtin_eval.rs",
        "physical/builtin_eval/input.rs",
        "reasoner_services.rs",
        "entail.rs",
        "entail/admission.rs",
        "reason/refute/native.rs",
        "reason/refute/diagnostic.rs",
        "reason/refute/casesplit/admission.rs",
        "reason/refute/casesplit/execution.rs",
        "physical/domain.rs",
        "physical/seminaive/joint/families.rs",
    ];
    for previous in previous_components {
        let path = if let Some(path) = previous.strip_prefix("logic-compile/") {
            format!("crates/logic-compile/src/{path}")
        } else if let Some(path) = previous.strip_prefix("gmeow-logic-compile/src/") {
            format!("crates/logic-compile/src/{path}")
        } else if previous == "ns/lib.rs" {
            "crates/ns/src/lib.rs".to_owned()
        } else {
            format!("crates/logic/src/{previous}")
        };
        assert!(
            actual.files.contains_key(&path),
            "previously pinned semantic component {path} must remain owned"
        );
    }
    assert_eq!(
        actual.digest().unwrap(),
        crate::runtime::NATIVE_SOURCE_CONTRACT
    );
    assert_eq!(native_contract_hash().len(), 40, "SHA-1 hex contract id");
}

/// Public admission and service behavior belongs to the exact engine identity,
/// including context refusal before any native consistency reduction.
#[test]
fn native_contract_hash_folds_public_admission_and_services() {
    let framed = framed_native_component_source();
    for source in [
        "crates/logic/src/reasoner_services.rs",
        "crates/logic/src/entail.rs",
        "crates/logic/src/entail/admission.rs",
        "crates/logic/src/reason/rl.rs",
    ] {
        let component = NATIVE_CONTRACT_COMPONENTS
            .iter()
            .find(|(name, _)| *name == source)
            .expect("public DL admission and services must participate in the engine identity");
        assert!(!component.1.is_empty(), "{source} must be nonempty");
        assert!(
            framed.contains(component.1),
            "{source} must reach the framed contract"
        );
        assert!(
            framed.contains(&format!("{}:{source}:{}:", source.len(), component.1.len())),
            "{source} must be framed by its name and byte length",
        );
    }
}

#[test]
fn native_contract_hash_folds_the_purrdf_substrate_identity() {
    // The moved lanes (RL chase, DL entail services, datatype value space) delegate to
    // purrdf, so their behaviour can change on a purrdf pin bump WITHOUT moving any
    // native source byte. This test proves the purrdf-provided identity genuinely
    // participates in `native_contract_hash`, so such a bump is detected.

    // 1. The purrdf identity is deterministic and carries every claimed sub-identity:
    //    the datalog CALCULUS_VERSION and the two 64-hex calculus contract hashes.
    let id = purrdf_substrate_identity();
    assert_eq!(id, purrdf_substrate_identity(), "purrdf identity is stable");
    assert!(
        id.contains(purrdf::datalog::cache::CALCULUS_VERSION),
        "the datalog calculus version must reach the folded identity: {id}"
    );
    let owl_rl = purrdf::datalog::cache::contract_hash(&purrdf::entail::calculus_program(
        purrdf::entail::Regime::OwlRl,
    ));
    let datatype = purrdf::datalog::cache::contract_hash(&purrdf::entail::calculus_program(
        purrdf::entail::Regime::D,
    ));
    assert!(
        id.contains(&owl_rl.to_hex()),
        "the OWL 2 RL calculus contract hash must reach the folded identity"
    );
    assert!(
        id.contains(&datatype.to_hex()),
        "the datatype calculus contract hash must reach the folded identity"
    );
    assert_ne!(
        owl_rl.to_hex(),
        datatype.to_hex(),
        "distinct calculi must carry distinct purrdf contract hashes"
    );

    // 2. Participation proof: recompute the digest over the native component source
    //    ALONE (no purrdf fold) and confirm it differs from `native_contract_hash`.
    //    A regression that dropped the purrdf fold would make these two equal.
    let native_only = crate::provenance::sha1_hex(&framed_native_component_source());
    assert_ne!(
        native_only,
        native_contract_hash(),
        "the purrdf substrate identity must move the native contract hash — if these \
             are equal the purrdf fold was dropped and a purrdf pin bump would leave a \
             stale engine seal"
    );

    // 3. And that the difference is EXACTLY the framed purrdf segment: folding the
    //    same-framed purrdf identity onto the native base reproduces the real hash, so
    //    the purrdf value is the sole additional input (no accidental extra framing).
    let mut folded = framed_native_component_source();
    use std::fmt::Write as _;
    const PURRDF_TAG: &str = "purrdf-substrate";
    write!(
        &mut folded,
        "{}:{PURRDF_TAG}:{}:",
        PURRDF_TAG.len(),
        id.len()
    )
    .expect("String writes cannot fail");
    folded.push_str(&id);
    assert_eq!(
        crate::provenance::sha1_hex(&folded),
        native_contract_hash(),
        "the folded native+purrdf digest must reproduce native_contract_hash exactly"
    );

    // The purrdf identity is a DISTINCT surface from the native contract hash — a
    // BLAKE3 purrdf calculus digest is never the SHA-1 native engine seal.
    assert_ne!(
        owl_rl.to_hex(),
        native_contract_hash(),
        "purrdf's contract_hash and native_contract_hash are distinct identities"
    );
}

#[test]
fn is_absolute_iri_recognizes_schemeless_authority_worlds() {
    // http(s) worlds — the common case — stay named.
    assert!(is_absolute_iri(
        "https://blackcatinformatics.ca/gmeow/graph/w"
    ));
    assert!(is_absolute_iri("http://example.org/g"));
    // Schemeless-authority IRIs (no `://`) are ALSO absolute named worlds — the old
    // `contains("://")` check silently demoted these to the default graph.
    assert!(is_absolute_iri(
        "urn:uuid:2c8f0a1e-0000-4000-8000-000000000001"
    ));
    assert!(is_absolute_iri("did:example:123"));
    assert!(is_absolute_iri("tag:blackcat,2026:world"));
    assert!(is_absolute_iri("mailto:someone@example.org"));
    // A bare token / relative reference is NOT absolute.
    assert!(!is_absolute_iri("c14n44"));
    assert!(!is_absolute_iri("world-1"));
    assert!(!is_absolute_iri(":no-scheme"));
    assert!(!is_absolute_iri("1http://bad-scheme")); // scheme must start with a letter
}

/// Production-surface antecedent guard: the primary reasoning path
/// (`reason_all` → `reason_closure` → `run_reasoning` → `forward_oracle().materialize`
/// → `chase_rows_to_inferred`) must carry REAL native premises end-to-end, not
/// just non-empty inferred facts. `forward_oracle()` funnels the binary
/// seminaive branch here; A⊑B, B⊑C derives the transitive A⊑C, whose
/// `InferredAxiom::premises` must be NON-EMPTY (it cites its two body facts).
/// Falsifiable: the escaped empty-antecedents bug leaves EVERY derived
/// `premises` empty, tripping this at the production observable.
#[test]
fn reason_all_derived_axioms_carry_nonempty_premises() {
    let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
    let result = reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("native reason_all must decide the closure");

    // The transitive subClassOf(A, C) is derived (is_edb == false) and must
    // cite its immediate antecedents through `InferredAxiom::premises`.
    // `subject`/`predicate` are bare IRIs; `object` is `term_display`ed (an IRI
    // renders angle-bracketed), so match the object against its display form.
    let object_c = format!("<{C}>");
    let derived_transitive = result.inferred().iter().find(|ax| {
        !ax.is_edb
            && ax.predicate == SUBCLASS
            && ax.subject == A
            && crate::provenance::term_display(&ax.object) == object_c
    });
    let axiom = derived_transitive.unwrap_or_else(|| {
        panic!(
            "transitive subClassOf(A, C) must be derived; got {:?}",
            result.inferred()
        )
    });
    assert!(
        !axiom.premises.is_empty(),
        "derived subClassOf(A, C) must carry NON-EMPTY premises on the production path \
             (the empty-antecedents bug fails here); got {axiom:?}"
    );
}

#[test]
fn reason_all_single_chase_yields_inconsistent_and_nonempty_closure() {
    // A ⊑ B, A ⊑ C, B disjointWith C, x : A — one chase must derive both the
    // subsumption closure AND the inconsistency verdict (x forced into Nothing).
    let store = dataset(vec![
        quad(A, SUBCLASS, B),
        quad(A, SUBCLASS, C),
        quad(B, DISJOINT, C),
        quad(X, TYPE, A),
    ]);
    let result = reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_all should succeed");

    assert!(
        !result.is_consistent(),
        "x forced into owl:Nothing must make the verdict inconsistent (information=both)"
    );
    assert_eq!(
        result.information,
        crate::result::InformationState::Both,
        "an inconsistent verdict is the four-valued Belnap glut"
    );
    assert!(
        !result.inferred().is_empty(),
        "the subsumption closure must be non-empty (asserted + derived axioms)"
    );
    assert!(
        result
            .provenance
            .contradiction_witnesses
            .iter()
            .any(|w| w.individual == X),
        "x must be a contradiction witness: {:?}",
        result.provenance.contradiction_witnesses
    );
}

#[test]
fn reason_all_with_data_merges_user_abox_into_bundle_tbox() {
    // The contradiction is entailed only ACROSS the two inputs: the disjointness
    // TBox lives in `bundle`, the offending individual `x : A` in `user`. Neither
    // alone is inconsistent; the merge must feed both to the chase.
    let bundle = dataset(vec![
        quad(A, SUBCLASS, B),
        quad(A, SUBCLASS, C),
        quad(B, DISJOINT, C),
    ]);
    let user = dataset(vec![quad(X, TYPE, A)]);

    // The user ABox on its own (no TBox) is consistent.
    let user_only = reason_all(
        crate::reason::prepare_reasoning_input(user.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_all over user-only");
    assert!(
        user_only.is_consistent(),
        "x : A with no disjointness axioms is consistent"
    );

    // Merged with the bundle TBox, x is forced into owl:Nothing.
    let merged = reason_all_with_data(
        bundle.as_ref(),
        user.as_ref(),
        &SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_all_with_data should succeed");
    assert!(
        !merged.is_consistent(),
        "user data merged with the bundle TBox entails an inconsistency"
    );
    assert!(
        merged
            .provenance
            .contradiction_witnesses
            .iter()
            .any(|w| w.individual == X),
        "x must be a contradiction witness in the merged run: {:?}",
        merged.provenance.contradiction_witnesses
    );
}

// ── Program-carrying reason: the full-FOL formula layer actually evaluates ──

use gmeow_logic_compile::ir::{Formula, LogicProgram, PreservationKind, Term};

const KNOWS: &str = "http://gmeow.example/knows";
const TRUSTS: &str = "http://gmeow.example/trusts";
const ALICE: &str = "http://gmeow.example/alice";
const BOB: &str = "http://gmeow.example/bob";
const SAM: &str = "http://gmeow.example/sam";

fn fml_atom(rel: &str, args: Vec<Term>) -> Formula {
    Formula::atom(Term::iri(rel.to_owned()).unwrap(), args).unwrap()
}

#[test]
fn reason_program_evaluates_a_horn_formula_end_to_end() {
    // ∀x. (knows(x, alice) → trusts(x, bob)) is Horn-expressible, so it must lower to a
    // rule that the chase fires: given knows(sam, alice), the program must DERIVE
    // trusts(sam, bob). This is the formula layer evaluating end-to-end (not dead code).
    let formula = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(fml_atom(
                KNOWS,
                vec![
                    Term::var("x").unwrap(),
                    Term::iri(ALICE.to_owned()).unwrap(),
                ],
            )),
            Box::new(fml_atom(
                TRUSTS,
                vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
            )),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
    let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // Objects decode to their N3 surface (`<iri>`); subjects/predicates are bare IRIs.
    let bob_obj = format!("<{BOB}>");
    assert!(
        result.inferred().iter().any(|ax| {
            ax.subject == SAM
                && ax.predicate == TRUSTS
                && crate::provenance::term_display(&ax.object) == bob_obj
        }),
        "the Horn formula must derive trusts(sam, bob); closure: {:?}",
        result
            .inferred()
            .iter()
            .map(|a| (&a.subject, &a.predicate, &a.object))
            .collect::<Vec<_>>()
    );
    // The Horn formula lowers exactly — it adds no formula residue to the claim.
    assert!(
        !result
            .preservation
            .unsupported_constructs
            .iter()
            .any(|c| c.contains("formula") || c.contains("disjunct")),
        "a fully-evaluable Horn formula adds no formula residue: {:?}",
        result.preservation.unsupported_constructs
    );
}

/// A law with TERNARY atoms in its BODY and a BINARY head (the associativity shape,
/// like the algebra-axioms law) evaluates end-to-end: the reified n-ary body atoms
/// join through the chase and the binary consequent is derived. This exercises the
/// body-reification path (no head derivation) all the way through `reason_program`.
#[test]
fn reason_program_evaluates_an_nary_body_law_end_to_end() {
    // ∀a b c ab bc l r. op(a,b,ab) ∧ op(ab,c,l) ∧ op(b,c,bc) ∧ op(a,bc,r) → eq(l,r)
    // Seeded on a concrete associative table so both bracketings reach the SAME value v;
    // then eq(l,r) must be derived (l = v = r).
    const OP: &str = "http://gmeow.example/op";
    const EQ: &str = "http://gmeow.example/eq";
    let v = |n: &str| Term::var(n).unwrap();
    let law = Formula::Forall {
        vars: ["a", "b", "c", "ab", "bc", "l", "r"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                fml_atom(OP, vec![v("a"), v("b"), v("ab")]),
                fml_atom(OP, vec![v("ab"), v("c"), v("l")]),
                fml_atom(OP, vec![v("b"), v("c"), v("bc")]),
                fml_atom(OP, vec![v("a"), v("bc"), v("r")]),
            ])),
            Box::new(fml_atom(EQ, vec![v("l"), v("r")])),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

    // A concrete op table where (a·b)·c and a·(b·c) both reach `v` for a=x,b=y,c=z.
    // op is ternary → the EDB op facts are authored PRE-REIFIED (instanceOf + naryArg).
    const X: &str = "http://gmeow.example/x";
    const Y: &str = "http://gmeow.example/y";
    const Z: &str = "http://gmeow.example/z";
    const XY: &str = "http://gmeow.example/xy";
    const YZ: &str = "http://gmeow.example/yz";
    const V: &str = "http://gmeow.example/v";
    let io = "https://blackcatinformatics.ca/logic/instanceOf";
    let a0 = "https://blackcatinformatics.ca/logic/naryArg0";
    let a1 = "https://blackcatinformatics.ca/logic/naryArg1";
    let a2 = "https://blackcatinformatics.ca/logic/naryArg2";
    // Reify one op(s,t,u) tuple as instanceOf + naryArg triples on a fresh node.
    let mut quads = Vec::new();
    let mut reify = |node: &str, s: &str, t: &str, u: &str| {
        quads.push(quad(node, io, OP));
        quads.push(quad(node, a0, s));
        quads.push(quad(node, a1, t));
        quads.push(quad(node, a2, u));
    };
    reify("http://gmeow.example/r_xy", X, Y, XY); // x·y = xy
    reify("http://gmeow.example/r_xyz1", XY, Z, V); // (x·y)·z = v
    reify("http://gmeow.example/r_yz", Y, Z, YZ); // y·z = yz
    reify("http://gmeow.example/r_xyz2", X, YZ, V); // x·(y·z) = v
    let edb = dataset(quads);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // The binary consequent eq(l, r) = eq(v, v) must be derived.
    let eq_vv = result.inferred().iter().any(|ax| {
        ax.predicate == EQ
            && ax.subject == V
            && crate::provenance::term_display(&ax.object) == format!("<{V}>")
    });
    assert!(
        eq_vv,
        "associativity must derive eq(v, v); closure: {:?}",
        result
            .inferred()
            .iter()
            .filter(|a| a.predicate == EQ)
            .map(|a| (&a.subject, &a.object))
            .collect::<Vec<_>>()
    );
    // A fully-evaluable n-ary body law lowers exactly (no residue).
    assert!(
        !result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "an n-ary body law lowers exactly: {:?}",
        result.preservation
    );
}

// ── n-ary HEAD derivation: the det homomorphism law evaluates end-to-end ──

const MATMUL: &str = "http://gmeow.example/matMul";
const MUL: &str = "http://gmeow.example/mul";
const DET: &str = "http://gmeow.example/det";
const MAT_A: &str = "http://gmeow.example/A";
const MAT_B: &str = "http://gmeow.example/B";
const MAT_AB: &str = "http://gmeow.example/AB";
const DET_A: &str = "http://gmeow.example/dA";
const DET_B: &str = "http://gmeow.example/dB";
const DET_AB: &str = "http://gmeow.example/dAB";
const MATMUL_REIFIER: &str = "http://gmeow.example/reif/matMul-A-B-AB";
const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const NARY_REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/nary/";

fn logic_nary_arg(i: usize) -> String {
    format!("https://blackcatinformatics.ca/logic/naryArg{i}")
}

#[test]
fn reason_program_derives_an_nary_head_tuple_end_to_end() {
    // The determinant homomorphism law:
    //   ∀A,B,AB,dA,dB,dAB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) ∧ det(AB,dAB) → mul(dA,dB,dAB)
    // `matMul` is ternary → reified BODY atom; `mul` is ternary → reified HEAD (a derived
    // tuple). Seed a minimal deterministic pre-reified EDB (the matMul tuple as reified
    // instanceOf+naryArg triples, plus the three det facts) and assert the closure DERIVES
    // the reified `mul(dA,dB,dAB)` tuple.
    let law = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                fml_atom(
                    MATMUL,
                    vec![
                        Term::var("A").unwrap(),
                        Term::var("B").unwrap(),
                        Term::var("AB").unwrap(),
                    ],
                ),
                fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                fml_atom(
                    DET,
                    vec![Term::var("AB").unwrap(), Term::var("dAB").unwrap()],
                ),
            ])),
            Box::new(fml_atom(
                MUL,
                vec![
                    Term::var("dA").unwrap(),
                    Term::var("dB").unwrap(),
                    Term::var("dAB").unwrap(),
                ],
            )),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

    // Pre-reified EDB: matMul(A,B,AB) as instanceOf + naryArg triples, plus the det facts.
    let na = logic_nary_arg(0);
    let nb = logic_nary_arg(1);
    let nab = logic_nary_arg(2);
    let edb = dataset(vec![
        quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
        quad(MATMUL_REIFIER, &na, MAT_A),
        quad(MATMUL_REIFIER, &nb, MAT_B),
        quad(MATMUL_REIFIER, &nab, MAT_AB),
        quad(MAT_A, DET, DET_A),
        quad(MAT_B, DET, DET_B),
        quad(MAT_AB, DET, DET_AB),
    ]);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // Find the derived reifier R by the typing atom instanceOf(R, mul).
    let mul_obj = format!("<{MUL}>");
    let r = result
        .inferred()
        .iter()
        .find(|ax| {
            ax.predicate == LOGIC_INSTANCE_OF
                && crate::provenance::term_display(&ax.object) == mul_obj
        })
        .map(|ax| ax.subject.clone())
        .unwrap_or_else(|| {
            panic!(
                "the law must DERIVE instanceOf(R, mul); closure: {:?}",
                result
                    .inferred()
                    .iter()
                    .map(|a| (&a.subject, &a.predicate, &a.object))
                    .collect::<Vec<_>>()
            )
        });

    // The reifier is minted by TUPLE IDENTITY (mint_nary_reifier), not a frontier Skolem.
    assert!(
        r.starts_with(NARY_REIFIER_PREFIX),
        "R must be the content-addressed n-ary reifier IRI, got: {r}"
    );

    // Join on R: the three positional argument atoms carry the concrete det values.
    let has_arg = |i: usize, value: &str| {
        let pred = logic_nary_arg(i);
        let obj = format!("<{value}>");
        result.inferred().iter().any(|ax| {
            ax.subject == r
                && ax.predicate == pred
                && crate::provenance::term_display(&ax.object) == obj
        })
    };
    assert!(has_arg(0, DET_A), "naryArg0(R, dA) must be derived");
    assert!(has_arg(1, DET_B), "naryArg1(R, dB) must be derived");
    assert!(has_arg(2, DET_AB), "naryArg2(R, dAB) must be derived");

    // The law lowers exactly — no formula residue, preservation stays Exact.
    assert!(
        !result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "a range-restricted n-ary head lowers exactly (no SoundUnder): {:?}",
        result.preservation
    );
    assert!(
        !result
            .preservation
            .unsupported_constructs
            .iter()
            .any(|c| c.contains("formula") || c.contains("not bound") || c.contains("nary")),
        "no n-ary head residue disclosed: {:?}",
        result.preservation.unsupported_constructs
    );
}

/// The E8 group-action law `(g·h)·x = g·(h·x)`: TERNARY `comp`/`act` atoms in the
/// BODY (reified) and a BINARY `eq` head (a plain binary tuple, not reified). Seeded on
/// a concrete compatible action so both bracketings reach the SAME value `r`; then the
/// binary consequent `eq(r, r)` must be derived. This is the e8-symmetry law shape
/// evaluating end-to-end through `reason_program`.
#[test]
fn reason_program_closure_dataset_carries_the_derived_nary_tuple() {
    // The closure→RDF bridge (the native competency lane's substrate): the det law's
    // closure dataset, obtained via reason_program_closure_dataset, must contain the
    // DERIVED reified argument triple logic:naryArg0(R, dA) — a triple no query over the
    // asserted EDB alone could see (R is a chase-minted reifier).
    let law = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                fml_atom(
                    MATMUL,
                    vec![
                        Term::var("A").unwrap(),
                        Term::var("B").unwrap(),
                        Term::var("AB").unwrap(),
                    ],
                ),
                fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                fml_atom(
                    DET,
                    vec![Term::var("AB").unwrap(), Term::var("dAB").unwrap()],
                ),
            ])),
            Box::new(fml_atom(
                MUL,
                vec![
                    Term::var("dA").unwrap(),
                    Term::var("dB").unwrap(),
                    Term::var("dAB").unwrap(),
                ],
            )),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);
    let na = logic_nary_arg(0);
    let nb = logic_nary_arg(1);
    let nab = logic_nary_arg(2);
    let edb = dataset(vec![
        quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
        quad(MATMUL_REIFIER, &na, MAT_A),
        quad(MATMUL_REIFIER, &nb, MAT_B),
        quad(MATMUL_REIFIER, &nab, MAT_AB),
        quad(MAT_A, DET, DET_A),
        quad(MAT_B, DET, DET_B),
        quad(MAT_AB, DET, DET_AB),
    ]);

    let closure = reason_program_closure_dataset(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("closure dataset must build");

    // Scan the projected closure for logic:naryArg0(R, dA): R is the chase-minted
    // content-addressed reifier, dA the concrete det value.
    let na0 = logic_nary_arg(0);
    let found = closure.owned_quads().any(|q| {
        q.predicate == na0
            && q.object == RdfTerm::iri(DET_A)
            && matches!(&q.subject, RdfTerm::Iri(s) if s.starts_with(NARY_REIFIER_PREFIX))
    });
    assert!(
        found,
        "the closure dataset must carry the derived logic:naryArg0(R, dA) triple; quads: {:?}",
        closure
            .owned_quads()
            .map(|q| (q.subject.clone(), q.predicate.clone(), q.object.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn reason_program_evaluates_the_group_action_law_end_to_end() {
    // ∀g,h,x,gh,r1,hx,r2. comp(g,h,gh) ∧ act(gh,x,r1) ∧ act(h,x,hx) ∧ act(g,hx,r2) → eq(r1,r2)
    const COMP: &str = "http://gmeow.example/comp";
    const ACT: &str = "http://gmeow.example/act";
    const EQ: &str = "http://gmeow.example/eq";
    let v = |n: &str| Term::var(n).unwrap();
    let law = Formula::Forall {
        vars: ["g", "h", "x", "gh", "r1", "hx", "r2"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                fml_atom(COMP, vec![v("g"), v("h"), v("gh")]),
                fml_atom(ACT, vec![v("gh"), v("x"), v("r1")]),
                fml_atom(ACT, vec![v("h"), v("x"), v("hx")]),
                fml_atom(ACT, vec![v("g"), v("hx"), v("r2")]),
            ])),
            Box::new(fml_atom(EQ, vec![v("r1"), v("r2")])),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

    // A concrete compatible action where (g·h)·x and g·(h·x) both reach `r`.
    const G: &str = "http://gmeow.example/g";
    const H: &str = "http://gmeow.example/h";
    const XPT: &str = "http://gmeow.example/pt";
    const GH: &str = "http://gmeow.example/gh";
    const HX: &str = "http://gmeow.example/hx";
    const R: &str = "http://gmeow.example/r";
    let a0 = logic_nary_arg(0);
    let a1 = logic_nary_arg(1);
    let a2 = logic_nary_arg(2);
    // comp/act are ternary → the EDB atoms are authored PRE-REIFIED (instanceOf + naryArg).
    let mut quads = Vec::new();
    let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
        quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
        quads.push(quad(node, &a0, s));
        quads.push(quad(node, &a1, t));
        quads.push(quad(node, &a2, u));
    };
    reify("http://gmeow.example/r_comp", COMP, G, H, GH); // g·h = gh
    reify("http://gmeow.example/r_act1", ACT, GH, XPT, R); // (g·h)·x = r
    reify("http://gmeow.example/r_act2", ACT, H, XPT, HX); // h·x = hx
    reify("http://gmeow.example/r_act3", ACT, G, HX, R); // g·(h·x) = r
    let edb = dataset(quads);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // The binary consequent eq(r1, r2) = eq(r, r) must be derived.
    let eq_rr = result.inferred().iter().any(|ax| {
        ax.predicate == EQ
            && ax.subject == R
            && crate::provenance::term_display(&ax.object) == format!("<{R}>")
    });
    assert!(
        eq_rr,
        "the group-action law must derive eq(r, r); closure: {:?}",
        result
            .inferred()
            .iter()
            .filter(|a| a.predicate == EQ)
            .map(|a| (&a.subject, &a.object))
            .collect::<Vec<_>>()
    );
    // A fully-evaluable n-ary body law lowers exactly (no residue).
    assert!(
        !result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "the group-action law lowers exactly: {:?}",
        result.preservation
    );
}

/// The homomorphic-encryption law `Dec(E(a) ⊗ E(b)) = a ⊕ b`: BINARY `enc`/`dec`
/// atoms (plain body triples) plus TERNARY `ctMul`/`ptAdd` atoms (reified body atoms)
/// and a BINARY `eq` head. Seeded on concrete values so the decrypted ciphertext
/// product and the plaintext sum reach the SAME value `p`; then `eq(p, p)` must be
/// derived. This is the homomorphic-encryption law shape evaluating end-to-end.
#[test]
fn reason_program_evaluates_the_he_law_end_to_end() {
    // ∀a,b,ea,eb,prod,decv,sum.
    //   enc(a,ea) ∧ enc(b,eb) ∧ ctMul(ea,eb,prod) ∧ dec(prod,decv) ∧ ptAdd(a,b,sum) → eq(decv,sum)
    const ENC: &str = "http://gmeow.example/enc";
    const DEC: &str = "http://gmeow.example/dec";
    const CTMUL: &str = "http://gmeow.example/ctMul";
    const PTADD: &str = "http://gmeow.example/ptAdd";
    const EQ: &str = "http://gmeow.example/eq";
    let v = |n: &str| Term::var(n).unwrap();
    let law = Formula::Forall {
        vars: ["a", "b", "ea", "eb", "prod", "decv", "sum"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                fml_atom(ENC, vec![v("a"), v("ea")]),
                fml_atom(ENC, vec![v("b"), v("eb")]),
                fml_atom(CTMUL, vec![v("ea"), v("eb"), v("prod")]),
                fml_atom(DEC, vec![v("prod"), v("decv")]),
                fml_atom(PTADD, vec![v("a"), v("b"), v("sum")]),
            ])),
            Box::new(fml_atom(EQ, vec![v("decv"), v("sum")])),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

    // Concrete values: encrypt a→ea, b→eb; the ciphertext product decrypts to `p`, and
    // the plaintext sum is the SAME `p` (the homomorphic property holds on this instance).
    const A: &str = "http://gmeow.example/pa";
    const B: &str = "http://gmeow.example/pb";
    const EA: &str = "http://gmeow.example/ea";
    const EB: &str = "http://gmeow.example/eb";
    const PROD: &str = "http://gmeow.example/prod";
    const P: &str = "http://gmeow.example/p";
    let a0 = logic_nary_arg(0);
    let a1 = logic_nary_arg(1);
    let a2 = logic_nary_arg(2);
    // Binary enc/dec are PLAIN triples; ternary ctMul/ptAdd are PRE-REIFIED.
    let mut quads = vec![
        quad(A, ENC, EA),   // enc(a) = ea
        quad(B, ENC, EB),   // enc(b) = eb
        quad(PROD, DEC, P), // dec(prod) = p
    ];
    let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
        quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
        quads.push(quad(node, &a0, s));
        quads.push(quad(node, &a1, t));
        quads.push(quad(node, &a2, u));
    };
    reify("http://gmeow.example/r_ctmul", CTMUL, EA, EB, PROD); // ea ⊗ eb = prod
    reify("http://gmeow.example/r_ptadd", PTADD, A, B, P); // a ⊕ b = p
    let edb = dataset(quads);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // The binary consequent eq(decv, sum) = eq(p, p) must be derived.
    let eq_pp = result.inferred().iter().any(|ax| {
        ax.predicate == EQ
            && ax.subject == P
            && crate::provenance::term_display(&ax.object) == format!("<{P}>")
    });
    assert!(
        eq_pp,
        "the homomorphic-encryption law must derive eq(p, p); closure: {:?}",
        result
            .inferred()
            .iter()
            .filter(|a| a.predicate == EQ)
            .map(|a| (&a.subject, &a.object))
            .collect::<Vec<_>>()
    );
    // A fully-evaluable law (binary + reified-ternary body) lowers exactly (no residue).
    assert!(
        !result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "the homomorphic-encryption law lowers exactly: {:?}",
        result.preservation
    );
}

#[test]
fn reason_program_discloses_nary_head_unbound_arg_residue() {
    // A head variable the body does not bind is a non-range-restricted existential: the law
    // is carried as residue (SoundUnder) and derives NOTHING, never an unsafe tuple.
    let law = Formula::Forall {
        vars: vec![
            "A".into(),
            "B".into(),
            "AB".into(),
            "dA".into(),
            "dB".into(),
            "dAB".into(),
        ],
        body: Box::new(Formula::Implies(
            // Body binds dA, dB but NOT dAB.
            Box::new(Formula::And(vec![
                fml_atom(
                    MATMUL,
                    vec![
                        Term::var("A").unwrap(),
                        Term::var("B").unwrap(),
                        Term::var("AB").unwrap(),
                    ],
                ),
                fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
            ])),
            Box::new(fml_atom(
                MUL,
                vec![
                    Term::var("dA").unwrap(),
                    Term::var("dB").unwrap(),
                    Term::var("dAB").unwrap(),
                ],
            )),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);
    let na = logic_nary_arg(0);
    let nb = logic_nary_arg(1);
    let nab = logic_nary_arg(2);
    let edb = dataset(vec![
        quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
        quad(MATMUL_REIFIER, &na, MAT_A),
        quad(MATMUL_REIFIER, &nb, MAT_B),
        quad(MATMUL_REIFIER, &nab, MAT_AB),
        quad(MAT_A, DET, DET_A),
        quad(MAT_B, DET, DET_B),
    ]);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    assert!(
        result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "an unsafe (non-range-restricted) head must drop the claim to SoundUnder: {:?}",
        result.preservation.polarities
    );
    assert!(
        result
            .preservation
            .unsupported_constructs
            .iter()
            .any(|c| c.contains("not bound by the body")),
        "the range-restriction residue must be disclosed: {:?}",
        result.preservation.unsupported_constructs
    );
    // Nothing of the mul tuple is materialized.
    let mul_obj = format!("<{MUL}>");
    assert!(
        !result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == LOGIC_INSTANCE_OF
                && crate::provenance::term_display(&ax.object) == mul_obj),
        "an unsafe head derives no tuple: {:?}",
        result
            .inferred()
            .iter()
            .map(|a| (&a.subject, &a.predicate, &a.object))
            .collect::<Vec<_>>()
    );
}

#[test]
fn reason_program_discloses_non_horn_formula_residue() {
    // ∀x. (knows(x, alice) → (trusts(x, bob) ∨ trusts(x, sam))) has a disjunctive head:
    // it does NOT lower to a rule, so it must be disclosed as residue in the result's
    // preservation claim — flagged, never silently evaluated as one disjunct.
    let formula = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(fml_atom(
                KNOWS,
                vec![
                    Term::var("x").unwrap(),
                    Term::iri(ALICE.to_owned()).unwrap(),
                ],
            )),
            Box::new(Formula::Or(vec![
                fml_atom(
                    TRUSTS,
                    vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                ),
                fml_atom(
                    TRUSTS,
                    vec![Term::var("x").unwrap(), Term::iri(SAM.to_owned()).unwrap()],
                ),
            ])),
        )),
    };
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
    let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&edb).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("reason_program ok");

    // Eval-path honesty: the disjunctive formula is disclosed (SoundUnder), and it does
    // NOT silently materialize either disjunct.
    assert!(
        result
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder),
        "a non-evaluable formula must drop the claim to SoundUnder: {:?}",
        result.preservation.polarities
    );
    assert!(
        !result.preservation.unsupported_constructs.is_empty(),
        "the disjunctive residue must be disclosed, not silently absent"
    );
    assert!(
        !result.inferred().iter().any(|ax| ax.predicate == TRUSTS),
        "neither disjunct may be silently materialized: {:?}",
        result
            .inferred()
            .iter()
            .map(|a| (&a.subject, &a.predicate, &a.object))
            .collect::<Vec<_>>()
    );
}

// ── Mid-chase step governor: reason_all_budgeted CUTS the forward closure ──────────

/// A subclass chain c0 ⊑ c1 ⊑ … ⊑ c(n-1) with x : c0. The native DL closure derives
/// every transitive subsumption (O(n²)) and propagates x up the whole chain (O(n)), so
/// the committed-derivation count grows super-linearly across many semi-naive rounds —
/// a closure large enough that a tiny step budget must cut it mid-chase.
fn chain_dataset(n: usize) -> std::sync::Arc<purrdf::RdfDataset> {
    let cls = |i: usize| format!("http://gmeow.example/c{i}");
    let mut quads = Vec::new();
    for i in 0..n.saturating_sub(1) {
        quads.push(quad(&cls(i), SUBCLASS, &cls(i + 1)));
    }
    quads.push(quad(X, TYPE, &cls(0)));
    dataset(quads)
}

#[test]
fn reason_all_budgeted_cuts_the_chase_and_returns_a_strictly_smaller_partial_closure() {
    let store = chain_dataset(20);

    // Ground truth: the UNBUDGETED closure runs to full fixpoint.
    let full = reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("unbudgeted reason_all decides the closure");
    let full_len = full.inferred().len();
    assert!(
        full_len > 100,
        "the chain closure must be large enough to bound meaningfully; got {full_len}"
    );

    const MAX: u64 = 5;
    let budget = Budget {
        max_answers: None,
        max_steps: Some(MAX),
    };
    let cut = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &budget,
    )
    .expect("budgeted reason_all decides");

    // The cut is OBSERVED on the governor's own signal — the budget-exhausted status and
    // the committed step count — NOT inferred from a size comparison after a full run.
    assert_eq!(
        cut.evaluation,
        EvaluationStatus::BudgetExhausted,
        "a mid-chase cut is a non-conclusive budget-exhausted verdict"
    );
    assert_eq!(cut.completeness, CompletenessStatus::Incomplete);
    assert_eq!(
        cut.information,
        InformationState::Undetermined,
        "a truncated closure yields the honest Undetermined, never a wrong supported/both"
    );
    assert_eq!(
        cut.provenance.consumed_budget.consumed, MAX,
        "the governor admits EXACTLY max_steps committed derivations, then stops"
    );
    assert_eq!(cut.provenance.consumed_budget.allowance, Some(MAX));
    assert_eq!(
        cut.provenance.consumed_budget.limit,
        Some(crate::result::BudgetLimit::Inference)
    );

    // The materialized PARTIAL closure is STRICTLY smaller than the full closure: the
    // chase stopped deriving facts, it was not relabeled after running to completion.
    assert!(
        cut.inferred().len() < full_len,
        "partial closure ({}) must be strictly smaller than the full closure ({full_len})",
        cut.inferred().len()
    );
    assert!(
        !cut.inferred().is_empty(),
        "the partial closure still carries the EDB echo + the derivations the budget bought"
    );
}

#[test]
fn reason_all_budgeted_with_ample_budget_is_byte_identical_to_unbudgeted() {
    let store = chain_dataset(8);
    let full = reason_all(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("unbudgeted reason_all");

    // A ceiling far above the true closure never trips the governor.
    let ample = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: Some(100_000),
        },
    )
    .expect("ample budgeted reason_all");
    assert_eq!(
        ample.evaluation,
        EvaluationStatus::Completed,
        "an ample ceiling completes normally (no spurious truncation)"
    );
    assert_eq!(ample.provenance.consumed_budget.allowance, Some(100_000));
    let mut same_allowance = ample.clone();
    same_allowance.provenance.consumed_budget.allowance = None;
    assert_eq!(
        same_allowance, full,
        "all native facts, proofs and completion evidence are identical; only the requested allowance differs"
    );

    // The absent-budget (`None`) path is likewise byte-identical to today's reason_all.
    let none = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: None,
        },
    )
    .expect("none-budget reason_all");
    assert_eq!(
        none, full,
        "max_steps == None is the unbudgeted path — identical to reason_all"
    );
}

#[test]
fn reason_all_budgeted_partial_closure_is_deterministic() {
    let store = chain_dataset(16);
    let budget = Budget {
        max_answers: None,
        max_steps: Some(7),
    };
    let a = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &budget,
    )
    .expect("budgeted run a");
    let b = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(store.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &budget,
    )
    .expect("budgeted run b");
    assert_eq!(a.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(
        a, b,
        "the same input + the same small max_steps must yield the SAME partial closure"
    );
    assert_eq!(a.provenance.consumed_budget.consumed, 7);
}

#[test]
fn dl_consequences_cannot_spend_past_the_ordinary_allowance() {
    let store = dataset(vec![quad(
        A,
        "http://www.w3.org/2002/07/owl#complementOf",
        B,
    )]);
    let ordinary = ordinary_rule_closure(&store, dl::structured_dl_rules()).unwrap();
    assert!(
        !ordinary
            .inferred
            .iter()
            .any(|row| row.predicate == DISJOINT)
    );
    let full = reason_all(
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(full.inferred().iter().any(|row| row.predicate == DISJOINT));
    let cut = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: Some(ordinary.consumed_steps),
        },
    )
    .unwrap();
    assert_eq!(cut.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(cut.information, InformationState::Undetermined);
    assert_eq!(
        cut.provenance.consumed_budget.consumed,
        ordinary.consumed_steps
    );
    assert!(!cut.inferred().iter().any(|row| row.predicate == DISJOINT));
}

#[test]
fn dl_existential_prefix_retains_its_admission_and_witness_recipe() {
    let store = dataset(vec![
        quad(
            A,
            "http://www.w3.org/2002/07/owl#onProperty",
            "urn:has-child",
        ),
        quad(A, "http://www.w3.org/2002/07/owl#someValuesFrom", B),
        quad(X, TYPE, A),
    ]);
    let ordinary = ordinary_rule_closure(&store, dl::structured_dl_rules()).unwrap();
    let allowance = ordinary.consumed_steps + 1;
    let cut = reason_all_budgeted(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: Some(allowance),
        },
    )
    .unwrap();
    assert_eq!(cut.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(cut.provenance.consumed_budget.consumed, allowance);
    let execution = cut.native_execution().unwrap();
    assert!(!execution.chase_certificates.is_empty());
    assert!(!execution.witness_derivations.is_empty());
    assert!(cut.inferred().iter().any(|row| {
        row.rule_name
            .as_deref()
            .is_some_and(|name| name.contains("dl-existential"))
    }));
}

#[test]
fn contextual_requests_share_the_remaining_run_allowance() {
    let source = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <urn:context-budget:> .
ex:context a logic:AttributedContext ; logic:contextWorld ex:world ;
    logic:contextStandpoint ex:author ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:first a logic:ContextualEvaluationRequest ; logic:queryFormula ex:formula ;
    logic:queryContext ex:context .
ex:second a logic:ContextualEvaluationRequest ; logic:queryFormula ex:formula ;
    logic:queryContext ex:context .
ex:formula a logic:Formula ; logic:relation ex:ready ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:task ],
        [ logic:termIndex 1 ; logic:termIri ex:value ] .
ex:world { ex:claim <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>
    <<( ex:task ex:ready ex:value )>> ; gmeow:accordingTo ex:author ;
    gmeow:standpointSupportStatus gmeow:supportSupported . }
"#;
    let store = purrdf::parse_dataset(source.as_bytes(), "application/trig", None).unwrap();
    let result = reason_all_budgeted(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: Some(1),
        },
    )
    .expect("synthetic requests share the native run allowance");
    assert_eq!(result.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(result.provenance.consumed_budget.consumed, 1);
    let inferred = result
        .inferred()
        .iter()
        .filter(|row| row.rule_name.as_deref() == Some(crate::contextual::RULE_IRI))
        .collect::<Vec<_>>();
    assert_eq!(
        inferred
            .iter()
            .filter(|row| row.predicate == "https://blackcatinformatics.ca/logic/contextualResult")
            .count(),
        2
    );
    assert!(inferred.iter().any(|row| row.predicate
        == "https://blackcatinformatics.ca/logic/resultEvaluation"
        && row.object == TermValue::iri(EvaluationStatus::BudgetExhausted.iri())));
    assert!(
        inferred
            .iter()
            .all(|row| row.world == crate::result_rdf::GRAPH_REASONING)
    );
    assert!(
        !inferred
            .iter()
            .any(|row| row.predicate == "urn:context-budget:ready")
    );
}

#[test]
fn modal_verdicts_consume_the_remaining_run_allowance() {
    let store = dataset(modal_fixture_quads(
        "https://example.org/budget-modal",
        true,
    ));
    let full = reason_all(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let modal_count = full
        .inferred()
        .iter()
        .filter(|row| row.rule_name.as_deref() == Some(modal::MODAL_RULE_IRI))
        .count() as u64;
    assert_eq!(
        modal_count, 2,
        "necessity and counterexample each consume one step"
    );
    let before_modal = full.provenance.consumed_budget.consumed - modal_count;
    assert!(
        full.inferred()
            .iter()
            .any(|row| row.rule_name.as_deref() == Some(modal::MODAL_RULE_IRI))
    );
    for remaining in 0..=1 {
        let cut = reason_all_budgeted(
            crate::reason::prepare_reasoning_input(&store).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap(),
            &Budget {
                max_answers: None,
                max_steps: Some(before_modal + remaining),
            },
        )
        .unwrap();
        assert_eq!(cut.evaluation, EvaluationStatus::BudgetExhausted);
        assert_eq!(
            cut.provenance.consumed_budget.consumed,
            before_modal + remaining
        );
        assert_eq!(
            cut.inferred()
                .iter()
                .filter(|row| row.rule_name.as_deref() == Some(modal::MODAL_RULE_IRI))
                .count(),
            remaining as usize
        );
    }
}

#[test]
fn exhausted_dl_does_not_evaluate_a_modal_frame_over_incomplete_input() {
    let mut quads = modal_fixture_quads("https://example.org/partial-modal", false);
    quads.push(quad(A, "http://www.w3.org/2002/07/owl#complementOf", B));
    let store = dataset(quads);
    assert!(
        reason_all(
            crate::reason::prepare_reasoning_input(&store).unwrap(),
            &crate::reason::SelectedDomains::new([]).unwrap()
        )
        .is_err(),
        "the deliberately malformed active frame is rejected when reached"
    );
    let ordinary = ordinary_rule_closure(&store, dl::structured_dl_rules()).unwrap();
    let cut = reason_all_budgeted(
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
        &Budget {
            max_answers: None,
            max_steps: Some(ordinary.consumed_steps),
        },
    )
    .unwrap();
    assert_eq!(cut.evaluation, EvaluationStatus::BudgetExhausted);
    assert!(
        !cut.inferred()
            .iter()
            .any(|row| row.rule_name.as_deref() == Some(modal::MODAL_RULE_IRI))
    );
}

fn modal_fixture_quads(base: &str, include_atom_predicate: bool) -> Vec<RdfQuad> {
    const RELATION: &str = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let frame = format!("{base}/frame");
    let formula = format!("{base}/F");
    let body = format!("{base}/B");
    let w0 = format!("{base}/w0");
    let w1 = format!("{base}/w1");
    let w2 = format!("{base}/w2");
    let subject = format!("{base}/a");
    let predicate = format!("{base}/knows");
    let object = format!("{base}/b");

    let mut quads = vec![
        quad_in(&frame, &formula, crate::modal::NECESSARILY, &body),
        quad_in(&frame, &formula, crate::modal::OVER_ACCESSIBILITY, RELATION),
        quad_in(&frame, &formula, crate::modal::MODAL_EVAL_WORLD, &w0),
        quad_in(&frame, &body, crate::modal::ATOM_SUBJECT, &subject),
        quad_in(&frame, &body, crate::modal::ATOM_OBJECT, &object),
        quad_in(&frame, &w0, RELATION, &w1),
        quad_in(&frame, &w0, RELATION, &w2),
        quad_in(&w1, &subject, &predicate, &object),
    ];
    if include_atom_predicate {
        quads.push(quad_in(
            &frame,
            &body,
            crate::modal::ATOM_PREDICATE,
            &predicate,
        ));
    }
    quads
}

#[test]
fn reason_all_routes_modal_evaluation_through_the_native_closure() {
    let store = dataset(modal_fixture_quads("https://example.org/modal", true));
    let result = reason_all(
        prepare_reasoning_input(&store).unwrap(),
        &SelectedDomains::new([]).unwrap(),
    )
    .expect("production native reasoner evaluates modal frame");
    let failure = result
        .inferred()
        .iter()
        .find(|axiom| axiom.predicate == crate::modal::MODAL_NECESSITY_FAILS)
        .expect("necessity failure is in the production closure");
    assert_eq!(failure.world, "https://example.org/modal/frame");
    assert_eq!(failure.subject, "https://example.org/modal/F");
    assert_eq!(failure.object.as_iri(), Some("https://example.org/modal/B"));
    assert_eq!(
        failure.rule_name.as_deref(),
        Some(crate::modal::MODAL_RULE_IRI)
    );
    let evidence = failure
        .modal_evaluation
        .as_ref()
        .expect("contextual modal evidence");
    evidence.validate_axiom(failure).unwrap();
    assert_eq!(evidence.evaluation_world, "https://example.org/modal/w0");
    assert!(
        !evidence
            .positive_premises()
            .iter()
            .any(|p| p.context.ends_with("/w2") && p.predicate.ends_with("/knows"))
    );

    let counterexample = result
        .inferred()
        .iter()
        .find(|axiom| axiom.predicate == crate::modal::MODAL_COUNTEREXAMPLE_WORLD)
        .expect("counterexample world is in the production closure");
    assert_eq!(counterexample.world, "https://example.org/modal/frame");
    assert_eq!(
        counterexample.object.as_iri(),
        Some("https://example.org/modal/w2")
    );
    assert_eq!(
        counterexample.rule_name.as_deref(),
        Some(crate::modal::MODAL_RULE_IRI)
    );
    assert_eq!(counterexample.premises.len(), 9);
}

#[test]
fn modal_contexts_survive_native_receipts_rdf_and_explanation_without_absent_premises() {
    let base = "https://example.org/context-roundtrip";
    let mut quads = modal_fixture_quads(base, true);
    let alternate: Vec<_> = quads
        .iter()
        .filter(|q| q.graph_name == Some(RdfTerm::iri(format!("{base}/frame"))))
        .filter(|q| q.object != RdfTerm::iri(format!("{base}/w2")))
        .cloned()
        .map(|q| q.in_graph(RdfTerm::iri("urn:context:alternate")))
        .collect();
    quads.extend(alternate);
    let result = reason_all(
        crate::reason::prepare_reasoning_input(dataset(quads).as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    result.validate().unwrap();
    let modal_rows: Vec<_> = result
        .inferred()
        .iter()
        .filter(|q| q.modal_evaluation.is_some())
        .cloned()
        .collect();
    assert_eq!(modal_rows.len(), 3);
    assert!(
        modal_rows
            .iter()
            .any(|q| q.world == "urn:context:alternate"
                && q.predicate == modal::MODAL_NECESSITY_HOLDS)
    );
    let wire = crate::result_rdf::project_reasoning_result(&result).unwrap();
    let recovered = crate::result_rdf::parse_reasoning_graph(&wire).unwrap();
    let recovered_rows: Vec<_> = recovered
        .inferred()
        .iter()
        .filter(|q| q.modal_evaluation.is_some())
        .cloned()
        .collect();
    assert_eq!(recovered_rows, modal_rows);
    let native = inferred_axioms_to_dataset(result.inferred()).unwrap();
    for q in native.owned_quads().filter(|q| {
        q.predicate == modal::MODAL_NECESSITY_FAILS || q.predicate == modal::MODAL_NECESSITY_HOLDS
    }) {
        assert!(
            q.graph_name.is_some(),
            "a contextual verdict must never become a default assertion"
        );
    }
    let explanations = crate::explain::explanations_for_result(&result).unwrap();
    let modal_steps: Vec<_> = explanations
        .iter()
        .flat_map(|e| &e.step_skeleton)
        .filter(|s| s.modal_evaluation.is_some())
        .collect();
    assert!(!modal_steps.is_empty());
    assert!(
        modal_steps
            .iter()
            .all(|s| s.graph_iri == s.modal_evaluation.as_ref().unwrap().context)
    );
    assert!(
        !explanations
            .iter()
            .flat_map(|e| &e.step_skeleton)
            .any(|s| s.graph_iri == format!("{base}/w2")
                && s.predicate_iri == format!("{base}/knows"))
    );

    for mutation in 0..4 {
        let mut changed = result.clone();
        let crate::result::ResultPayload::Inferred(rows) = &mut changed.payload else {
            panic!("inferred");
        };
        let row = rows
            .iter_mut()
            .find(|q| q.modal_evaluation.is_some())
            .unwrap();
        match mutation {
            0 => row.modal_evaluation = None,
            1 => row.modal_evaluation.as_mut().unwrap().context = "urn:wrong:context".to_owned(),
            2 => {
                row.modal_evaluation.as_mut().unwrap().conclusion_predicate =
                    modal::MODAL_POSSIBILITY_FAILS.to_owned()
            }
            _ => row.premises.push((
                format!("{base}/a"),
                format!("{base}/knows"),
                format!("<{base}/b>"),
            )),
        }
        assert!(
            changed.validate().is_err(),
            "mutation {mutation} must fail native admission"
        );
        assert!(
            crate::result_rdf::project_reasoning_result(&changed).is_err(),
            "mutation {mutation} must fail RDF publication before malformed bytes escape"
        );
    }
}

#[test]
fn original_default_and_blank_frame_contexts_are_retained_without_global_assertion() {
    let base = "https://example.org/default-modal";
    let mut quads = modal_fixture_quads(base, true);
    for quad in &mut quads {
        if quad.graph_name == Some(RdfTerm::iri(format!("{base}/frame"))) {
            quad.graph_name = None;
        }
    }
    let default = reason_all(
        crate::reason::prepare_reasoning_input(dataset(quads.clone()).as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    assert!(
        default
            .inferred()
            .iter()
            .any(|q| q.modal_evaluation.is_some() && q.world == rl::DEFAULT_WORLD)
    );
    for quad in &mut quads {
        if quad.graph_name.is_none() {
            quad.graph_name = Some(RdfTerm::blank_node("frame"));
        }
    }
    let blank = reason_all(
        crate::reason::prepare_reasoning_input(dataset(quads).as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .unwrap();
    let row = blank
        .inferred()
        .iter()
        .find(|q| q.modal_evaluation.is_some())
        .unwrap();
    assert_ne!(row.world, rl::DEFAULT_WORLD);
    assert_ne!(row.world, format!("{base}/w0"));
    assert_eq!(row.world, row.modal_evaluation.as_ref().unwrap().context);
}

#[test]
fn reason_all_ignores_unrelated_literal_sharpens_without_a_modal_frame() {
    let unrelated = RdfQuad::new(
        RdfTerm::iri("https://example.org/domain/source"),
        "https://blackcatinformatics.ca/gmeow/sharpens",
        RdfTerm::literal(purrdf::RdfLiteral::simple("a non-world value")),
    )
    .in_graph(RdfTerm::iri("https://example.org/domain/world"));
    let result = reason_all(
        crate::reason::prepare_reasoning_input(dataset(vec![unrelated]).as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("ordinary sharpens data does not enter modal frame validation");
    assert!(
        !result
            .inferred()
            .iter()
            .any(|axiom| axiom.rule_name.as_deref() == Some(crate::modal::MODAL_RULE_IRI))
    );
}

#[test]
fn reason_all_hard_fails_on_a_malformed_active_modal_edge() {
    const RELATION: &str = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let base = "https://example.org/malformed-active-edge";
    let mut quads = modal_fixture_quads(base, true);
    quads.push(
        RdfQuad::new(
            RdfTerm::iri(format!("{base}/w0")),
            RELATION,
            RdfTerm::literal(purrdf::RdfLiteral::simple("not a world IRI")),
        )
        .in_graph(RdfTerm::iri(format!("{base}/frame"))),
    );

    let err = reason_all(
        crate::reason::prepare_reasoning_input(dataset(quads).as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect_err("an edge selected by the modal frame must remain fail-closed");
    assert!(
        err.message()
            .contains("typed accessibility edge target world must be an IRI"),
        "got: {err}"
    );
}

#[test]
fn incremental_ground_insert_recomputes_modal_verdicts_across_worlds() {
    let base = "https://example.org/incremental-modal";
    let w2 = format!("{base}/w2");
    let subject = format!("{base}/a");
    let predicate = format!("{base}/knows");
    let object_iri = format!("{base}/b");

    let base_quads = modal_fixture_quads(base, true);
    let base_edb = dataset(base_quads.clone());

    let mut candidate_quads = base_quads;
    candidate_quads.push(quad_in(&w2, &subject, &predicate, &object_iri));
    let with_candidate_edb = dataset(candidate_quads);
    let scratch = reason_all(
        crate::reason::prepare_reasoning_input(with_candidate_edb.as_ref()).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("scratch reasoning recomputes the completed modal frame");
    let fact = crate::rule_ir::Fact {
        subject: TermValue::iri(&subject),
        predicate: predicate.clone(),
        object: TermValue::iri(object_iri),
    };
    let session = NativeReasoningSession::new(
        prepare_reasoning_input(&base_edb).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        vec![(w2.clone(), fact.clone())],
    )
    .expect("base modal frame reasons");
    let base_result = session.base();
    assert!(base_result.inferred().iter().any(|axiom| {
        axiom.predicate == crate::modal::MODAL_NECESSITY_FAILS
            && axiom.rule_name.as_deref() == Some(crate::modal::MODAL_RULE_IRI)
    }));
    assert!(base_result.inferred().iter().any(|axiom| {
        axiom.predicate == crate::modal::MODAL_COUNTEREXAMPLE_WORLD
            && crate::provenance::term_display(&axiom.object) == format!("<{w2}>")
    }));

    let incremental = session
        .insert(LogicalGraph::Named(TermValue::iri(&w2)), fact.clone(), None)
        .expect("incremental reasoning recomputes modal verdicts");

    let modal_axioms = |result: &ReasoningResult| {
        result
            .inferred()
            .iter()
            .filter(|axiom| axiom.rule_name.as_deref() == Some(crate::modal::MODAL_RULE_IRI))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(incremental.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(
        modal_axioms(&incremental.result),
        modal_axioms(&scratch),
        "incremental modal rows must be identical to a scratch production closure"
    );
    assert!(incremental.result.inferred().iter().any(|axiom| {
        axiom.predicate == crate::modal::MODAL_NECESSITY_HOLDS
            && axiom.rule_name.as_deref() == Some(crate::modal::MODAL_RULE_IRI)
    }));
    assert!(!incremental.result.inferred().iter().any(|axiom| {
        axiom.predicate == crate::modal::MODAL_NECESSITY_FAILS
            || axiom.predicate == crate::modal::MODAL_COUNTEREXAMPLE_WORLD
    }));

    let cut = session
        .insert(LogicalGraph::Named(TermValue::iri(&w2)), fact, Some(0))
        .unwrap();
    assert_eq!(cut.status, BudgetStatus::Exhausted);
    assert_eq!(cut.consumed_steps, 0);
    assert_eq!(cut.result.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(cut.result.information, InformationState::Undetermined);
    assert!(
        modal_axioms(&cut.result).is_empty(),
        "neither stale nor uncharged modal verdicts survive a cut"
    );
}

#[test]
fn incremental_ground_insert_governs_new_dl_witnesses() {
    let base_quads = vec![
        quad(
            A,
            "http://www.w3.org/2002/07/owl#onProperty",
            "urn:has-child",
        ),
        quad(A, "http://www.w3.org/2002/07/owl#someValuesFrom", B),
    ];
    let base_edb = dataset(base_quads.clone());
    let fact = crate::rule_ir::Fact {
        subject: TermValue::iri(X),
        predicate: TYPE.to_owned(),
        object: TermValue::iri(A),
    };
    let session = NativeReasoningSession::new(
        prepare_reasoning_input(&base_edb).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        vec![(W.to_owned(), fact.clone())],
    )
    .unwrap();
    let run = |max_steps| {
        session
            .insert(
                LogicalGraph::Named(TermValue::iri(W)),
                fact.clone(),
                max_steps,
            )
            .unwrap()
    };
    let full = run(None);
    assert_eq!(full.status, BudgetStatus::Ok);
    assert!(
        full.result
            .inferred()
            .iter()
            .any(|row| row.predicate == "urn:has-child")
    );
    assert!(
        full.consumed_steps >= 2,
        "the existential head publishes a link and a filler type"
    );
    let cut = run(Some(0));
    assert_eq!(cut.status, BudgetStatus::Exhausted);
    assert_eq!(cut.result.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(cut.consumed_steps, 0);
    assert!(
        !cut.result
            .inferred()
            .iter()
            .any(|row| row.predicate == "urn:has-child")
    );
}

#[test]
fn native_run_is_atomic_when_any_modal_frame_is_malformed() {
    let mut quads = modal_fixture_quads("https://example.org/valid-modal", true);
    quads.extend(modal_fixture_quads(
        "https://example.org/malformed-modal",
        false,
    ));
    let edb = dataset(quads);
    let input = prepare_reasoning_input(&edb).unwrap();
    let before = (*input.ingress_contract(), input.source_contexts().clone());
    let err = reason_all(input.clone(), &SelectedDomains::new([]).unwrap()).unwrap_err();
    assert!(
        err.message().contains(crate::modal::ATOM_PREDICATE),
        "got: {err}"
    );
    assert_eq!(
        (*input.ingress_contract(), input.source_contexts().clone()),
        before,
        "a refused run cannot mutate the admitted source"
    );
}

#[test]
fn reason_program_routes_modal_evaluation_through_the_native_closure() {
    // `reason_program` (the program-carrying path the slicetest competency-question
    // projection and the conjecture lane run over) must evaluate typed modal frames on its
    // completed closure, exactly like `reason_all`/`reason_all_budgeted` —
    // otherwise a completed program result could silently omit modal verdicts. An empty
    // program carries the modal frame purely as EDB, so this exercises the same shared
    // kernel through the program entry point.
    let program = LogicProgram::new(vec![], vec![], vec![], None);
    let store = dataset(modal_fixture_quads("https://example.org/modal", true));
    let result = reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::reason::SelectedDomains::new([]).unwrap(),
    )
    .expect("program reasoner evaluates the frame");

    let failure = result
        .inferred()
        .iter()
        .find(|axiom| axiom.predicate == crate::modal::MODAL_NECESSITY_FAILS)
        .expect("necessity failure is in the program closure");
    assert_eq!(failure.world, "https://example.org/modal/frame");
    assert_eq!(failure.subject, "https://example.org/modal/F");
    assert_eq!(failure.object.as_iri(), Some("https://example.org/modal/B"));
    assert_eq!(
        failure.rule_name.as_deref(),
        Some(crate::modal::MODAL_RULE_IRI)
    );

    assert!(
        result
            .inferred()
            .iter()
            .any(|axiom| axiom.predicate == crate::modal::MODAL_COUNTEREXAMPLE_WORLD),
        "the counterexample world must reach the program closure too"
    );
}
