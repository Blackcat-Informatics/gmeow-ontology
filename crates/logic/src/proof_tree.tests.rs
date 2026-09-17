// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use gmeow_logic_compile::ir::{EvaluationMode, Formula, Term};

const NS: &str = "https://example.org/pt#";

fn atom(pred: &str, args: &[Term]) -> Formula {
    Formula::atom(Term::iri(format!("{NS}{pred}")).unwrap(), args.to_vec()).unwrap()
}

fn var(name: &str) -> Term {
    Term::var(name).unwrap()
}

fn konst(name: &str) -> Term {
    Term::iri(format!("{NS}{name}")).unwrap()
}

/// `b(X) :- a(X).  c(X) :- b(X).  a(w).`  ?- `c(w)` — the Horn shape a subclass
/// refutation (`a ⊑ b, b ⊑ c ⊢ a ⊑ c` with a fresh witness `w`) reduces to.
fn subclass_chain() -> ReasoningProgramIr {
    ReasoningProgramIr::new(
        "https://example.org/pt/subclass-chain",
        EvaluationMode::Backward,
        vec![
            Formula::Implies(
                Box::new(atom("a", &[var("X")])),
                Box::new(atom("b", &[var("X")])),
            ),
            Formula::Implies(
                Box::new(atom("b", &[var("X")])),
                Box::new(atom("c", &[var("X")])),
            ),
            atom("a", &[konst("w")]),
        ],
        atom("c", &[konst("w")]),
        vec![],
        vec![],
        vec![],
    )
    .expect("well-formed reasoning program")
}

#[test]
fn proof_tree_decodes_parent_edges_and_asserted_leaves() {
    let proved = prove_reasoning_program(&subclass_chain(), &[]).expect("resolves");
    assert_eq!(proved.status, "ok", "the tiny program resolves in budget");
    assert_eq!(proved.answers.len(), 1, "exactly one answer: c(w)");
    let tree = &proved.answers[0].tree;
    assert_eq!(tree.len(), 3, "c(w) ← b(w) ← a(w) is three steps");

    // Root first, and it concludes the answer atom.
    let root = tree.root();
    assert_eq!(root.conclusion, format!("{NS}c({NS}w)"));
    assert!(!root.asserted, "the root is a rule application");
    assert!(
        root.rule_iri.is_some(),
        "a derived step cites a firing rule"
    );
    assert_eq!(root.premises, vec![1], "the root has one premise");

    let mid = &tree.steps()[1];
    assert_eq!(mid.conclusion, format!("{NS}b({NS}w)"));
    assert!(!mid.asserted);
    assert_eq!(mid.premises, vec![2]);

    let leaf = &tree.steps()[2];
    assert_eq!(leaf.conclusion, format!("{NS}a({NS}w)"));
    assert!(leaf.asserted, "a(w) is the asserted EDB leaf");
    assert!(
        leaf.rule_iri.is_none(),
        "an asserted leaf cites no firing rule"
    );
    assert!(leaf.premises.is_empty());
}

#[test]
fn proof_tree_derivation_iris_match_the_proof_projection() {
    // Identity parity: rebuild the SAME proof term through the engine and assert every tree
    // step's IRI is byte-identical to the flattened backward projection's own recipe
    // (DERIVATION_PREFIX over PurRDF's derivation_id) — the tree must never fork it.
    let program = subclass_chain();
    let built = crate::goal_directed::lower_reasoning_program(&program, &[]).expect("lower");
    let crate::goal_directed::BuiltDemonstrator {
        mut dag,
        program: fol,
        ctx,
        ..
    } = built;
    let outcome = match resolve_fol(
        &mut dag,
        &fol,
        &ctx,
        &crate::goal_directed::GROUNDING_BUDGET,
    ) {
        FolControl::Decided(o) => o,
        FolControl::Unsupported(kind) => panic!("unsupported: {kind:?}"),
    };
    assert_eq!(outcome.answers.len(), 1);
    let proof = &outcome.answers[0].proof;
    let tree = ProofTree::of_answer(&dag, proof).expect("tree");

    // Walk the FolProof term independently (root, then its single premise chain) and compare.
    let mut node = proof;
    for step in tree.steps() {
        let expected = format!(
            "{}{}",
            provenance::DERIVATION_PREFIX,
            derivation_id(&dag, node)
        );
        assert_eq!(
            step.derivation_iri, expected,
            "tree step identity must equal the flattened derivation_id projection"
        );
        match node {
            FolProof::ByRule { premises, .. } => {
                assert_eq!(premises.len(), 1);
                node = &premises[0];
            }
            FolProof::Assert { .. } => break,
        }
    }
    // And every step's IRI is a genuine derivation-namespace address.
    for step in tree.steps() {
        assert!(
            step.derivation_iri
                .starts_with(provenance::DERIVATION_PREFIX),
            "{}",
            step.derivation_iri
        );
    }
}

#[test]
fn tstp_step_name_round_trips_the_derivation_iri() {
    let iri = provenance::mint_derivation_id(
        "https://example.org/rule",
        &["https://example.org/reifier/1"],
    );
    let name = tstp_step_name(&iri).expect("name");
    assert!(name.starts_with(TSTP_STEP_NAME_PREFIX));
    assert_eq!(tstp_step_derivation_iri(&name).expect("inverse"), iri);

    assert!(
        tstp_step_name("https://example.org/not-a-derivation").is_err(),
        "an IRI outside the derivation namespace has no step name"
    );
    assert!(
        tstp_step_derivation_iri("no_sigil").is_err(),
        "a name without the sigil denotes no derivation"
    );
}

#[test]
fn tstp_derivation_has_one_line_per_step_leaves_first() {
    let proved = prove_reasoning_program(&subclass_chain(), &[]).expect("resolves");
    let tstp = proved.answers[0].tree.to_tstp().expect("tstp");
    let lines: Vec<&str> = tstp.lines().collect();
    assert_eq!(lines.len(), 3, "one line per step:\n{tstp}");
    assert!(
        lines[0].contains(", axiom, "),
        "leaves come first: {}",
        lines[0]
    );
    assert!(
        lines[2].contains(", plain, ") && lines[2].contains("inference("),
        "the root is the last, derived line: {}",
        lines[2]
    );
    // Every parent name is defined by an earlier line.
    for (i, line) in lines.iter().enumerate() {
        let root_name =
            tstp_step_name(&proved.answers[0].tree.steps()[2 - i].derivation_iri).expect("name");
        assert!(
            line.starts_with(&format!("cnf({root_name}, ")),
            "line {i} names the reverse-order step: {line}"
        );
    }
    // The IRIs ride as single-quoted atoms, never lossily shortened.
    assert!(tstp.contains(&format!("'{NS}c'('{NS}w')")), "{tstp}");
}

#[test]
fn quoted_atom_escapes_backslash_and_quote() {
    assert_eq!(quoted_atom("a'b\\c"), "'a\\'b\\\\c'");
}
