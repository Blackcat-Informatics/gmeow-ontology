// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A throwaway case directory whose whole tree is removed when the `TmpCase`
/// is dropped — on success, on panic, and on early return — because it owns
/// the `tempfile::TempDir` its scratch root lives in.
struct TmpCase {
    /// The case directory itself: `<scratch root>/category/<tag>`. The
    /// `category/` segment is load-bearing — `run_case` derives the case id
    /// from the `<category>/<case>` path tail.
    dir: std::path::PathBuf,
    /// Owns the scratch root; dropping it removes `dir` with it.
    _root: tempfile::TempDir,
}
impl TmpCase {
    fn new(tag: &str) -> Self {
        let root = tempfile::tempdir().expect("create temp dir");
        let dir = root.path().join("category").join(tag);
        std::fs::create_dir_all(&dir).expect("mkdir case dir");
        Self { dir, _root: root }
    }
    fn write(&self, name: &str, body: &str) {
        std::fs::write(self.dir.join(name), body).expect("write case file");
    }
}

/// A `logic:ReasoningContract` authoring the forbidden probabilistic +
/// stable-model combination (RuleNoProbabilisticStableModel).
const UNSUPPORTED_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/g/> .\n\
        ex:C a logic:ReasoningContract ;\n\
            logic:modelSemantics logic:StableModelSemantics ;\n\
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .\n\
        ex:m a logic:ProbabilityModel .\n";

/// A clean, supported positive-Horn domain axiom (no contract, no error).
const SUPPORTED_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/g/> .\n\
        ex:Bird logic:subClassOf ex:Animal .\n";

#[test]
fn shared_query_world_keeps_goals_and_counterfactual_assumptions_independent() {
    let store = WorldStore::new();
    store.insert_quad("urn:test:world", "urn:test:s", "urn:test:p", "urn:test:o");
    let world = QueryWorld {
        store: &store,
        world: "urn:test:world",
        profile: "PositiveHornProfile",
        foreign: std::cell::OnceCell::new(),
    };
    let prefix = ":- prefix(ex, 'urn:test:').\n";
    let positive = format!("{prefix}?- ex:p(ex:s, Y).");
    let before = resolve_query("synthetic", &world, &positive, None, None).unwrap();
    assert_eq!(
        before["bindings"],
        serde_json::json!([{"Y":"<urn:test:o>"}])
    );
    let snapshot = world.foreign.get().unwrap() as *const _;
    let missing = resolve_query(
        "synthetic",
        &world,
        &format!("{prefix}?- ex:p(ex:absent, Y)."),
        None,
        None,
    )
    .unwrap();
    assert_eq!(missing["bindings"], serde_json::json!([]));
    let counterfactual = format!(
        "{prefix}:- counterfactual(ex:alternate, ex:world).\n:- assume(ex:p(ex:s, ex:alternateValue)).\n?- ex:p(ex:s, ex:alternateValue)."
    );
    let changed = resolve_query("synthetic", &world, &counterfactual, None, None).unwrap();
    assert_eq!(changed["bindings"], serde_json::json!([{}]));
    assert_eq!(store.worlds(), vec!["urn:test:world"]);
    assert_eq!(
        before,
        resolve_query("synthetic", &world, &positive, None, None).unwrap()
    );
    assert_eq!(snapshot, world.foreign.get().unwrap() as *const _);
}

#[test]
fn expect_unsupported_with_forbidden_combo_short_circuits_to_empty() {
    let case = TmpCase::new("ok");
    case.write("input.logic.ttl", UNSUPPORTED_TTL);
    case.write(
            "profile.json",
            r#"{"reasoning_contract":{"preset":"StableModelProfile"},"expect_unsupported":true,"mode":"native"}"#,
        );
    let out = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .expect("expect_unsupported case must pass");
    // The program was never evaluated: no quads, no answers, empty verdicts.
    assert!(out.materialized_nquads.is_empty());
    assert!(out.answers.is_empty());
    assert_eq!(out.verdicts, serde_json::json!({}));
    // The refusal is disclosed as `{unsupported}` (the legalization floor), never a
    // false `{exact}` that would hide it from a consumer reading `preservation`.
    assert_eq!(
        out.preservation,
        serialize::preservation_to_json(&PreservationClaim::unsupported()),
        "a refused expect_unsupported case must disclose {{unsupported}}, not {{exact}}"
    );
}

#[test]
fn expect_unsupported_but_supported_contract_hard_fails() {
    // The case CLAIMS unsupported but the engine accepts the contract: refuse.
    let case = TmpCase::new("claim");
    case.write("input.logic.ttl", SUPPORTED_TTL);
    case.write(
        "profile.json",
        r#"{"expect_unsupported":true,"mode":"native"}"#,
    );
    let err = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .unwrap_err();
    assert!(err.message().contains("expect_unsupported"), "{err}");
    assert!(err.message().contains("no UNSUPPORTED_CONTRACT"), "{err}");
}

#[test]
fn undeclared_compile_error_hard_fails() {
    // A forbidden combo WITHOUT expect_unsupported must surface as a hard
    // failure (the silent-run hole this firewall closes), never a silent evaluate.
    let case = TmpCase::new("silent");
    case.write("input.logic.ttl", UNSUPPORTED_TTL);
    case.write(
        "profile.json",
        r#"{"reasoning_contract":{"preset":"StableModelProfile"},"mode":"native"}"#,
    );
    let err = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .unwrap_err();
    assert!(err.message().contains("Severity::Error"), "{err}");
    assert!(err.message().contains("UNSUPPORTED_CONTRACT"), "{err}");
}

// ── profile.json `shipped_rules` ──────────────────────────────────────────
//
// These are the TEETH of the corpus's derivation claim. A case that re-typed a
// shipped rule inside its own `input.logic.ttl` stays green after the shipped rule
// is deleted, so it pins its own copy rather than what ships. Resolution through
// the module makes deletion red; the two refusals below are what make that true,
// and neither is reachable through the committed corpus (every committed case
// names rules that exist and declares none of them locally).

const SYNTHETIC_RULE: &str = "urn:test:library-rule";
const SYNTHETIC_LIBRARY: &str = r#"
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        <urn:test:library-rule> a logic:Rule ;
            logic:provenance <urn:test:library-rule> ;
            logic:head [ rdf:subject <urn:test:entry> ; rdf:predicate logic:entryLabel ; rdf:object logic:FrontierReadyAuthorized ] ;
            logic:body [ rdf:subject <urn:test:entry> ; rdf:predicate logic:entryAxisWitness ; rdf:object logic:StepReady ] .
    "#;

fn synthetic_library() -> gmeow_logic_compile::ir::LogicProgram {
    let (program, diagnostics) =
        gmeow_logic_compile::frontend::parse_logic_str(SYNTHETIC_LIBRARY, None).unwrap();
    assert!(first_error(&diagnostics).is_none(), "{diagnostics:?}");
    program
}

#[test]
fn supplied_rules_reach_the_case_projection() {
    let program = synthetic_library();
    let library = RuleLibrary::new(&program).unwrap();
    let case = TmpCase::new("library-ok");
    case.write("input.logic.ttl", SUPPORTED_TTL);
    case.write(
        "profile.json",
        &format!(r#"{{"mode":"native","shipped_rules":["{SYNTHETIC_RULE}"]}}"#),
    );
    case.write("input.nq", "");
    let out = run_case(&case.dir, &library, &crate::native_observation::read)
        .expect("synthetic library case");
    assert!(out.projections.text["datalog"].contains("entryLabel"));
}

#[test]
fn absent_library_rule_hard_fails() {
    let program = synthetic_library();
    let library = RuleLibrary::new(&program).unwrap();
    let case = TmpCase::new("library-missing");
    case.write("input.logic.ttl", SUPPORTED_TTL);
    case.write(
        "profile.json",
        r#"{"mode":"native","shipped_rules":["urn:test:deleted-rule"]}"#,
    );
    let err = run_case(&case.dir, &library, &crate::native_observation::read).unwrap_err();
    assert!(err.message().contains("urn:test:deleted-rule"), "{err}");
    assert!(
        err.message()
            .contains("not a logic:Rule in the shipped module"),
        "{err}"
    );
}

#[test]
fn locally_redeclaring_a_loaded_library_rule_hard_fails() {
    let program = synthetic_library();
    let library = RuleLibrary::new(&program).unwrap();
    let case = TmpCase::new("library-dup");
    case.write("input.logic.ttl", SYNTHETIC_LIBRARY);
    case.write(
        "profile.json",
        &format!(r#"{{"mode":"native","shipped_rules":["{SYNTHETIC_RULE}"]}}"#),
    );
    let err = run_case(&case.dir, &library, &crate::native_observation::read).unwrap_err();
    assert!(
        err.message().contains("redeclares the shipped rule"),
        "{err}"
    );
}

#[test]
fn ambiguous_library_identity_is_refused() {
    let mut program = synthetic_library();
    program.rules.push(program.rules[0].clone());
    assert!(
        RuleLibrary::new(&program)
            .err()
            .unwrap()
            .message()
            .contains("repeats named rule")
    );
}

// ── verdict_mode = consistency ────────────────────────────────────────────

const CONSISTENCY_PROFILE: &str = r#"{"verdict_mode":"consistency","mode":"native"}"#;
const W: &str = "https://gmeow.example/dl/world";

/// A world-scoped N-Quad EDB line in the gmeow ternary RDF shape.
fn q(s: &str, p: &str, o: &str) -> String {
    format!("<{s}> <{p}> <{o}> <{W}> .\n")
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const A: &str = "https://gmeow.example/dl/A";
const B: &str = "https://gmeow.example/dl/B";
const C: &str = "https://gmeow.example/dl/C";
const X: &str = "https://gmeow.example/dl/x";

#[test]
fn consistency_mode_populated_clash_is_inconsistent() {
    // x:A, A⊑B, A⊑C, B disjointWith C — x is forced into owl:Nothing, so the
    // world is INCONSISTENT (the external Theorem/Unsatisfiable branch). This
    // exercises the genuine native DL chase (no fake golden).
    let case = TmpCase::new("incon");
    case.write("profile.json", CONSISTENCY_PROFILE);
    let mut nq = String::new();
    nq.push_str(&q(X, RDF_TYPE, A));
    nq.push_str(&q(A, SUBCLASS, B));
    nq.push_str(&q(A, SUBCLASS, C));
    nq.push_str(&q(B, DISJOINT, C));
    case.write("input.nq", &nq);

    let out = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .expect("consistency case runs");
    assert_eq!(
        out.verdicts[W]["status"], "inconsistent",
        "populated clash must be inconsistent: {}",
        out.verdicts
    );
}

#[test]
fn consistency_mode_clash_free_is_consistent() {
    // x:A, A⊑B with no disjointness — no clash, so the world is CONSISTENT
    // (the external Satisfiable/CounterSatisfiable branch).
    let case = TmpCase::new("con");
    case.write("profile.json", CONSISTENCY_PROFILE);
    let mut nq = String::new();
    nq.push_str(&q(X, RDF_TYPE, A));
    nq.push_str(&q(A, SUBCLASS, B));
    case.write("input.nq", &nq);

    let out = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .expect("consistency case runs");
    assert_eq!(
        out.verdicts[W]["status"], "consistent",
        "clash-free world must be consistent: {}",
        out.verdicts
    );
}

#[test]
fn consistency_mode_requires_input_nq() {
    // No input.nq ⇒ hard fail (no silent skip / empty verdict).
    let case = TmpCase::new("noedb");
    case.write("profile.json", CONSISTENCY_PROFILE);
    let err = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .unwrap_err();
    assert!(err.message().contains("requires input.nq"), "{err}");
}

#[test]
fn consistency_execution_failure_cannot_become_a_gap_verdict() {
    let case = TmpCase::new("failed-execution");
    case.write("profile.json", CONSISTENCY_PROFILE);
    case.write("input.nq", &q(X, RDF_TYPE, A));
    let error = run_case(&case.dir, &RuleLibrary::default(), &|_, _| {
        Err(run_fail("native execution failed".to_owned()))
    })
    .unwrap_err();
    assert!(
        error.message().contains("native execution failed"),
        "{error}"
    );
}

#[test]
fn source_only_case_keeps_graph_ownership_without_compiling_a_rule_program() {
    let case = TmpCase::new("source-only");
    case.write("profile.json", r#"{"verdict_mode":"class-source-admission","source_admission_contract":"nativeClassExpressionListAdmission"}"#);
    case.write(
        "input.logic.ttl",
        "not a logic program; this operation must not read it",
    );
    case.write(
        "input.nq",
        &q(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            A,
        ),
    );
    let out = run_case(
        &case.dir,
        &RuleLibrary::default(),
        &crate::native_observation::read,
    )
    .unwrap();
    assert_eq!(
        out.verdicts["contract"],
        "nativeClassExpressionListAdmission"
    );
    assert_eq!(out.verdicts["selected_worlds"], serde_json::json!({}));
    assert_eq!(out.verdicts["source_worlds"][W]["assertions"], 1);
    assert!(out.materialized_nquads.is_empty());
    assert!(out.answers.is_empty());
    assert!(!out.verdicts.to_string().contains("consistent"));
}

#[test]
fn native_case_provider_cannot_substitute_an_operation_or_input() {
    let case = TmpCase::new("wrong-native-result");
    case.write("profile.json", r#"{"verdict_mode":"class-source-admission","source_admission_contract":"nativeClassExpressionListAdmission"}"#);
    case.write("input.nq", &q(X, RDF_TYPE, A));
    let wrong_operation = run_case(&case.dir, &RuleLibrary::default(), &|path, _| {
        crate::native_observation::read(path, NativeCaseOperation::Consistency)
    })
    .unwrap_err();
    assert!(wrong_operation.message().contains("different operation"));
    let wrong_identity = run_case(&case.dir, &RuleLibrary::default(), &|path, operation| {
        let NativeCaseObservation::ClassSourceAdmission(mut observed) =
            crate::native_observation::read(path, operation)?
        else {
            unreachable!()
        };
        observed.input_blake3[0] ^= 1;
        Ok(NativeCaseObservation::ClassSourceAdmission(observed))
    })
    .unwrap_err();
    assert!(wrong_identity.message().contains("another input identity"));
}
