// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn atom(predicate: &str, args: &[&str]) -> Formula {
    Formula::Atom {
        relation: Term::Iri(format!("urn:{predicate}")),
        args: args
            .iter()
            .map(|name| Term::Var((*name).to_owned()))
            .collect(),
    }
}

fn exists(name: &str, body: Formula) -> Formula {
    Formula::Exists {
        vars: vec![name.to_owned()],
        body: Box::new(body),
    }
}

fn compile(head: Formula) -> (Vec<RcRule>, Vec<String>) {
    let formula = Formula::Forall {
        vars: vec!["x".to_owned(), "y".to_owned(), "existH0".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(Formula::And(vec![
                atom("seed", &["x", "y"]),
                atom("reserved", &["x", "existH0"]),
            ])),
            Box::new(head),
        )),
    };
    lower_formulas_to_rc(
        &LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]),
    )
}

#[test]
fn one_dependency_shares_witness_across_binary_and_reified_heads() {
    let (rules, residue) = compile(exists(
        "z",
        Formula::And(vec![
            atom("p", &["x", "z"]),
            atom("q", &["z", "y"]),
            atom("tuple", &["x", "y", "z"]),
        ]),
    ));
    assert!(residue.is_empty(), "{residue:?}");
    assert_eq!(rules.len(), 1);
    let rule = &rules[0];
    assert!(rule.has_existential_head());
    let witness = &rule.head.object;
    assert_eq!(rule.head_conjuncts[0].subject, *witness);
    assert_eq!(rule.head_conjuncts[4].object, *witness);
    assert_ne!(
        *witness,
        RcTerm::Var("?existH0".to_owned()),
        "authored variable cannot be captured"
    );
    assert!(!body_bound_vars(&rule.body).contains(match witness {
        RcTerm::Var(v) => v,
        _ => panic!("witness variable"),
    }));
    assert_eq!(rule.head_conjuncts.len(), 5);
}

#[test]
fn nested_and_sibling_binders_do_not_capture_each_other_or_the_body() {
    let (rules, residue) = compile(exists(
        "x",
        Formula::And(vec![
            atom("outer", &["x", "y"]),
            exists("x", atom("inner", &["x", "y"])),
            atom("restored", &["x", "y"]),
            exists("x", atom("sibling", &["x", "y"])),
        ]),
    ));
    assert!(residue.is_empty(), "{residue:?}");
    let rule = &rules[0];
    let outer = &rule.head.subject;
    assert_ne!(*outer, RcTerm::Var("?x".to_owned()));
    assert_eq!(rule.head_conjuncts[1].subject, *outer);
    assert_ne!(rule.head_conjuncts[0].subject, *outer);
    assert_ne!(rule.head_conjuncts[2].subject, *outer);
    assert_ne!(
        rule.head_conjuncts[0].subject,
        rule.head_conjuncts[2].subject
    );
}

#[test]
fn unsafe_or_nonpositive_heads_remain_whole_residue() {
    for head in [
        exists("z", atom("unsafe", &["z", "free"])),
        exists(
            "z",
            Formula::Or(vec![atom("p", &["x", "z"]), atom("q", &["x", "z"])]),
        ),
        exists(
            "z",
            Formula::And(vec![
                atom("p", &["x", "z"]),
                Formula::Not(Box::new(atom("q", &["x", "z"]))),
            ]),
        ),
        exists(
            "z",
            Formula::Forall {
                vars: vec!["u".to_owned()],
                body: Box::new(atom("p", &["z", "u"])),
            },
        ),
        atom("unsafe", &["x", "free"]),
    ] {
        let (rules, residue) = compile(head);
        assert!(
            rules.is_empty(),
            "an unsupported head cannot leak an admitted prefix: {rules:?}"
        );
        assert_eq!(residue.len(), 1);
    }
}

#[test]
fn single_head_witness_and_noninventing_conjunction_are_distinguished() {
    let (rules, residue) = compile(exists("z", atom("witness", &["x", "z"])));
    assert!(residue.is_empty());
    assert!(rules[0].head_conjuncts.is_empty());
    assert!(rules[0].has_existential_head());
    let (rules, residue) = compile(Formula::And(vec![
        atom("p", &["x", "y"]),
        atom("q", &["y", "x"]),
    ]));
    assert!(residue.is_empty());
    assert_eq!(rules.len(), 1);
    assert!(!rules[0].has_existential_head());
    assert_eq!(rules[0].head_conjuncts.len(), 1);
}
