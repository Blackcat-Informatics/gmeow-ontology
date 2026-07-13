// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native backward-query dispatch.
//!
//! `dispatch_query` is the single production entry point for goal resolution. It
//! applies the profile gates and delegates to the native demand-transformed physical
//! core. A fragment the native core cannot soundly decide is a typed hard failure:
//! there is no secondary engine, silent approximation, or demotion route.

use crate::annotation::{
    AnnotatedAnswerSet, AnnotationFactRef, AnnotationRequest, TupleAnnotationAlgebra,
};
use crate::profile_gate;
use crate::query_ir::{AnswerSet, Budget, QProgram};
use crate::seam::WorldFactSource;

/// Content identity of the backward execution contract available at this boundary.
///
/// The rule program has its own canonical digest in the physical plan key. This digest
/// covers the remaining semantics/resource inputs that can change dispatch behavior,
/// with explicit option tags so `None` cannot alias a numeric zero.
///
/// `pub(crate)` so the stable runtime façade ([`crate::runtime::EngineContract`]) can
/// surface the per-query contract from THIS single source — a runtime consumer that
/// records "answer minted under contract Y" reproduces the same Y, and there is never a
/// second copy of this hash to drift.
pub(crate) fn query_contract_hash(profile: &str, budget: &Budget) -> String {
    fn frame(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"gmeow-backward-query-contract-v1");
    let canonical_profile = profile_gate::canonical_profile_identity(profile);
    frame(&mut hasher, canonical_profile.as_bytes());
    match budget.max_answers {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&(value as u64).to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    match budget.max_steps {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&value.to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// Resolve `program` against `world` with the native physical core.
///
/// # Errors
///
/// Returns `Err` from a profile gate, from the native engine, or when the native
/// engine reports an unsupported fragment. Unsupported never means an empty answer.
pub fn dispatch_query(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> gmeow_errors::Result<AnswerSet> {
    profile_gate::reject_cut(program)?;
    profile_gate::check_builtin_profile(program, profile)?;

    let contract_hash = query_contract_hash(profile, budget);
    match crate::physical::resolve_native_under(&contract_hash, foreign, world, program, budget)? {
        crate::physical::NativeOutcome::Decided(answer) => Ok(answer),
        crate::physical::NativeOutcome::Unsupported(kind) => {
            Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "native backward engine does not support {kind:?}; query refused because \
                     no fallback engine remains"
                ),
            }))
        }
    }
}

/// Resolve `program` while carrying opaque annotations through native derivations.
///
/// `annotation_for` is consulted for asserted world facts. Returning `None` assigns
/// `algebra.one()`. Body conjunction uses `multiply`; alternative derivations use
/// `add`; each answer exposes the combined value plus its direct derivation lineage.
/// The annotation contract is independently content-framed into the plan identity, so
/// an exact-semiring call cannot alias a declared approximation call.
///
/// # Errors
///
/// Returns `Err` for a profile/fragment refusal, an annotation contract mismatch, an
/// algebra failure, or a non-convergent annotation fixed point. Annotation dispatch is
/// currently the native binary positive-Datalog seam; unsupported n-ary or negated
/// programs hard-fail rather than silently losing scores.
pub fn dispatch_query_annotated<A, F>(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    annotation: AnnotationRequest<'_, A, F>,
) -> gmeow_errors::Result<AnnotatedAnswerSet<A::Element>>
where
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    profile_gate::reject_cut(program)?;
    profile_gate::check_builtin_profile(program, profile)?;

    let base_contract = query_contract_hash(profile, budget);
    let annotation_frame = annotation.contract.canonical_key();
    let contract_hash = blake3::hash(
        format!("gmeow-annotated-query-contract-v1:{base_contract}:{annotation_frame}").as_bytes(),
    )
    .to_hex()
    .to_string();
    match crate::physical::resolve_native_annotated_under(
        &contract_hash,
        foreign,
        world,
        program,
        budget,
        &annotation,
    )? {
        crate::physical::NativeOutcome::Decided(answer) => Ok(answer),
        crate::physical::NativeOutcome::Unsupported(kind) => {
            Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "native annotated backward engine does not support {kind:?}; query refused because annotations cannot be demoted to post-hoc scoring"
                ),
            }))
        }
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::seam::{BudgetStatus, WorldFactSnapshot};
    use crate::store::WorldStore;

    const BASE: &str = "https://example.org/";
    const W: &str = "http://logic.test/world/dispatch";
    const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const STRATIFIED_NAF_PROFILE: &str =
        "https://blackcatinformatics.ca/logic/StratifiedNAFProfile";
    const PROCEDURAL_PROFILE: &str = "https://blackcatinformatics.ca/logic/ProceduralPrologProfile";
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

    fn p(local: &str) -> String {
        format!("{BASE}{local}")
    }

    fn rdf(local: &str) -> String {
        format!("{RDF}{local}")
    }

    struct PeakAlgebra {
        _runtime_identity: String,
    }

    struct FloatingScore;

    impl crate::annotation::TupleAnnotationAlgebra for FloatingScore {
        type Element = f64;

        fn zero(&self) -> Self::Element {
            0.0
        }

        fn one(&self) -> Self::Element {
            1.0
        }

        fn add(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            let score = left + right;
            if score.is_finite() {
                Ok(score)
            } else {
                Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: "floating score addition produced a non-finite value".to_owned(),
                }))
            }
        }

        fn multiply(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            let score = left * right;
            if score.is_finite() {
                Ok(score)
            } else {
                Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: "floating score multiplication produced a non-finite value".to_owned(),
                }))
            }
        }
    }

    impl crate::annotation::TupleAnnotationAlgebra for PeakAlgebra {
        type Element = i64;

        fn zero(&self) -> Self::Element {
            0
        }

        fn one(&self) -> Self::Element {
            0
        }

        fn add(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            Ok((*left).max(*right))
        }

        fn multiply(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            Ok((*left).max(*right))
        }
    }

    #[test]
    fn query_contract_hash_covers_profile_and_resource_limits() {
        let unlimited = Budget::default();
        assert_eq!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash(HORN_PROFILE, &unlimited)
        );
        assert_ne!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash(PROCEDURAL_PROFILE, &unlimited)
        );
        assert_ne!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash(
                HORN_PROFILE,
                &Budget {
                    max_answers: Some(0),
                    max_steps: Some(0),
                }
            )
        );
        assert_eq!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash("logic:PositiveHornProfile", &unlimited)
        );
        assert_eq!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash("PositiveHornProfile", &unlimited)
        );
        assert_eq!(
            query_contract_hash(HORN_PROFILE, &unlimited),
            query_contract_hash(
                "<https://blackcatinformatics.ca/logic/PositiveHornProfile>",
                &unlimited,
            )
        );
    }

    /// Build a 3-element RDF list (x y z) at l0 → l1 → l2 → rdf:nil in a fresh world.
    fn list_world() -> (WorldStore, &'static str) {
        let store = WorldStore::new();
        let first = rdf("first");
        let rest = rdf("rest");
        let nil = rdf("nil");
        store.insert_quad(W, &p("l0"), &first, &p("x"));
        store.insert_quad(W, &p("l0"), &rest, &p("l1"));
        store.insert_quad(W, &p("l1"), &first, &p("y"));
        store.insert_quad(W, &p("l1"), &rest, &p("l2"));
        store.insert_quad(W, &p("l2"), &first, &p("z"));
        store.insert_quad(W, &p("l2"), &rest, &nil);
        (store, W)
    }

    // ── dispatch_query end-to-end (recursive ancestor, 3 answers) ─────────────

    #[test]
    fn dispatch_query_recursive_ancestor_runs_native() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("b"), &p("parentOf"), &p("c"));
        store.insert_quad(W, &p("c"), &p("parentOf"), &p("d"));

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans = dispatch_query(&foreign, W, &prog, HORN_PROFILE, &budget).unwrap();

        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(
            ans.bindings.len(),
            3,
            "expected 3 transitive ancestors: {ans:?}"
        );
        let ys: Vec<&str> = ans.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{BASE}b>").as_str()),
            "missing b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}c>").as_str()),
            "missing c: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}d>").as_str()),
            "missing d: {ys:?}"
        );
    }

    #[test]
    fn annotated_dispatch_combines_body_scores_and_alternative_derivations() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("edge"), &p("b"));
        store.insert_quad(W, &p("b"), &p("edge"), &p("c"));
        store.insert_quad(W, &p("a"), &p("edge"), &p("c"));

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:path(X, Y) :- ex:edge(X, Y).\n\
             ex:path(X, Z) :- ex:path(X, Y), ex:edge(Y, Z).\n\
             ?- ex:path(ex:a, Y).\n"
        );
        let program = parse_query_program(&src).unwrap();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let answer = dispatch_query_annotated(
            &foreign,
            W,
            &program,
            HORN_PROFILE,
            &Budget::default(),
            crate::annotation::AnnotationRequest::new(
                &FloatingScore,
                &crate::annotation::AnnotationContract::exact(),
                |fact: crate::annotation::AnnotationFactRef<'_>| {
                    if fact.predicate != p("edge") {
                        return None;
                    }
                    let key = (
                        crate::provenance::term_display(fact.subject),
                        crate::provenance::term_display(fact.object),
                    );
                    match key {
                        (subject, object)
                            if subject == format!("<{BASE}a>")
                                && object == format!("<{BASE}b>") =>
                        {
                            Some(2.0)
                        }
                        (subject, object)
                            if subject == format!("<{BASE}b>")
                                && object == format!("<{BASE}c>") =>
                        {
                            Some(3.0)
                        }
                        (subject, object)
                            if subject == format!("<{BASE}a>")
                                && object == format!("<{BASE}c>") =>
                        {
                            Some(4.0)
                        }
                        _ => None,
                    }
                },
            ),
        )
        .unwrap();

        assert_eq!(
            answer.certification.query_class,
            crate::annotation::AnnotationQueryClass::PositiveRecursive
        );
        let by_y: std::collections::BTreeMap<&str, _> = answer
            .answers
            .iter()
            .map(|row| (row.binding["Y"].as_str(), row))
            .collect();
        let b = by_y[format!("<{BASE}b>").as_str()];
        assert_eq!(b.annotation, 2.0);
        assert_eq!(b.derivations.len(), 1);
        let c = by_y[format!("<{BASE}c>").as_str()];
        assert_eq!(c.annotation, 10.0, "direct 4 plus path product 2*3");
        assert_eq!(c.derivations.len(), 2, "both score lineages survive");
        assert!(
            c.derivations
                .iter()
                .any(|derivation| derivation.annotation == 6.0 && derivation.sources.len() == 2),
            "{:#?}",
            c.derivations
        );
    }

    #[test]
    fn annotated_dispatch_scopes_declared_law_deviation_to_query_class() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("edge"), &p("b"));
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:path(X, Y) :- ex:edge(X, Y).\n\
             ex:path(X, Z) :- ex:path(X, Y), ex:edge(Y, Z).\n\
             ?- ex:path(ex:a, Y).\n"
        );
        let program = parse_query_program(&src).unwrap();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let contract = crate::annotation::AnnotationContract::complete_over(
            [crate::annotation::SemiringLaw::Distributive],
            [crate::annotation::AnnotationQueryClass::PositiveAcyclic],
        );
        let error = dispatch_query_annotated(
            &foreign,
            W,
            &program,
            HORN_PROFILE,
            &Budget::default(),
            crate::annotation::AnnotationRequest::new(
                &crate::provenance::ZWeightSemiring,
                &contract,
                |_fact: crate::annotation::AnnotationFactRef<'_>| None,
            ),
        )
        .unwrap_err();
        assert!(
            error
                .message()
                .contains("not certified for actual query class PositiveRecursive"),
            "{error}"
        );

        let admitted_contract = crate::annotation::AnnotationContract::complete_over(
            [crate::annotation::SemiringLaw::ZeroAnnihilates],
            [crate::annotation::AnnotationQueryClass::PositiveRecursive],
        );
        let answer = dispatch_query_annotated(
            &foreign,
            W,
            &program,
            HORN_PROFILE,
            &Budget::default(),
            crate::annotation::AnnotationRequest::new(
                &PeakAlgebra {
                    _runtime_identity: "lillith-recall-peak-v1".to_owned(),
                },
                &admitted_contract,
                |fact: crate::annotation::AnnotationFactRef<'_>| {
                    (fact.predicate == p("edge")).then_some(7)
                },
            ),
        )
        .unwrap();
        assert_eq!(answer.answers[0].annotation, 7);
        assert_eq!(
            answer.certification.declared_deviations,
            [crate::annotation::SemiringLaw::ZeroAnnihilates]
                .into_iter()
                .collect()
        );
        assert!(
            answer
                .certification
                .preservation
                .polarities
                .contains(&gmeow_logic_compile::ir::PreservationKind::CompleteOver)
        );
    }

    #[test]
    fn annotated_dispatch_hard_fails_a_nonconvergent_recursive_algebra() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("edge"), &p("a"));
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:path(X, Y) :- ex:edge(X, Y).\n\
             ex:path(X, Z) :- ex:path(X, Y), ex:edge(Y, Z).\n\
             ?- ex:path(ex:a, Y).\n"
        );
        let program = parse_query_program(&src).unwrap();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let contract = crate::annotation::AnnotationContract::exact().with_max_fixpoint_rounds(4);
        let error = dispatch_query_annotated(
            &foreign,
            W,
            &program,
            HORN_PROFILE,
            &Budget::default(),
            crate::annotation::AnnotationRequest::new(
                &crate::provenance::ZWeightSemiring,
                &contract,
                |_fact: crate::annotation::AnnotationFactRef<'_>| Some(1),
            ),
        )
        .unwrap_err();
        assert!(
            error
                .message()
                .contains("annotation fixed point did not converge within 4 deterministic rounds"),
            "{error}"
        );
    }

    // ── Arithmetic-builtin list functions (G2a) ─────────────────────────
    //
    // Over the list (x y z): l0 →first x, →rest l1; l1 →first y, →rest l2;
    // l2 →first z, →rest rdf:nil. Binary list-length runs via the native core.
    // The former SLD fallback also served n-ary arithmetic programs; production now
    // exposes those residuals as typed refusals instead of silently changing semantics.

    const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    #[test]
    fn dispatch_query_ground_naf_only_rule_evaluates_under_all_free_goal() {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b) :- \\+ ex:q(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        let store = WorldStore::new();
        let foreign = WorldFactSnapshot::from_world(&store, W, STRATIFIED_NAF_PROFILE).unwrap();
        let answer = dispatch_query(
            &foreign,
            W,
            &prog,
            STRATIFIED_NAF_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(answer.status, BudgetStatus::Ok);
        assert_eq!(
            answer.bindings.len(),
            1,
            "absent q must let p fire: {answer:?}"
        );
        assert_eq!(answer.bindings[0]["X"], format!("<{BASE}a>"));
        assert_eq!(answer.bindings[0]["Y"], format!("<{BASE}b>"));

        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("q"), &p("b"));
        let foreign = WorldFactSnapshot::from_world(&store, W, STRATIFIED_NAF_PROFILE).unwrap();
        let answer = dispatch_query(
            &foreign,
            W,
            &prog,
            STRATIFIED_NAF_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert!(
            answer.bindings.is_empty(),
            "present q must block the NAF-only rule: {answer:?}"
        );
    }

    #[test]
    fn dispatch_query_builtin_only_rule_evaluates_adjacent_assignments() {
        let store = WorldStore::new();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- X is 1, Y is 2.\n\
             ?- ex:p(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let answer =
            dispatch_query(&foreign, W, &prog, PROCEDURAL_PROFILE, &Budget::default()).unwrap();
        assert_eq!(answer.status, BudgetStatus::Ok);
        assert_eq!(
            answer.bindings.len(),
            1,
            "builtin-only rule must fire once: {answer:?}"
        );
        assert_eq!(answer.bindings[0]["X"], format!("\"1\"^^<{XSD_INT}>"));
        assert_eq!(answer.bindings[0]["Y"], format!("\"2\"^^<{XSD_INT}>"));
    }

    #[test]
    fn list_length_via_arithmetic_builtin() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = dispatch_query(
            &foreign,
            world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(ans.bindings.len(), 1, "exactly one length answer: {ans:?}");
        assert_eq!(ans.bindings[0]["N"], format!("\"3\"^^<{XSD_INT}>"));
    }

    /// The binary arithmetic list-length program is DECIDED by the native core,
    /// so `dispatch_query` returns the native answer. Probing `resolve_native`
    /// directly proves this program is inside the supported fragment.
    #[test]
    fn binary_arithmetic_is_decided_by_native_not_demoted() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // Native decides it directly — not an Unsupported gap that would demote.
        let outcome =
            crate::physical::resolve_native(&foreign, world, &prog, &Budget::default()).unwrap();
        let crate::physical::NativeOutcome::Decided(answer) = outcome else {
            panic!("binary arithmetic must be decided natively, not demoted: {outcome:?}");
        };
        assert_eq!(answer.bindings.len(), 1);
        assert_eq!(answer.bindings[0]["N"], format!("\"3\"^^<{XSD_INT}>"));
    }

    #[test]
    fn nary_list_get_is_a_typed_arithmetic_refusal() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:get(L, 0, X) :- rdf:first(L, X).\n\
             ex:get(L, N, X) :- N > 0, rdf:rest(L, R), M is N - 1, ex:get(R, M, X).\n\
             ?- ex:get(ex:l0, 1, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let error = dispatch_query(
            &foreign,
            world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .expect_err("n-ary arithmetic must not outlive its retired fallback");
        assert_eq!(
            error.message(),
            "native backward engine does not support Arithmetic; query refused because no fallback engine remains"
        );
    }

    #[test]
    fn nary_list_index_of_is_a_typed_arithmetic_refusal() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:idx(L, X, 0) :- rdf:first(L, X).\n\
             ex:idx(L, X, N) :- rdf:rest(L, R), ex:idx(R, X, M), N is M + 1.\n\
             ?- ex:idx(ex:l0, ex:z, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let error = dispatch_query(
            &foreign,
            world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .expect_err("n-ary arithmetic must not outlive its retired fallback");
        assert_eq!(
            error.message(),
            "native backward engine does not support Arithmetic; query refused because no fallback engine remains"
        );
    }

    #[test]
    fn nary_comparison_program_is_a_typed_arithmetic_refusal() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:idx(L, X, 0) :- rdf:first(L, X).\n\
             ex:idx(L, X, N) :- rdf:rest(L, R), ex:idx(R, X, M), N is M + 1.\n\
             ex:positive(X, N) :- ex:idx(ex:l0, X, N), N > 0.\n\
             ?- ex:positive(X, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let error = dispatch_query(
            &foreign,
            world,
            &prog,
            PROCEDURAL_PROFILE,
            &Budget::default(),
        )
        .expect_err("n-ary comparison must not outlive its retired fallback");
        assert_eq!(
            error.message(),
            "native backward engine does not support Arithmetic; query refused because no fallback engine remains"
        );
    }

    // ── Native-first backward wiring ─────────────────────────────────────────────

    /// An IDB (recursive) program is resolved by the native physical core
    /// (`crate::physical::resolve_native`) — the sole backward path.
    /// The native magic-sets engine decides the binary positive fragment, so the
    /// transitive-ancestor answers come back native-authoritative. We assert the full
    /// answer set (a→b, a→c, a→d) to pin that the native path actually answered.
    #[test]
    fn dispatch_query_idb_resolved_by_native() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("b"), &p("parentOf"), &p("c"));
        store.insert_quad(W, &p("c"), &p("parentOf"), &p("d"));
        let world_nn = W.to_owned();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        // The native core MUST decide this binary positive query directly.
        let native = crate::physical::resolve_native(
            &WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap(),
            &world_nn,
            &prog,
            &budget,
        )
        .unwrap();
        assert!(
            matches!(native, crate::physical::NativeOutcome::Decided(_)),
            "native core must decide an IDB binary positive query: {native:?}"
        );

        // dispatch_query routes through the native core and returns the same answers.
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans = dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        let ys: BTreeSet<String> = ans.bindings.iter().map(|b| b["Y"].clone()).collect();
        let want: BTreeSet<String> = ["b", "c", "d"]
            .into_iter()
            .map(|x| format!("<{BASE}{x}>"))
            .collect();
        assert_eq!(ys, want, "native-resolved transitive ancestors: {ys:?}");
    }

    // ── N-ary predicate-as-data `triple/4` on the PARSED production surface ──────────

    /// The canonical predicate-as-data `triple/4` shape, driven end-to-end through the
    /// REAL production surface (`parse_query_program` → `dispatch_query`), DECIDES with
    /// non-empty correct bindings.
    ///
    /// This is the parser-driven twin of the hand-built-IR unit tests in
    /// `physical::magic_generic`: it proves the reserved bare `triple` relation now
    /// parses (previously `parse_query_program` rejected it with
    /// `cannot resolve predicate IRI "triple"`), routes through the arity-generic
    /// evaluator, and agrees with the generic-triple EDB's `push_fact("triple", …)`.
    #[test]
    fn dispatch_query_parsed_triple4_decides_nary_goal() {
        // A single <p1> edge x→y; the sub-property rule derives x <p2> y.
        let store = WorldStore::new();
        store.insert_quad(W, &p("x"), &p("p1"), &p("y"));
        let world_nn = W.to_owned();

        // The reserved bare `triple` relation with the property pinned in the DATA
        // position — the shape the binary store cannot express.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             triple(S, ex:p2, O, Wg) :- triple(S, ex:p1, O, Wg).\n\
             ?- triple(S, ex:p2, O, Wg).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // The parser carried the reserved relation VERBATIM (bare, un-resolved).
        assert_eq!(prog.goal.atoms[0].pred, "triple");
        assert_eq!(prog.goal.atoms[0].args.len(), 4, "arity 4 ⇒ n-ary path");

        let budget = Budget::default();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let ans = dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &budget).unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok);
        assert_eq!(
            ans.bindings.len(),
            1,
            "exactly one derived <p2> edge (non-empty): {ans:?}"
        );
        let b = &ans.bindings[0];
        assert_eq!(b["S"], format!("<{BASE}x>"), "subject binding");
        assert_eq!(b["O"], format!("<{BASE}y>"), "object binding");
        assert_eq!(b["Wg"], format!("<{W}>"), "world binding");
    }

    /// An n-ary shape the generic evaluator CANNOT serve — an arity-3 IDB over a binary
    /// EDB predicate (`edge`) that the generic-triple EDB never loads — must NOT be a
    /// silent-empty `Ok`. Native declares `Unsupported(NonBinaryAtom)` and production
    /// dispatch hard-fails with the same typed reason.
    #[test]
    fn dispatch_query_parsed_nary_over_binary_edb_not_silent_empty() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("edge"), &p("b"));
        store.insert_quad(W, &p("b"), &p("edge"), &p("c"));
        let world_nn = W.to_owned();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:tri(X, Y, Z) :- ex:edge(X, Y), ex:edge(Y, Z).\n\
             ?- ex:tri(ex:a, Y, Z).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        // Native MUST declare the gap (never a silent-empty `Decided`): the generic
        // evaluator cannot load the binary `edge` EDB, so it is `NonBinaryAtom`.
        let native = crate::physical::resolve_native(
            &WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap(),
            &world_nn,
            &prog,
            &budget,
        )
        .unwrap();
        assert!(
            matches!(
                native,
                crate::physical::NativeOutcome::Unsupported(
                    crate::physical::UnsupportedKind::NonBinaryAtom
                )
            ),
            "an un-servable n-ary shape must be a declared gap, not silent-empty: {native:?}"
        );

        // Production dispatch preserves the refusal instead of fabricating an empty answer.
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let error = dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &budget)
            .expect_err("unsupported n-ary shape must hard-fail");
        assert_eq!(
            error.message(),
            "native backward engine does not support NonBinaryAtom; query refused because no fallback engine remains"
        );
    }

    /// Cut is retained in the parser only to produce a stable retirement diagnostic.
    #[test]
    fn dispatch_query_rejects_retired_cut_under_procedural_profile() {
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("b"));
        store.insert_quad(W, &p("a"), &p("parentOf"), &p("c"));
        let world_nn = W.to_owned();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:parentOf(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let foreign = WorldFactSnapshot::from_world(&store, W, PROCEDURAL_PROFILE).unwrap();
        let error = dispatch_query(&foreign, &world_nn, &prog, PROCEDURAL_PROFILE, &budget)
            .expect_err("cut is retired even under the procedural builtin profile");
        assert!(error.message().contains("retired cut syntax"));
    }

    #[test]
    fn builtin_under_non_procedural_profile_is_rejected() {
        let (store, world) = list_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{RDF}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let result = dispatch_query(&foreign, world, &prog, HORN_PROFILE, &Budget::default());
        assert!(
            result.is_err(),
            "arithmetic builtin under PositiveHornProfile must be rejected"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.message().contains("builtin") && msg.message().contains(HORN_PROFILE),
            "error must name the offending profile: {msg:?}"
        );
    }

    // ── Budget: max_steps runs NATIVE (no demotion) ──────────────────────────────

    #[test]
    fn dispatch_budget_max_steps_runs_native_and_matches_reference() {
        // Build a chain: a→b→c→d (3 EDB parentOf edges), transitive-closure program.
        // The native engine now HONOURS `max_steps`: a step-budgeted query runs native
        // (no demotion) and stamps `Exhausted` at the ceiling. At a zero-step budget the
        // IDB goal derives nothing, so native and the reference oracle agree byte-for-byte
        // (both `Exhausted`, both empty); at an ample budget native completes with `Ok`.
        let store = WorldStore::new();
        let base = "https://example.org/";
        store.insert_quad(
            W,
            &format!("{base}a"),
            &format!("{base}parentOf"),
            &format!("{base}b"),
        );
        store.insert_quad(
            W,
            &format!("{base}b"),
            &format!("{base}parentOf"),
            &format!("{base}c"),
        );
        store.insert_quad(
            W,
            &format!("{base}c"),
            &format!("{base}parentOf"),
            &format!("{base}d"),
        );
        let world_nn = W.to_owned();
        let foreign = crate::seam::WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{base}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Unbudgeted dispatch goes through native and returns all 3 ancestors.
        let full =
            dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &Budget::default()).unwrap();
        assert_eq!(
            full.bindings.len(),
            3,
            "unbudgeted should yield all 3 ancestors"
        );
        assert_eq!(
            full.status,
            BudgetStatus::Ok,
            "unbudgeted status must be Ok"
        );

        // Zero-step budget: the native governor stops before the first ancestor
        // derivation → `Exhausted` with no bindings. The reference oracle also exhausts at
        // its first budget check (steps=0 >= 0) with no bindings. Because the IDB goal
        // derives nothing at budget 0, the two engines agree byte-for-byte here.
        let tight_budget = Budget {
            max_steps: Some(0),
            max_answers: None,
        };
        let dispatched =
            dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &tight_budget).unwrap();
        assert_eq!(
            dispatched.status,
            BudgetStatus::Exhausted,
            "a zero-step budget must be Exhausted (native honours it, no wrong Ok)"
        );
        assert!(
            dispatched.bindings.is_empty(),
            "a zero-step budget derives no ancestor ⇒ no bindings"
        );

        // At budget 0 the native path and the reference oracle agree byte-for-byte (both
        // Exhausted, both empty) — the completion boundary where the two engines coincide.
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            dispatched.status, reference.status,
            "native dispatch status must match the reference oracle at budget 0"
        );
        assert_eq!(
            dispatched.bindings, reference.bindings,
            "native dispatch bindings must match the reference oracle at budget 0 (both empty)"
        );
        // GAP A: the completion frontier crosses the PUBLIC `AnswerSet` boundary out of
        // `dispatch_query`. A zero-step cut leaves the single (magic-transformed) stratum
        // unsaturated — the caller reads `completed < total` to tell that from a complete
        // result.
        assert_eq!(
            dispatched.frontier.completed, 0,
            "a zero-step cut saturates no stratum: {:?}",
            dispatched.frontier
        );
        assert_eq!(
            dispatched.frontier.total, 1,
            "one stratum in the ancestor program: {:?}",
            dispatched.frontier
        );

        // An ample step budget completes on native with `Ok` and the full 3 answers —
        // native is NOT demoted for carrying a step budget.
        let ample = Budget {
            max_steps: Some(1_000_000),
            max_answers: None,
        };
        let completed = dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &ample).unwrap();
        assert_eq!(completed.status, BudgetStatus::Ok, "ample budget completes");
        assert_eq!(
            completed.bindings.len(),
            3,
            "an ample step budget yields all 3 ancestors on the native path"
        );
        // GAP A: an ample budget saturates the stratum, so the public frontier reports a
        // complete run and a positive committed-derivation count.
        assert_eq!(
            completed.frontier.completed, completed.frontier.total,
            "an ample budget saturates the whole program: {:?}",
            completed.frontier
        );
        assert!(
            completed.frontier.consumed_steps >= 1,
            "deriving the 3 ancestors commits at least one derivation: {:?}",
            completed.frontier
        );
    }

    #[test]
    fn dispatch_budget_max_steps_pure_edb_goal_completes_native() {
        // A single binary EDB atom classifies as `Dispatch::Fast`, but the native engine
        // now runs first: a pure-EDB goal is the settled stratum 0, so it derives NOTHING
        // and native returns the COMPLETE answer with `Ok` under ANY step budget, including
        // 0. This is the frontier win at the query surface — more correct than the
        // reference oracle, which counts the EDB lookup as a step and stamps `Exhausted`
        // at 0. The two engines intentionally DIVERGE on the pure-EDB path (different step
        // units), so no cross-engine status parity is asserted here.
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        );
        store.insert_quad(
            W,
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}c"),
        );
        let world_nn = W.to_owned();
        let foreign = WorldFactSnapshot::from_world(&store, W, HORN_PROFILE).unwrap();

        // Pure-EDB goal (no IDB predicate) is decided in native stratum 0.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // Unbudgeted native dispatch returns both children with status Ok.
        let full =
            dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &Budget::default()).unwrap();
        assert_eq!(
            full.bindings.len(),
            2,
            "unbudgeted native goal yields both children"
        );
        assert_eq!(full.status, BudgetStatus::Ok);

        // Zero-step budget: native decides the pure-EDB goal without any derivation, so it
        // returns the COMPLETE answer (both children) with `Ok` — no inference was needed.
        let tight_budget = Budget {
            max_steps: Some(0),
            max_answers: None,
        };
        let dispatched =
            dispatch_query(&foreign, &world_nn, &prog, HORN_PROFILE, &tight_budget).unwrap();
        assert_eq!(
            dispatched.status,
            BudgetStatus::Ok,
            "a pure-EDB goal needs no derivation ⇒ complete `Ok` under any step budget"
        );
        assert_eq!(
            dispatched.bindings.len(),
            2,
            "the complete pure-EDB answer (both children) is returned under budget 0"
        );

        // The reference oracle, by contrast, counts the EDB lookup as a step and stamps
        // Exhausted at budget 0 — the documented, intended divergence (different step
        // units). Native's complete answer is the more faithful verdict.
        let reference =
            crate::reference_resolver::resolve(&foreign, &world_nn, &prog, &tight_budget).unwrap();
        assert_eq!(
            reference.status,
            BudgetStatus::Exhausted,
            "the reference oracle exhausts at budget 0 — native intentionally diverges"
        );
    }
}
