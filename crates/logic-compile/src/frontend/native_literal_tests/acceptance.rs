// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW's literal representation contract, through each authored compiler path.
//! These are synthetic compiler inputs, not RDF parser conformance or value comparisons.

use super::*;
use crate::ir::AtomicTerm;
use crate::relational_core::{RcAtom, RelationalCoreProgram, project_relational_core_dataset};
use std::collections::BTreeSet;

const PREFIXES: &str = r#"
    @prefix logic: <https://blackcatinformatics.ca/logic/> .
    @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
    @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
"#;
const PAYLOAD: &str = "https://blackcatinformatics.ca/logic/payload";
const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

struct LiteralCase {
    source: &'static str,
    identity: &'static str,
    expected: RdfLiteral,
}

fn cases() -> Vec<LiteralCase> {
    [
        (r#""1""#, "string", "1", None, None, None),
        (r#""1"^^xsd:string"#, "string", "1", None, None, None),
        (
            r#""1"^^xsd:integer"#,
            "integer",
            "1",
            Some(INTEGER),
            None,
            None,
        ),
        (
            r#""01"^^xsd:integer"#,
            "integer-01",
            "01",
            Some(INTEGER),
            None,
            None,
        ),
        (r#""1"@en"#, "english", "1", None, Some("en"), None),
        (r#""1"@EN"#, "english", "1", None, Some("en"), None),
        (r#""1"@fr"#, "french", "1", None, Some("fr"), None),
        (
            r#""1"@en--ltr"#,
            "ltr",
            "1",
            None,
            Some("en"),
            Some(RdfTextDirection::Ltr),
        ),
        (
            r#""1"@en--rtl"#,
            "rtl",
            "1",
            None,
            Some("en"),
            Some(RdfTextDirection::Rtl),
        ),
    ]
    .into_iter()
    .map(
        |(source, identity, lexical, datatype, language, direction)| LiteralCase {
            source,
            identity,
            expected: RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype: datatype.map(str::to_owned),
                language: language.map(str::to_owned),
                direction,
            },
        },
    )
    .collect()
}

fn atom_source(node: &str, literal: &str) -> String {
    format!(
        r#"<{node}> a logic:Formula; logic:relation logic:payload;
            logic:argument [ logic:termIndex 0; logic:termIri <urn:subject> ],
                           [ logic:termIndex 1; logic:termLiteral {literal} ] ."#
    )
}

fn sources(literal: &str) -> [(&'static str, String); 4] {
    [
        ("direct", format!("<urn:subject> logic:payload {literal} .")),
        ("standalone", atom_source("urn:root", literal)),
        (
            "nested",
            format!(
                r#"<urn:root> a logic:Formula; logic:and <urn:leaf>, <urn:marker> .
                {}
                <urn:marker> a logic:Formula; logic:relation logic:marker;
                  logic:argument [ logic:termIndex 0; logic:termIri <urn:subject> ],
                                 [ logic:termIndex 1; logic:termIri <urn:present> ] ."#,
                atom_source("urn:leaf", literal)
            ),
        ),
        (
            "rule",
            format!(
                r#"<urn:rule> a logic:Rule;
                  logic:head [ rdf:subject "?s"; rdf:predicate logic:payload;
                               rdf:object {literal} ];
                  logic:body [ rdf:subject "?s"; rdf:predicate logic:payload;
                               rdf:object {literal} ] ."#
            ),
        ),
    ]
}

fn compile(body: &str) -> LogicProgram {
    let (program, diagnostics) = parse_logic_str(&format!("{PREFIXES}\n{body}"), None).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}\n{body}");
    program
}

fn payload_atoms(program: &RelationalCoreProgram) -> Vec<&RcAtom> {
    program
        .facts
        .iter()
        .chain(program.rules.iter().flat_map(|rule| {
            std::iter::once(&rule.head)
                .chain(&rule.head_conjuncts)
                .chain(&rule.body)
        }))
        .filter(|atom| atom.predicate == PAYLOAD)
        .collect()
}

fn assert_literal_paths(program: &LogicProgram, path: &str, expected: &RdfLiteral) {
    match path {
        "direct" | "standalone" => {
            assert!(program.formulas.is_empty());
            assert_eq!(program.axioms.len(), 1);
            assert_eq!(program.axioms[0].obj.as_literal(), Some(expected));
        }
        "nested" => {
            assert!(
                program.axioms.is_empty(),
                "a component is not a separate assertion"
            );
            let [Formula::And(children)] = program.formulas.as_slice() else {
                panic!("expected one authored conjunction: {:?}", program.formulas);
            };
            assert_eq!(children.len(), 2);
            let payload = children
                .iter()
                .find_map(|child| match child {
                    Formula::Atom {
                        relation: Term::Iri(relation),
                        args,
                    } if relation == PAYLOAD => Some(args),
                    _ => None,
                })
                .expect("the conjunction retains its payload atom");
            assert_eq!(
                payload,
                &vec![
                    Term::Iri("urn:subject".into()),
                    Term::Literal(expected.clone())
                ]
            );
        }
        "rule" => {
            assert!(program.axioms.is_empty());
            assert_eq!(program.rules.len(), 1);
            assert_eq!(program.rules[0].head.obj.as_literal(), Some(expected));
            assert_eq!(program.rules[0].body.len(), 1);
            assert_eq!(program.rules[0].body[0].obj.as_literal(), Some(expected));
        }
        _ => panic!("unrecognized test path {path}"),
    }
    let lowered = lower_program_with_formulas(program);
    assert!(lowered.residue.is_empty(), "{:?}", lowered.residue);
    let atoms = payload_atoms(&lowered);
    assert_eq!(atoms.len(), if path == "rule" { 2 } else { 1 });
    for atom in atoms {
        assert_eq!(atom.object.as_literal(), Some(expected), "{path}");
        assert!(!atom.negated);
    }
}

#[test]
fn authored_literal_paths_preserve_identity_through_gmeow_codecs() {
    for case in cases() {
        for (path, source) in sources(case.source) {
            let program = compile(&source);
            assert_literal_paths(&program, path, &case.expected);

            let cached: LogicProgram =
                serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
            assert_eq!(cached.canonical_key(), program.canonical_key());
            assert_literal_paths(&cached, path, &case.expected);

            let canonical = project_canonical_rdf12_dataset(&program).unwrap();
            let (restored, diagnostics) = parse_logic_dataset(&canonical.dataset, None).unwrap();
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            crate::adapter::assert_ir_isomorphic(&program, &restored).unwrap();
            assert_literal_paths(&restored, path, &case.expected);

            let lowered = lower_program_with_formulas(&program);
            let projected = project_relational_core_dataset(&lowered).unwrap();
            let restored = parse_relational_core(&projected).unwrap();
            assert_eq!(
                restored.projection_key().unwrap(),
                lowered.projection_key().unwrap()
            );
            assert_eq!(payload_atoms(&restored), payload_atoms(&lowered));
        }
    }
}

#[test]
fn gmeow_identity_keys_normalize_spelling_without_numeric_value_coercion() {
    let cases = cases();
    let mut records = Vec::new();
    for case in &cases {
        let mut keys = Vec::new();
        let mut enclosing_keys = Vec::new();
        for (_, source) in sources(case.source) {
            let program = compile(&source);
            let lowered = lower_program_with_formulas(&program);
            let atoms = payload_atoms(&lowered);
            let atom = atoms.first().unwrap();
            keys.push(atom.object.key());
            enclosing_keys.push((program.canonical_key(), lowered.content_key().unwrap()));
        }
        assert!(keys.iter().all(|key| key == &keys[0]));
        records.push((case.identity, keys.remove(0), enclosing_keys));
    }
    for (left_group, left_key, left_enclosing) in &records {
        for (right_group, right_key, right_enclosing) in &records {
            assert_eq!(
                left_key == right_key,
                left_group == right_group,
                "term identity groups {left_group} and {right_group}"
            );
            for ((left_program, left_lowered), (right_program, right_lowered)) in
                left_enclosing.iter().zip(right_enclosing)
            {
                assert_eq!(left_program == right_program, left_group == right_group);
                assert_eq!(left_lowered == right_lowered, left_group == right_group);
            }
        }
    }
    // Relational finalization deduplicates actual RDF identities, retaining distinct
    // lexical integers; this says nothing about their separate datatype value comparison.
    let atoms = cases
        .iter()
        .map(|case| compile(&sources(case.source)[0].1).axioms.remove(0))
        .collect();
    let program = LogicProgram::new(atoms, vec![], vec![], None);
    let lowered = lower_program_with_formulas(&program);
    assert_eq!(lowered.facts.len(), 7);
    let objects: BTreeSet<_> = lowered.facts.iter().map(|fact| fact.object.key()).collect();
    assert_eq!(objects.len(), 7);
    assert!(lowered.facts.iter().any(|fact| {
        fact.object.as_literal().is_some_and(|literal| {
            literal.lexical_form == "01" && literal.datatype_iri() == INTEGER
        })
    }));

    // A direct claim and a standalone Formula with the same native object share
    // one compact assertion; the source shape cannot manufacture a new identity.
    let source = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            format!(
                "{}\n{}",
                sources(case.source)[0].1,
                atom_source(&format!("urn:root:{index}"), case.source)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let merged = compile(&source);
    assert_eq!(merged.axioms.len(), 7);
    assert!(merged.formulas.is_empty());
    assert_eq!(lower_program_with_formulas(&merged), lowered);
}

#[test]
fn checked_literal_constructors_share_normalized_identity_with_authored_formula_terms() {
    for mut literal in [
        RdfLiteral::typed("1", "http://www.w3.org/2001/XMLSchema#string"),
        RdfLiteral::language_tagged("1", "EN"),
        RdfLiteral {
            lexical_form: "1".into(),
            datatype: None,
            language: Some("EN".into()),
            direction: Some(RdfTextDirection::Rtl),
        },
        RdfLiteral::typed("01", INTEGER),
    ] {
        let formula = Formula::atom(
            Term::Iri(PAYLOAD.into()),
            vec![
                Term::Iri("urn:subject".into()),
                Term::rdf_literal(literal.clone()).unwrap(),
            ],
        )
        .unwrap();
        let axiom =
            LogicAxiom::ground("urn:subject", PAYLOAD, AtomicTerm::Literal(literal.clone()))
                .unwrap();
        assert_eq!(formula.as_horn_axiom().unwrap(), axiom);
        let cached: Formula =
            serde_json::from_slice(&serde_json::to_vec(&formula).unwrap()).unwrap();
        assert_eq!(cached, formula);
        assert_eq!(cached.content_key(), formula.content_key());
        if let Some(language) = &mut literal.language {
            language.make_ascii_lowercase();
        }
        if literal.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#string") {
            literal.datatype = None;
        }
        assert_eq!(axiom.obj.as_literal(), Some(&literal));
        assert_eq!(
            axiom.obj.rdf_term(),
            Some(purrdf::RdfTerm::literal(literal))
        );
    }
}

#[test]
fn signed_literals_keep_the_same_complete_object_without_cross_identity_collapse() {
    for case in cases() {
        let source = format!(
            "<urn:subject> logic:payload {} .\n<urn:negative> a logic:Formula; logic:not <urn:leaf> .\n{}",
            case.source,
            atom_source("urn:leaf", case.source)
        );
        let program = compile(&source);
        assert_eq!(program.axioms.len(), 1);
        let [Formula::Not(inner)] = program.formulas.as_slice() else {
            panic!(
                "the explicit negation must retain its sign: {:?}",
                program.formulas
            );
        };
        let negative_object = inner.as_horn_axiom().unwrap().obj;
        assert_eq!(program.axioms[0].obj, negative_object);
        assert_eq!(negative_object.as_literal(), Some(&case.expected));
        for other in cases() {
            let other_atom = compile(&atom_source("urn:other", other.source))
                .axioms
                .remove(0);
            assert_eq!(
                negative_object == other_atom.obj,
                case.identity == other.identity
            );
        }
    }
}

#[test]
fn quoted_literal_propositions_do_not_assert_their_payload() {
    for case in cases() {
        for asserted in [false, true] {
            let source = format!(
                "<urn:quotation> rdf:reifies <<( <urn:subject> logic:payload {} )>> .\n\
                 <urn:negative> a logic:Formula; logic:not <urn:leaf> .\n{}\n{}",
                case.source,
                atom_source("urn:leaf", case.source),
                if asserted {
                    sources(case.source)[0].1.clone()
                } else {
                    String::new()
                }
            );
            let program = compile(&source);
            assert_eq!(program.axioms.len(), usize::from(asserted), "{source}");
            let [Formula::Not(inner)] = program.formulas.as_slice() else {
                panic!(
                    "quotation must not add a positive formula: {:?}",
                    program.formulas
                );
            };
            assert_eq!(
                inner.as_horn_axiom().unwrap().obj,
                AtomicTerm::Literal(case.expected.clone())
            );
            if asserted {
                assert_eq!(program.axioms[0].obj.as_literal(), Some(&case.expected));
                assert!(!program.axioms[0].negated);
            }
            let canonical = project_canonical_rdf12_dataset(&program).unwrap();
            let (restored, diagnostics) = parse_logic_dataset(&canonical.dataset, None).unwrap();
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            crate::adapter::assert_ir_isomorphic(&program, &restored).unwrap();
            assert_eq!(restored.axioms.len(), usize::from(asserted));
        }
    }
}
