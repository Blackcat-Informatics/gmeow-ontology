// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The `math:` subsort tower IRIs the order-sorted demonstrator tests below reason
/// over — literal references to the AUTHORED grounding vocabulary
/// (`slices/grounding/math/module.ttl`), not a second source of the tower itself (the
/// tower's edges are always supplied as `subsort_edges`, never hardcoded here).
const TEST_MATH_INTEGER: &str = "https://blackcatinformatics.ca/math/Integer";
const TEST_MATH_REAL: &str = "https://blackcatinformatics.ca/math/RealNumber";

// ── Compiled `logic:ReasoningProgram` → `FolProgram`, via `lower_reasoning_program` ──
//
// These parse a `logic:ReasoningProgram` from a Turtle fixture (the SAME authoring
// vocabulary `crates/logic-compile`'s own frontend tests exercise), compile it to
// `ReasoningProgramIr`, then run it through `evaluate_reasoning_programs` — the
// SOLE production path for goal-directed programs.

/// `add(zero,Y,Y). add(s(X),Y,s(Z)) :- add(X,Y,Z).` with goal
/// `?- add(s(s(zero)),s(zero),R)`, authored as a `logic:ReasoningProgram`.
const PEANO_ADD_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:peanoAdd a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:add ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termApplication ex:ssZero ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:sZero ] ,\n\
                               [ logic:termIndex 2 ; logic:termVariable \"R\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:add ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:zero ] ,\n\
                               [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                               [ logic:termIndex 2 ; logic:termVariable \"Y\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:add ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                                   [ logic:termIndex 2 ; logic:termVariable \"Z\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:add ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sX ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                                   [ logic:termIndex 2 ; logic:termApplication ex:sZ ]\n\
                ]\n\
            ] .\n\
        ex:sZero a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:zero ] .\n\
        ex:ssZero a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sZero ] .\n\
        ex:sX a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] .\n\
        ex:sZ a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"Z\" ] .\n\
    ";

#[test]
fn compiled_peano_add_reasoning_program_resolves_and_proof_checks() {
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(
        prog.reasoning_programs.len(),
        1,
        "exactly one logic:ReasoningProgram parsed"
    );

    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate the compiled reasoning program");
    assert_eq!(evals.len(), 1);
    let peano = &evals[0];
    assert_eq!(peano.status, "ok");
    assert_eq!(peano.answers.len(), 1, "2 + 1 has exactly one answer");
    let ans = &peano.answers[0];
    // Every constant/function-symbol in the compiled path is a REAL RDF IRI (rendered in
    // full) — so the expected surfaces are built from the same `ex:` namespace the
    // fixture authors its symbols under.
    const EX: &str = "https://example.org/goal-directed-test/";
    let zero = format!("{EX}zero");
    let s = |inner: &str| format!("{EX}s({inner})");
    let s_zero = s(&zero); // s(zero)
    let ss_zero = s(&s_zero); // s(s(zero))
    let sss_zero = s(&ss_zero); // s(s(s(zero))) = R
    assert_eq!(
        ans.bindings.get("R").map(String::as_str),
        Some(sss_zero.as_str()),
        "2 + 1 = 3 in Peano successors"
    );
    // PurRDF's `render` joins application arguments with `", "` (space after the comma).
    assert_eq!(
        ans.atom,
        format!("{EX}add({ss_zero}, {s_zero}, {sss_zero})")
    );
    assert!(ans.proof_checks, "the compiled answer is proof-checked");
    assert!(
        ans.derivation_iri.starts_with("https://"),
        "the answer carries a content-addressed derivation IRI: {}",
        ans.derivation_iri
    );

    // Two independent evaluations of the SAME parsed program mint the SAME derivation
    // IRI — content-addressing (PurRDF's `derivation_id` proof digest), not mint-order.
    let evals2 =
        evaluate_reasoning_programs(&prog.reasoning_programs, &[]).expect("second evaluation");
    assert_eq!(
        evals2[0].answers[0].derivation_iri, ans.derivation_iri,
        "the compiled program's rule identity is content-addressed, not interning-order \
             dependent"
    );
}

/// `member(X,cons(X,T)). member(X,cons(H,T)) :- member(X,T).` with goal
/// `?- member(M,cons(a,cons(b,cons(c,nil))))`. The base clause and the recursive
/// clause's antecedent/consequent deliberately REUSE the variable names `X`/`T` — this
/// is the exact scenario that proves per-clause [`VarScope`] freshness: if the compiler
/// accidentally shared one metavariable per NAME across clauses (instead of per NAME
/// WITHIN one clause), this program would resolve incorrectly.
const MEMBER_CONS_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:memberCons a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:member ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"M\" ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:list1 ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:member ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:consXT ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:member ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"T\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:member ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termApplication ex:consHT ]\n\
                ]\n\
            ] .\n\
        ex:consXT a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                           [ logic:termIndex 1 ; logic:termVariable \"T\" ] .\n\
        ex:consHT a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"H\" ] ,\n\
                           [ logic:termIndex 1 ; logic:termVariable \"T\" ] .\n\
        ex:list1 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                           [ logic:termIndex 1 ; logic:termApplication ex:list2 ] .\n\
        ex:list2 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:b ] ,\n\
                           [ logic:termIndex 1 ; logic:termApplication ex:list3 ] .\n\
        ex:list3 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:c ] ,\n\
                           [ logic:termIndex 1 ; logic:termIri ex:nil ] .\n\
    ";

#[test]
fn compiled_member_cons_reasoning_program_enumerates_with_reused_variable_names() {
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(MEMBER_CONS_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);

    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate the compiled reasoning program");
    assert_eq!(evals.len(), 1);
    let member = &evals[0];
    assert_eq!(member.status, "ok");
    let mut bound: Vec<String> = member
        .answers
        .iter()
        .map(|a| a.bindings["M"].clone())
        .collect();
    bound.sort();
    // Every constant is a REAL RDF IRI (rendered in full) — see the peano-add test above.
    const EX: &str = "https://example.org/goal-directed-test/";
    assert_eq!(
        bound,
        vec![format!("{EX}a"), format!("{EX}b"), format!("{EX}c"),],
        "the SAME variable names (X, T) reused across the base and recursive clauses must \
             NOT collide across clause scopes: {bound:?}"
    );
    for ans in &member.answers {
        assert!(ans.proof_checks, "every member answer is proof-checked");
    }
}

// ── Compiled math-subsort + incomparable control, with seeded `term_sorts` ──
//
// `ex:one` is an ordinary domain individual, typed `math:Integer` by a plain
// `rdf:type` triple (never `logic:` structural vocabulary, so the stage's L3 fold drops
// it — `ReasoningProgramIr::constant_sorts` is what preserves it). Program
// A's query variable is declared `math:RealNumber`; program B's (the control) is
// declared the INCOMPARABLE `math:Set`. Both share the SAME fact `p(one)` and the SAME
// constant `ex:one`, so the ONLY difference between A's answer and B's empty answer set
// is the order-sorted lattice discriminating `Integer ⊑ RealNumber` from `Integer ⋢
// Set` — proving `SortContext::term_sorts` (not just `meta_sorts`) is actually seeded
// from the compiled IR's `constant_sorts`, not left empty (which would make every
// constant order-sort top and erase the F-4 differential).
const MATH_SUBSORT_REASONING_PROGRAMS_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        @prefix math: <https://blackcatinformatics.ca/math/> .\n\
        \n\
        ex:one a math:Integer .\n\
        \n\
        ex:subsortPositive a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort math:RealNumber ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:one ]\n\
            ] .\n\
        \n\
        ex:subsortControl a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort math:Set ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:one ]\n\
            ] .\n\
    ";

#[test]
fn compiled_math_subsort_reasoning_program_seeds_term_sorts_from_constant_sorts() {
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(MATH_SUBSORT_REASONING_PROGRAMS_TTL, None)
            .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 2);

    const EX: &str = "https://example.org/goal-directed-test/";
    let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];

    // Under the ℤ⊑ℝ reasoned edge: program A (RealNumber-sorted X) resolves to exactly
    // the Integer constant `one`; program B (the Set-sorted control) resolves to NOTHING
    // — status ok, empty answer set, never an error.
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &subsort_edges)
        .expect("evaluate the compiled reasoning programs");
    assert_eq!(evals.len(), 2);
    let positive = evals
        .iter()
        .find(|e| e.name == "subsortPositive")
        .expect("the positive program is present");
    assert_eq!(positive.status, "ok");
    assert_eq!(
        positive.answers.len(),
        1,
        "an Integer constant binds a RealNumber variable (ℤ ⊑ ℝ) under the reasoned \
             edge: {:?}",
        positive.answers
    );
    let ans = &positive.answers[0];
    assert_eq!(
        ans.bindings.get("X").map(String::as_str),
        Some(format!("{EX}one").as_str()),
        "the subsort-unified answer binds X = ex:one"
    );
    assert_eq!(ans.atom, format!("{EX}p({EX}one)"));
    assert!(ans.proof_checks, "the subsort answer is proof-checked");

    let control = evals
        .iter()
        .find(|e| e.name == "subsortControl")
        .expect("the control program is present");
    assert_eq!(control.status, "ok");
    assert!(
        control.answers.is_empty(),
        "an Integer constant does NOT bind an incomparable-sort (Set) variable, status \
             ok, empty answer set: {:?}",
        control.answers
    );

    // M5/F-4: with EMPTY subsort_edges, program A ALSO returns ZERO answers — the
    // Integer/RealNumber unification comes from the REASONED edge, never a hardcoded
    // tower baked into the compiler.
    let evals_no_edges = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate with no subsort edges");
    let positive_no_edges = evals_no_edges
        .iter()
        .find(|e| e.name == "subsortPositive")
        .expect("the positive program is present");
    assert_eq!(positive_no_edges.status, "ok");
    assert!(
        positive_no_edges.answers.is_empty(),
        "without the reasoned ℤ⊑ℝ edge, an Integer constant does NOT bind a RealNumber \
             variable — order-sortedness comes from subsort_edges, not a hardcoded tower: {:?}",
        positive_no_edges.answers
    );
}

/// Two STRUCTURALLY-IDENTICAL clauses `p(X)`, one declaring `X:Nat` and one `X:Real`. They
/// share a `Formula::content_key` (a `logic:variableSort` is harvested separately, not part
/// of the clause AST), so ONLY the per-clause occurrence-index disambiguation keeps their
/// scopes distinct. Each clause lowers under its OWN sort; one scope's declarations never
/// bleed into the other clause's `X`.
const DUP_CLAUSES_DISTINCT_SORTS_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:dupSorts a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"R\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort ex:Nat ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort ex:Real ]\n\
            ] .\n\
    ";

#[test]
fn identical_clauses_with_distinct_sorts_each_lower_under_their_own_sort() {
    const EX: &str = "https://example.org/goal-directed-test/";
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(DUP_CLAUSES_DISTINCT_SORTS_TTL, None)
            .expect("parse succeeds");
    // The two identical clauses are accepted rather than falsely rejected by
    // `ReasoningProgramIr::new`'s intra-scope conflict guard.
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "two identical clauses with different variable sorts must be accepted: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let rp = &prog.reasoning_programs[0];
    assert_eq!(rp.clauses.len(), 2, "both identical clauses are retained");

    // Lower directly (the SOLE production path) and inspect the per-metavariable sort map.
    let built = lower_reasoning_program(rp, &[]).expect("lower the compiled program");
    let BuiltDemonstrator {
        mut dag, program, ..
    } = built;
    // Exactly two sorted metavariables: clause-0's X (Nat) and clause-1's X (Real). The
    // query variable R carries no sort, so it is absent. If the scopes had collided (the
    // bug), a single clause scope would have carried BOTH sorts and lowering would apply an
    // ambiguous sort — here each clause's X gets exactly its own.
    assert_eq!(
        program.meta_sorts.len(),
        2,
        "each identical clause's X is a distinct sorted metavariable: {:?}",
        program.meta_sorts
    );
    // Hash-consing: re-interning a sort IRI returns the SAME NodeId lowering used, so the
    // two authored sorts must BOTH appear among the metavariable sort tags.
    let nat = leaf(&mut dag, &format!("{EX}Nat"));
    let real = leaf(&mut dag, &format!("{EX}Real"));
    let sort_nodes: std::collections::HashSet<NodeId> =
        program.meta_sorts.values().copied().collect();
    assert!(
        sort_nodes.contains(&nat),
        "one identical clause's X lowers under ex:Nat: {:?}",
        program.meta_sorts
    );
    assert!(
        sort_nodes.contains(&real),
        "the other identical clause's X lowers under ex:Real: {:?}",
        program.meta_sorts
    );
}

// ── T3: cross-engine (backward vs. forward) fixpoint-agreement oracle ───────────────
//
// `ex:reachability`: `edge(a,b). edge(b,c). reach(X,Y):-edge(X,Y). reach(X,Z):-edge(X,Y),
// reach(Y,Z).` with goal `?- reach(a,W)`. Definite, function-free, and every atom binary
// — squarely inside `is_definite_function_free_binary`'s fragment, so
// `evaluate_reasoning_programs` runs the T3 oracle over it.

const REACHABILITY_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:reachability a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:reach ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termVariable \"W\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:edge ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termIri ex:b ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:edge ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:b ] ,\n\
                               [ logic:termIndex 1 ; logic:termIri ex:c ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:edge ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:reach ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:and [ a logic:Formula ;\n\
                            logic:relation ex:edge ;\n\
                            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                           [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                        ] ,\n\
                        [ a logic:Formula ;\n\
                            logic:relation ex:reach ;\n\
                            logic:argument [ logic:termIndex 0 ; logic:termVariable \"Y\" ] ,\n\
                                           [ logic:termIndex 1 ; logic:termVariable \"Z\" ]\n\
                        ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:reach ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Z\" ]\n\
                ]\n\
            ] .\n\
    ";

#[test]
fn reachability_program_is_gated_into_the_oracle_fragment_and_passes_it() {
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(REACHABILITY_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    assert!(
        is_definite_function_free_binary(&prog.reasoning_programs[0]),
        "reachability is definite, function-free, and every atom is binary — squarely \
             inside the T3 oracle's fragment"
    );

    // `evaluate_reasoning_programs` runs the oracle inline; a mismatch would HARD-FAIL
    // here, so success itself proves backward == forward for this program.
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate + cross-check the reachability program");
    assert_eq!(evals.len(), 1);
    let reach = &evals[0];
    assert_eq!(reach.status, "ok");
    const EX: &str = "https://example.org/goal-directed-test/";
    let mut bound: Vec<String> = reach
        .answers
        .iter()
        .map(|a| a.bindings["W"].clone())
        .collect();
    bound.sort();
    assert_eq!(
        bound,
        vec![format!("{EX}b"), format!("{EX}c")],
        "backward resolution of reach(a,W) enumerates W ∈ {{b,c}}"
    );
}

#[test]
fn programs_with_function_symbols_negation_or_non_binary_atoms_are_excluded_from_the_oracle() {
    // Peano add carries `s(...)` function-term applications: NOT function-free.
    let (peano, _) =
        gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    assert!(
        !is_definite_function_free_binary(&peano.reasoning_programs[0]),
        "peano-add's s(...) function terms exclude it from the oracle's fragment"
    );

    // Math-subsort's `p(X)` is unary: NOT binary.
    let (subsort, _) =
        gmeow_logic_compile::frontend::parse_logic_str(MATH_SUBSORT_REASONING_PROGRAMS_TTL, None)
            .expect("parse succeeds");
    for program in &subsort.reasoning_programs {
        assert!(
            !is_definite_function_free_binary(program),
            "{}'s unary p(X) atom excludes it from the oracle's binary-atom fragment",
            program.iri
        );
    }
}

#[test]
fn the_cross_engine_oracle_hard_fails_a_deliberately_wrong_answer_set() {
    // Proves the oracle is not vacuous: corrupt a REAL, oracle-passing evaluation's
    // answer set and confirm `cross_check_forward_agreement` actually detects and
    // rejects the mismatch, rather than trivially succeeding for any input.
    let (prog, _) =
        gmeow_logic_compile::frontend::parse_logic_str(REACHABILITY_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    let program = &prog.reasoning_programs[0];
    let evals = evaluate_reasoning_programs(std::slice::from_ref(program), &[])
        .expect("the real program passes the oracle");
    let real_eval = &evals[0];
    assert!(
        !real_eval.answers.is_empty(),
        "the real program has at least one answer to corrupt"
    );

    let mut wrong = real_eval.clone();
    wrong.answers.pop();
    wrong.answers.push(GoalDirectedAnswer {
        atom: "https://example.org/goal-directed-test/reach(https://example.org/\
                   goal-directed-test/a,https://example.org/goal-directed-test/nonexistent)"
            .to_owned(),
        bindings: BTreeMap::new(),
        derivation_iri: "https://blackcatinformatics.ca/gmeow/derivation/bogus".to_owned(),
        proof_checks: true,
    });

    let result = cross_check_forward_agreement(program, &wrong);
    assert!(
        result.is_err(),
        "the oracle must HARD-FAIL when the (corrupted) backward answer set disagrees \
             with the forward least model — proving the check is not vacuous"
    );
}

// ── The authored/compiled path is the SOLE source — every demonstrator
// behavior below is asserted directly against `evaluate_reasoning_programs` over a
// parsed `logic:ReasoningProgram` fixture, never a hand-interned Rust-constant corpus.

#[test]
fn compiled_peano_add_projection_carries_answer_atom_and_derivation_iri() {
    let (prog, _) =
        gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate the compiled reasoning program");
    let nt = project_goal_directed(&evals);
    assert!(
        nt.contains("GoalDirectedQuery"),
        "the projection types the query"
    );
    const EX: &str = "https://example.org/goal-directed-test/";
    // PurRDF's `render` joins application arguments with `", "`.
    let expected_atom =
        format!("{EX}add({EX}s({EX}s({EX}zero)), {EX}s({EX}zero), {EX}s({EX}s({EX}s({EX}zero))))");
    assert!(
        nt.contains(&expected_atom),
        "the projection carries the ground answer atom:\n{nt}"
    );
    assert!(
        nt.contains("goalDirectedDerivation"),
        "the projection carries the proof-derivation IRI predicate"
    );
    // Deterministic: a second projection of the SAME evals is byte-identical.
    let nt2 = project_goal_directed(&evals);
    assert_eq!(nt, nt2, "the projection is byte-stable");
}

// ── U2: the authored PROGRAM STRUCTURE itself is projected, not only its answers ────

#[test]
fn compiled_peano_add_projection_carries_the_authored_program_structure_and_is_byte_stable_across_runs()
 {
    let (prog, _) =
        gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate the compiled reasoning program");
    let nt = project_goal_directed(&evals);
    assert!(
        nt.contains("GoalDirectedProgram"),
        "the projection types the authored program node:\n{nt}"
    );
    assert!(
        nt.contains("hasGoalDirectedProgram"),
        "the query node links to its authored program:\n{nt}"
    );
    assert!(
        nt.contains("goalDirectedClause"),
        "the projection carries the authored clauses:\n{nt}"
    );
    assert!(
        nt.contains("goalDirectedProgramQuery"),
        "the projection carries the authored program's query:\n{nt}"
    );
    // The Peano program's own fact clause and the recursive rule's body both surface as
    // rendered `goalDirectedClause` literals.
    const EX: &str = "https://example.org/goal-directed-test/";
    assert!(
        nt.contains(&format!("{EX}add({EX}zero,")),
        "the Peano fact clause add(zero,Y,Y). is projected:\n{nt}"
    );
    assert!(
        nt.contains(&format!(" :- {EX}add(")),
        "the Peano recursive rule's antecedent is projected:\n{nt}"
    );
    // The query linkage: the peano-add query node's `hasGoalDirectedProgram` object is a
    // `GoalDirectedProgram` individual carrying that SAME program's `goalDirectedProgramQuery`
    // literal, equal to the query node's own `goalDirectedGoal` literal (the SAME `render`
    // surface, reused rather than re-derived).
    let peano = &evals[0];
    let expected_query_triple = format!("<{GMEOW}goalDirectedProgramQuery> \"{}\" .", peano.goal);
    assert!(
        nt.lines().any(|l| l.ends_with(&expected_query_triple)),
        "the program node's goalDirectedProgramQuery literal equals the query node's own \
             rendered goal:\n{nt}"
    );

    // Byte-stability ACROSS two independent evaluations (not merely two projections of
    // the same `evals`): content-addressed, never interning/mint-order dependent.
    let evals2 =
        evaluate_reasoning_programs(&prog.reasoning_programs, &[]).expect("second evaluation");
    let nt2 = project_goal_directed(&evals2);
    assert_eq!(
        nt, nt2,
        "the authored-program projection is byte-identical across independent evaluations"
    );
}

// ── Positive structured demonstrator: member over cons/nil ──────────────────────────

#[test]
fn compiled_member_cons_projection_carries_structured_answers_and_derivation() {
    let (prog, _) =
        gmeow_logic_compile::frontend::parse_logic_str(MEMBER_CONS_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate the compiled reasoning program");
    let member = &evals[0];
    assert_eq!(member.status, "ok");
    const EX: &str = "https://example.org/goal-directed-test/";
    // Each answer is proof-checked and carries a content-addressed derivation IRI over the
    // cons spine (a genuine structured atom, not a flat binary one).
    for ans in &member.answers {
        assert!(ans.proof_checks, "every member answer is proof-checked");
        assert!(
            ans.derivation_iri
                .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
            "the answer carries a content-addressed derivation IRI: {}",
            ans.derivation_iri
        );
        assert!(
            ans.atom.starts_with(&format!("{EX}member("))
                && ans.atom.contains(&format!("{EX}cons(")),
            "the answer atom is a structured cons-list membership: {}",
            ans.atom
        );
    }

    let nt = project_goal_directed(&evals);
    // PurRDF's `render` joins application arguments with `", "`.
    let expected_atom =
        format!("{EX}member({EX}a, {EX}cons({EX}a, {EX}cons({EX}b, {EX}cons({EX}c, {EX}nil))))");
    assert!(
        nt.contains(&expected_atom),
        "the projection carries a structured member answer atom:\n{nt}"
    );
    // Every member answer surfaces a derivation IRI triple.
    assert!(
        nt.contains(
            "<https://blackcatinformatics.ca/gmeow/goalDirectedDerivation> \
                 <https://blackcatinformatics.ca/gmeow/derivation/"
        ),
        "the projection carries the member answers' derivation IRIs:\n{nt}"
    );
}

// ── WFS negation demonstrator: three-valued verdicts including undefined ─────────────
//
// No authored-path test above exercises `logic:verdictProbe`s, so this test is what
// proves the compiled path carries the three-valued SLG-WFS verdict surface end to end.
// This parses the REAL committed corpus
// (`slices/grounding/logic/examples/reasoning-programs.ttl`) via [`authored_reasoning_programs`].
// `ex:winWfs`'s rule body `win(X) :- move(X,Y), not win(Y)` is authored as a `logic:and` of a
// positive and a negation-as-failure literal, and `logic:and`/`logic:or` carry no
// `logic:conjunctIndex` analogous to `logic:argument`'s `logic:termIndex`. Formerly the
// lowered conjunct order was therefore only as stable as the frontend's per-document
// blank-node interning. [`lower_body`] now FLATTENS and SORTS a conjunction by
// `Formula::content_key` (the SAME key `Formula::And`'s order-normalized identity uses)
// before lowering, so the conjunct order — and hence the clause text, metavariable
// numbering, and content-addressed IRIs — is DETERMINISTIC regardless of RDF object order:
// the positive `move(...)` literal (`content_key` prefix `ATOM…`) always precedes the
// negation (`NOT…`). This test now asserts that stable order.

/// Parse the REAL authored demonstrator corpus
/// (`slices/grounding/logic/examples/reasoning-programs.ttl`) through the exact same
/// production frontend entry point `gmeow-pipeline`'s `stage-compile-logic` uses.
fn authored_reasoning_programs() -> Vec<ReasoningProgramIr> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("slices/grounding/logic/examples/reasoning-programs.ttl");
    let (prog, diags) = gmeow_logic_compile::frontend::parse_logic_path(&path, None)
        .expect("parse the authored reasoning-programs cell");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert!(
        !prog.reasoning_programs.is_empty(),
        "the authored cell carries at least one logic:ReasoningProgram"
    );
    prog.reasoning_programs
}

#[test]
fn compiled_win_wfs_reasoning_program_carries_three_valued_verdicts() {
    let programs = authored_reasoning_programs();
    let win_program = programs
        .iter()
        .find(|p| p.iri.ends_with("winWfs"))
        .cloned()
        .expect("ex:winWfs is authored in the reasoning-programs cell");

    let evals = evaluate_reasoning_programs(std::slice::from_ref(&win_program), &[])
        .expect("evaluate the compiled win-wfs reasoning program");
    assert_eq!(evals.len(), 1);
    let win = &evals[0];
    assert_eq!(win.status, "ok");

    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
    // The only well-founded-TRUE goal answer is win(c).
    let ws: Vec<String> = win
        .answers
        .iter()
        .map(|a| a.bindings["W"].clone())
        .collect();
    assert_eq!(
        ws,
        vec![format!("{EX}c")],
        "only c is a founded win: {ws:?}"
    );
    for ans in &win.answers {
        assert!(ans.proof_checks, "the win answer is proof-checked");
    }

    let atom_of = |local: &str| format!("{EX}win({EX}{local})");
    let verdict_of = |atom: &str| {
        win.verdicts
            .iter()
            .find(|v| v.atom == atom)
            .unwrap_or_else(|| panic!("verdict for {atom} present: {:?}", win.verdicts))
            .verdict
            .as_str()
    };
    // The a⇄b negative loop is well-founded UNDEFINED (never a fabricated true/false).
    assert_eq!(
        verdict_of(&atom_of("a")),
        "undefined",
        "even cycle ⇒ undefined"
    );
    assert_eq!(
        verdict_of(&atom_of("b")),
        "undefined",
        "even cycle ⇒ undefined"
    );
    // The founded positions are a definite true/false.
    assert_eq!(verdict_of(&atom_of("c")), "true", "move to lost d ⇒ won");
    assert_eq!(verdict_of(&atom_of("d")), "false", "no move ⇒ lost");

    // The rule clause `win(X) :- move(X,Y), not win(Y)` lowers to a deterministic conjunct
    // order: the positive `move` literal (content_key `ATOM…`) always precedes the
    // negation-as-failure `not win` literal (content_key `NOT…`), regardless of the authored
    // RDF `logic:and` object order (which carries no index). Assert the order structurally
    // (robust to the exact `?n` metavariable numbering).
    let rule = win
        .clauses
        .iter()
        .find(|c| c.contains(" :- "))
        .expect("the win rule clause is projected");
    let body = rule.split(" :- ").nth(1).expect("the rule has a body");
    let move_pos = body
        .find(&format!("{EX}move("))
        .expect("the positive move literal is present in the body");
    let not_pos = body
        .find(&format!("not {EX}win("))
        .expect("the negation-as-failure win literal is present in the body");
    assert!(
        move_pos < not_pos,
        "the positive move literal must precede the not-win literal (canonical content_key \
             conjunct order, interning-independent): {rule}"
    );

    // The distinctive SLG-WFS surface projects: an undefined verdict AND both founded
    // verdicts.
    let nt = project_goal_directed(&evals);
    // G15: `atom` and `verdict` must be asserted of the SAME verdict subject — checking
    // "some line has this atom" and (independently) "some OTHER line has this verdict"
    // would false-pass a cross-subject mismatch (e.g. win(a)'s atom line paired with
    // win(c)'s "true" verdict line, even though win(a) is actually undefined). Each
    // N-Triples line is `<subject> <predicate> "object" .`; extract the bracketed
    // subject token from the `goalDirectedVerdictAtom` line naming `atom`, then require
    // a DIFFERENT line with that EXACT SAME subject to carry `goalDirectedVerdict`
    // "verdict".
    let has_verdict = |atom: &str, verdict: &str| {
        nt.lines()
            .filter(|l| l.contains("goalDirectedVerdictAtom") && l.contains(atom))
            .any(|l| {
                let subject = l.split_whitespace().next().unwrap_or("");
                !subject.is_empty()
                    && nt.lines().any(|v| {
                        v.starts_with(subject)
                            && v.contains("goalDirectedVerdict>")
                            && v.contains(&format!("\"{verdict}\""))
                    })
            })
    };
    assert!(
        nt.contains("\"undefined\""),
        "the projection carries at least one undefined WFS verdict (SLG-WFS is non-dark):\n{nt}"
    );
    assert!(
        has_verdict(&atom_of("a"), "undefined"),
        "win(a) is serialized as undefined:\n{nt}"
    );
    assert!(
        has_verdict(&atom_of("c"), "true"),
        "win(c) is serialized as a founded true:\n{nt}"
    );
    assert!(
        has_verdict(&atom_of("d"), "false"),
        "win(d) is serialized as a founded false:\n{nt}"
    );
    // G15 regression: win(a) is undefined and win(c) is a DIFFERENT subject's "true" —
    // `has_verdict` must NOT cross-match win(a)'s atom line against win(c)'s verdict
    // line just because both substrings appear somewhere in the projection.
    assert!(
        !has_verdict(&atom_of("a"), "true"),
        "win(a) must not false-pass as \"true\" via a DIFFERENT subject's verdict line:\n{nt}"
    );
    assert!(
        !has_verdict(&atom_of("c"), "undefined"),
        "win(c) must not false-pass as \"undefined\" via a DIFFERENT subject's verdict \
             line:\n{nt}"
    );

    // Byte-stability across two independent evaluations.
    let evals2 = evaluate_reasoning_programs(std::slice::from_ref(&win_program), &[])
        .expect("second evaluation");
    let nt2 = project_goal_directed(&evals2);
    assert_eq!(
        nt, nt2,
        "the win-wfs projection is byte-identical across independent evaluations"
    );
}

// ── Math sub-sort demonstrator (order-sorted ℤ ⊑ ℝ) + incomparable control ───────────

#[test]
fn compiled_math_subsort_projection_carries_the_subsort_unified_answer() {
    let (prog, _) =
        gmeow_logic_compile::frontend::parse_logic_str(MATH_SUBSORT_REASONING_PROGRAMS_TTL, None)
            .expect("parse succeeds");
    let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &subsort_edges)
        .expect("evaluate the compiled reasoning programs");
    let nt = project_goal_directed(&evals);
    const EX: &str = "https://example.org/goal-directed-test/";
    assert!(
        nt.contains(&format!(
            "<https://blackcatinformatics.ca/gmeow/goalDirectedAtom> \"{EX}p({EX}one)\""
        )),
        "the projection carries the subsort-unified answer atom p(one):\n{nt}"
    );
    assert!(
        nt.contains(&format!("\"X = {EX}one\"")),
        "the projection carries the subsort-unified binding X = one:\n{nt}"
    );
}

// ── Every compiled reasoning-program answer proof-checks; whole projection is
// non-vacuous and byte-stable — the authored-path equivalent of the retired
// `evaluate_shipped_demonstrators()` corpus sweep, over the SAME six programs
// `slices/grounding/logic/examples/reasoning-programs.ttl` ships. ──────────────────────

#[test]
fn every_compiled_reasoning_program_answer_proof_checks_and_projection_is_deterministic() {
    // The REAL committed corpus — all six authored programs (peano-add, member-cons, the
    // math-subsort positive/control pair, reachability, win-wfs) in ONE parse — the
    // authored-path equivalent of the retired `evaluate_shipped_demonstrators()` corpus
    // sweep.
    let programs = authored_reasoning_programs();
    let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];
    let evals = evaluate_reasoning_programs(&programs, &subsort_edges)
        .expect("evaluate the merged compiled corpus");
    let mut total_answers = 0usize;
    for eval in &evals {
        for ans in &eval.answers {
            assert!(
                ans.proof_checks,
                "demonstrator {} answer {} must be proof-checked",
                eval.name, ans.atom
            );
            total_answers += 1;
        }
    }
    assert!(
        total_answers >= 5,
        "the corpus ships several proof-checked answers (peano + 3 members + subsort + \
             reachability + win-wfs): got {total_answers}"
    );

    // Two evaluations produce byte-identical serialization (no hash-iteration order).
    let nt_first = project_goal_directed(&evals);
    assert!(!nt_first.is_empty(), "the projection is non-empty");
    let evals2 = evaluate_reasoning_programs(&programs, &subsort_edges).expect("second evaluation");
    let nt_second = project_goal_directed(&evals2);
    assert_eq!(
        nt_first, nt_second,
        "two independent evaluations serialize byte-identically (deterministic)"
    );
}

// ── G12: content-addressed answer/verdict IRIs, order-independent ──────────────────

#[test]
fn answer_and_verdict_iris_are_content_addressed_not_positional() {
    // G12 regression: `answer_iri`/`verdict_iri` must be content-addressed (folding the
    // demonstrator name, atom, bindings/verdict, and derivation) rather than a
    // positional `/answer/{idx}`/`/verdict/{idx}`. Build the SAME two answers (and two
    // verdicts) in two DIFFERENT vector orders — simulating what a different internal
    // evaluation order would hand `project_goal_directed` — and confirm the projected
    // N-Triples are BYTE-IDENTICAL either way. Under a positional scheme this would
    // fail: answer 0 in one order is answer 1 in the other, so `/answer/0` would be
    // minted for a DIFFERENT answer's triples depending on evaluation order.
    let make_eval = |answers: Vec<GoalDirectedAnswer>, verdicts: Vec<GoalDirectedVerdict>| {
        GoalDirectedEvaluation {
            iri: "https://example.org/goal-directed-test/order-probe".to_owned(),
            name: "order-probe".to_owned(),
            description: "G12 order-independence probe".to_owned(),
            goal: "p(?X)".to_owned(),
            status: "ok".to_owned(),
            answers,
            verdicts,
            clauses: vec!["p(a).".to_owned(), "p(b).".to_owned()],
            verdict_probe_atoms: vec!["q(a)".to_owned(), "q(b)".to_owned()],
        }
    };

    let ans_a = GoalDirectedAnswer {
        atom: "p(a)".to_owned(),
        bindings: BTreeMap::from([("X".to_owned(), "a".to_owned())]),
        derivation_iri: "https://blackcatinformatics.ca/gmeow/goal-directed/rule/aaa".to_owned(),
        proof_checks: true,
    };
    let ans_b = GoalDirectedAnswer {
        atom: "p(b)".to_owned(),
        bindings: BTreeMap::from([("X".to_owned(), "b".to_owned())]),
        derivation_iri: "https://blackcatinformatics.ca/gmeow/goal-directed/rule/bbb".to_owned(),
        proof_checks: true,
    };
    let verdict_a = GoalDirectedVerdict {
        atom: "q(a)".to_owned(),
        verdict: "true".to_owned(),
    };
    let verdict_b = GoalDirectedVerdict {
        atom: "q(b)".to_owned(),
        verdict: "false".to_owned(),
    };

    let eval_forward = make_eval(
        vec![ans_a.clone(), ans_b.clone()],
        vec![verdict_a.clone(), verdict_b.clone()],
    );
    let eval_reversed = make_eval(
        vec![ans_b.clone(), ans_a.clone()],
        vec![verdict_b.clone(), verdict_a.clone()],
    );

    let nt_forward = project_goal_directed(std::slice::from_ref(&eval_forward));
    let nt_reversed = project_goal_directed(std::slice::from_ref(&eval_reversed));
    assert_eq!(
        nt_forward, nt_reversed,
        "the SAME two answers/verdicts in a different vector order must project to \
             byte-identical N-Triples (content-addressed IRIs, not positional):\n\
             forward:\n{nt_forward}\nreversed:\n{nt_reversed}"
    );

    // The minted IRIs are content hashes, not small positional integers.
    let a_iri = answer_iri(&eval_forward, &ans_a);
    let b_iri = answer_iri(&eval_forward, &ans_b);
    assert_ne!(a_iri, b_iri, "distinct answers mint distinct IRIs");
    assert!(
        !a_iri.ends_with("/answer/0") && !a_iri.ends_with("/answer/1"),
        "the answer IRI must not be a small positional index: {a_iri}"
    );
    let v_iri = verdict_iri(&eval_forward, &verdict_a);
    assert!(
        !v_iri.ends_with("/verdict/0") && !v_iri.ends_with("/verdict/1"),
        "the verdict IRI must not be a small positional index: {v_iri}"
    );

    // The SAME answer content always mints the SAME IRI, regardless of which position
    // it happens to occupy.
    assert_eq!(
        answer_iri(&eval_forward, &ans_a),
        answer_iri(&eval_reversed, &ans_a),
        "the same answer content mints the same IRI regardless of vector position"
    );
}

// ── Distinct authored IRIs sharing a local name mint collision-free resource nodes ──

#[test]
fn distinct_authored_iris_with_same_local_name_project_to_distinct_nodes() {
    // Two programs authored under DIFFERENT full IRIs that happen to share the SAME local
    // name (`prog`) must NEVER collapse to the same projected query/program nodes. The
    // minted resource IRIs fold the FULL authored IRI, not the bare local name.
    let mk = |iri: &str| GoalDirectedEvaluation {
        iri: iri.to_owned(),
        name: local_name(iri).to_owned(),
        description: "collision probe".to_owned(),
        goal: "p(?0)".to_owned(),
        status: "ok".to_owned(),
        answers: Vec::new(),
        verdicts: Vec::new(),
        clauses: vec!["p(a).".to_owned()],
        verdict_probe_atoms: Vec::new(),
    };
    let a = mk("https://a.example/prog");
    let b = mk("https://b.example/prog");
    assert_eq!(a.name, b.name, "the two programs share a local name");
    assert_ne!(
        query_iri(&a),
        query_iri(&b),
        "distinct authored IRIs must mint distinct query nodes"
    );
    assert_ne!(
        program_iri_for(&a),
        program_iri_for(&b),
        "distinct authored IRIs must mint distinct program nodes"
    );
    // Projected TOGETHER, the two do not merge: two distinct `GoalDirectedQuery` subjects.
    let nt = project_goal_directed(&[a.clone(), b.clone()]);
    assert!(
        nt.contains(&format!("<{}>", query_iri(&a))),
        "program a's query node is projected:\n{nt}"
    );
    assert!(
        nt.contains(&format!("<{}>", query_iri(&b))),
        "program b's query node is projected:\n{nt}"
    );
    let query_type_object = format!("<{GMEOW}GoalDirectedQuery> .");
    let query_nodes = nt
        .lines()
        .filter(|l| l.ends_with(&query_type_object))
        .count();
    assert_eq!(
        query_nodes, 2,
        "two distinct query subjects survive projection (no collision):\n{nt}"
    );
}

// ── An order-sorted binary program is excluded from the unsorted forward oracle ──

const SORTED_BINARY_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:b a ex:Blue .\n\
        ex:sortedReach a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:reach ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termVariable \"W\" ;\n\
                                  logic:variableSort ex:Red ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:reach ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termIri ex:b ]\n\
            ] .\n\
    ";

#[test]
fn a_sorted_binary_program_is_excluded_from_the_unsorted_forward_oracle() {
    // `reach(a,b)` with `b : ex:Blue`, query `reach(a, W)` with `W : ex:Red` (incomparable
    // to Blue). The SORTED backward answer set is correctly EMPTY (Blue ⋢ Red). The
    // UNSORTED forward chase, however, would admit `reach(a,b)` — so cross-checking the two
    // would HARD-FAIL spuriously. The gate must therefore EXCLUDE any order-sorted program.
    let (prog, diags) =
        gmeow_logic_compile::frontend::parse_logic_str(SORTED_BINARY_REASONING_PROGRAM_TTL, None)
            .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);
    let program = &prog.reasoning_programs[0];
    assert!(
        !program.variable_sorts.is_empty(),
        "the program is order-sorted (W : Red)"
    );
    assert!(
        !is_definite_function_free_binary(program),
        "an order-sorted program — even a definite, function-free, binary one — is outside \
             the UNSORTED forward oracle's fragment"
    );

    // Evaluating must NOT trip the T3 oracle: with the program excluded, the correctly-empty
    // sorted backward answer set is returned without a spurious cross-engine hard-fail.
    let evals = evaluate_reasoning_programs(std::slice::from_ref(program), &[])
        .expect("the order-sorted program evaluates without a spurious oracle hard-fail");
    assert_eq!(evals.len(), 1);
    assert_eq!(evals[0].status, "ok");
    assert!(
        evals[0].answers.is_empty(),
        "the Red-sorted W does not bind the Blue-typed constant b (incomparable sorts): {:?}",
        evals[0].answers
    );
}

// ── A constant carrying multiple asserted sorts binds when any of them satisfies ──

const MULTI_TYPE_CONSTANT_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        @prefix math: <https://blackcatinformatics.ca/math/> .\n\
        \n\
        ex:c a ex:Set , math:Integer .\n\
        ex:multiType a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort math:RealNumber ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:c ]\n\
            ] .\n\
    ";

#[test]
fn a_constant_with_two_asserted_sorts_binds_when_one_is_comparable() {
    // `ex:c a ex:Set, math:Integer` — TWO asserted types, one (Integer) comparable to the
    // query variable's declared `math:RealNumber` sort (ℤ ⊑ ℝ under the reasoned edge), one
    // (`ex:Set`) incomparable. The order-sort semantics: `c` binds `X : RealNumber` because
    // ANY of its asserted types satisfies the sort. A last-write-wins fold that kept only
    // the lexically-last type (`ex:Set`) would WRONGLY return zero answers.
    let (prog, diags) = gmeow_logic_compile::frontend::parse_logic_str(
        MULTI_TYPE_CONSTANT_REASONING_PROGRAM_TTL,
        None,
    )
    .expect("parse succeeds");
    assert!(
        diags
            .iter()
            .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
        "unexpected error diagnostics: {diags:#?}"
    );
    assert_eq!(prog.reasoning_programs.len(), 1);

    const EX: &str = "https://example.org/goal-directed-test/";
    let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];
    let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &subsort_edges)
        .expect("evaluate the multi-typed-constant program");
    assert_eq!(evals.len(), 1);
    let eval = &evals[0];
    assert_eq!(eval.status, "ok");
    assert_eq!(
        eval.answers.len(),
        1,
        "the constant binds via its Integer type (ℤ ⊑ ℝ) even though its Set type is \
             incomparable — every asserted type is retained, not just the last: {:?}",
        eval.answers
    );
    assert_eq!(
        eval.answers[0].bindings.get("X").map(String::as_str),
        Some(format!("{EX}c").as_str()),
        "the multiply-typed constant binds X = ex:c"
    );

    // Control: WITHOUT the reasoned ℤ⊑ℝ edge, NEITHER asserted type reaches RealNumber, so
    // the binding is correctly refused — the multi-sort handling never fabricates an edge.
    let evals_no_edge = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
        .expect("evaluate with no subsort edges");
    assert!(
        evals_no_edge[0].answers.is_empty(),
        "without ℤ⊑ℝ, neither Set nor Integer reaches RealNumber, so no binding: {:?}",
        evals_no_edge[0].answers
    );
}
