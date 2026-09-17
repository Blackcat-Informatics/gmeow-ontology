// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::frontend::{
    AdmittedSemanticUnit, CompiledTheory, PropertyCharacteristic, SemanticUnitDisposition,
    SourceAdmission, SourceAdmissionStatus, SourceAssertion,
};
use crate::ir::{AtomicTerm, Formula, LogicAxiom, LogicRule, Term};

// `matches!` patterns require literals; this keeps canonical logic IRIs beside
// their standard-vocabulary counterparts without allocating.
macro_rules! concat_logic {
    ($local:literal) => {
        concat!("https://blackcatinformatics.ca/logic/", $local)
    };
}

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";

/// Whether a complete external problem was authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FofProjectionStatus {
    Complete,
    Blocked,
}

/// One emitted sentence and every admitted source unit it represents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FofSentence {
    pub name: String,
    pub role: String,
    pub body: String,
    pub source_units: BTreeSet<String>,
}

/// A projection refusal. No problem text is returned while any blocker exists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FofProjectionBlocker {
    pub unit_id: Option<String>,
    pub code: String,
    pub reason: String,
}

/// Typed projection publication. `problem` and `problem_digest` are present only
/// for a complete, nonempty, source-bound projection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FofProjection {
    pub status: FofProjectionStatus,
    pub profile_id: String,
    pub source_digest: String,
    pub program_digest: String,
    pub selection_digest: String,
    pub sentences: Vec<FofSentence>,
    pub blockers: Vec<FofProjectionBlocker>,
    pub problem: Option<String>,
    pub problem_digest: Option<String>,
}

/// Project one exact, completely admitted source selection to TPTP FOF.
///
/// The function is deliberately total: refusals are typed data and can be emitted
/// by a CLI/report surface. A caller may invoke an external prover only when status
/// is [`FofProjectionStatus::Complete`] and `problem` is present.
#[must_use]
pub fn project_tptp_fof(theory: &CompiledTheory, admission: &SourceAdmission) -> FofProjection {
    let mut blockers = admission_blockers(admission);
    if !admission.matches(theory) {
        blockers.push(FofProjectionBlocker {
            unit_id: None,
            code: "ADMISSION_THEORY_IDENTITY_MISMATCH".into(),
            reason: "the source admission does not belong to this immutable compiled theory".into(),
        });
    }
    if admission.status != SourceAdmissionStatus::Complete && blockers.is_empty() {
        blockers.push(FofProjectionBlocker {
            unit_id: None,
            code: match admission.status {
                SourceAdmissionStatus::Empty => "EMPTY_SELECTED_THEORY",
                SourceAdmissionStatus::Blocked => "BLOCKED_SOURCE_ADMISSION",
                SourceAdmissionStatus::Malformed => "MALFORMED_SOURCE_ADMISSION",
                SourceAdmissionStatus::Complete => unreachable!(),
            }
            .into(),
            reason: "source admission did not authorize a complete classical theory".into(),
        });
    }

    let mut bodies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if blockers.is_empty() {
        for unit in admission.admitted() {
            let SemanticUnitDisposition::Admitted(payload) = &unit.disposition else {
                unreachable!("SourceAdmission::admitted returned a non-admitted row");
            };
            match bodies_for(payload, theory, admission) {
                Ok(projected) => {
                    for body in projected {
                        bodies.entry(body).or_default().insert(unit.id.clone());
                    }
                }
                Err((code, reason)) => blockers.push(FofProjectionBlocker {
                    unit_id: Some(unit.id.clone()),
                    code,
                    reason,
                }),
            }
        }
    }
    if blockers.is_empty() && bodies.is_empty() {
        blockers.push(FofProjectionBlocker {
            unit_id: None,
            code: "EMPTY_PROJECTED_THEORY".into(),
            reason: "the complete admission produced no classical sentence".into(),
        });
    }

    let sentences: Vec<_> = bodies
        .into_iter()
        .map(|(body, source_units)| {
            let digest = digest_bytes(b"gmeow-tptp-sentence-v1\0", body.as_bytes());
            FofSentence {
                name: format!("gmeow_{digest}"),
                role: "axiom".into(),
                body,
                source_units,
            }
        })
        .collect();
    let (status, problem, problem_digest) = if blockers.is_empty() {
        let problem = render_problem(admission, &sentences);
        let digest = digest_bytes(b"gmeow-tptp-problem-v1\0", problem.as_bytes());
        (FofProjectionStatus::Complete, Some(problem), Some(digest))
    } else {
        (FofProjectionStatus::Blocked, None, None)
    };
    FofProjection {
        status,
        profile_id: admission.profile.id.clone(),
        source_digest: admission.source_digest.clone(),
        program_digest: admission.program_digest.clone(),
        selection_digest: admission.selection_digest.clone(),
        sentences,
        blockers,
        problem,
        problem_digest,
    }
}

fn admission_blockers(admission: &SourceAdmission) -> Vec<FofProjectionBlocker> {
    admission
        .blockers()
        .map(|unit| match &unit.disposition {
            SemanticUnitDisposition::Blocked { code, reason }
            | SemanticUnitDisposition::Malformed { code, reason } => FofProjectionBlocker {
                unit_id: Some(unit.id.clone()),
                code: code.clone(),
                reason: reason.clone(),
            },
            _ => unreachable!("SourceAdmission::blockers returned a non-blocker"),
        })
        .collect()
}

fn bodies_for(
    payload: &AdmittedSemanticUnit,
    theory: &CompiledTheory,
    admission: &SourceAdmission,
) -> Result<Vec<String>, (String, String)> {
    match payload {
        AdmittedSemanticUnit::Assertion(assertion) => {
            assertion_body(assertion).map(|body| vec![body])
        }
        AdmittedSemanticUnit::Formula { index } => theory
            .program()
            .formulas
            .get(*index)
            .ok_or_else(|| missing("formula", *index))
            .and_then(|formula| formula_body(formula, &admission.declared_classes))
            .map(|body| vec![body]),
        AdmittedSemanticUnit::Rule { index } => theory
            .program()
            .rules
            .get(*index)
            .ok_or_else(|| missing("rule", *index))
            .and_then(|rule| rule_body(rule, &admission.declared_classes))
            .map(|body| vec![body]),
        AdmittedSemanticUnit::PropertyCharacteristic(characteristic) => {
            property_characteristic_body(characteristic).map(|body| vec![body])
        }
    }
}

fn assertion_body(assertion: &SourceAssertion) -> Result<String, (String, String)> {
    let subject = constant_term(&assertion.subject)?;
    let object = constant_term(&assertion.object)?;
    let member = |value: &str, class: &str| format!("{}({value},{class})", member_symbol());
    let binary = |property: &str, left: &str, right: &str| {
        format!("{}({left},{right})", predicate_symbol(property, 2))
    };
    match assertion.predicate.as_str() {
        RDF_TYPE | concat_logic!("instanceOf") => Ok(member(&subject, &object)),
        RDFS_SUBCLASS | concat_logic!("subClassOf") => Ok(format!(
            "(! [V_x] : ({} => {}))",
            member("V_x", &subject),
            member("V_x", &object)
        )),
        OWL_EQUIVALENT_CLASS | concat_logic!("equivalentClass") => Ok(format!(
            "(! [V_x] : ({} <=> {}))",
            member("V_x", &subject),
            member("V_x", &object)
        )),
        OWL_DISJOINT_WITH | concat_logic!("disjointWith") => Ok(format!(
            "(! [V_x] : (~ ({} & {})))",
            member("V_x", &subject),
            member("V_x", &object)
        )),
        RDFS_SUBPROPERTY | concat_logic!("subPropertyOf") => {
            let (left, right) = property_pair(assertion)?;
            Ok(format!(
                "(! [V_x,V_y] : ({} => {}))",
                binary(left, "V_x", "V_y"),
                binary(right, "V_x", "V_y")
            ))
        }
        OWL_EQUIVALENT_PROPERTY | concat_logic!("equivalentProperty") => {
            let (left, right) = property_pair(assertion)?;
            Ok(format!(
                "(! [V_x,V_y] : ({} <=> {}))",
                binary(left, "V_x", "V_y"),
                binary(right, "V_x", "V_y")
            ))
        }
        OWL_INVERSE_OF | concat_logic!("inverseOf") => {
            let (left, right) = property_pair(assertion)?;
            Ok(format!(
                "(! [V_x,V_y] : ({} <=> {}))",
                binary(left, "V_x", "V_y"),
                binary(right, "V_y", "V_x")
            ))
        }
        RDFS_DOMAIN | concat_logic!("domain") => {
            let (property, class) = property_pair(assertion)?;
            Ok(format!(
                "(! [V_x,V_y] : ({} => {}))",
                binary(property, "V_x", "V_y"),
                member("V_x", &iri_constant(class))
            ))
        }
        RDFS_RANGE | concat_logic!("range") => {
            let (property, class) = property_pair(assertion)?;
            Ok(format!(
                "(! [V_x,V_y] : ({} => {}))",
                binary(property, "V_x", "V_y"),
                member("V_y", &iri_constant(class))
            ))
        }
        OWL_SAME_AS | concat_logic!("sameAs") => Ok(format!("({subject} = {object})")),
        OWL_DIFFERENT_FROM | concat_logic!("differentFrom") => {
            Ok(format!("({subject} != {object})"))
        }
        predicate => Ok(binary(predicate, &subject, &object)),
    }
}

fn property_pair(assertion: &SourceAssertion) -> Result<(&str, &str), (String, String)> {
    let AtomicTerm::Iri(subject) = &assertion.subject else {
        return Err((
            "NON_IRI_SCHEMA_SUBJECT".into(),
            format!(
                "{} requires an IRI property/class subject",
                assertion.predicate
            ),
        ));
    };
    let AtomicTerm::Iri(object) = &assertion.object else {
        return Err((
            "NON_IRI_SCHEMA_OBJECT".into(),
            format!(
                "{} requires an IRI property/class object",
                assertion.predicate
            ),
        ));
    };
    Ok((subject, object))
}

fn property_characteristic_body(
    characteristic: &PropertyCharacteristic,
) -> Result<String, (String, String)> {
    let p = |left: &str, right: &str| {
        format!(
            "{}({left},{right})",
            predicate_symbol(&characteristic.property, 2)
        )
    };
    let body = match characteristic.characteristic.strip_prefix(LOGIC) {
        Some("functionalProperty") => format!(
            "(! [V_x,V_y,V_z] : (({} & {}) => (V_y = V_z)))",
            p("V_x", "V_y"),
            p("V_x", "V_z")
        ),
        Some("inverseFunctionalProperty") => format!(
            "(! [V_x,V_y,V_z] : (({} & {}) => (V_x = V_z)))",
            p("V_x", "V_y"),
            p("V_z", "V_y")
        ),
        Some("transitiveProperty") => format!(
            "(! [V_x,V_y,V_z] : (({} & {}) => {}))",
            p("V_x", "V_y"),
            p("V_y", "V_z"),
            p("V_x", "V_z")
        ),
        Some("symmetricProperty") => format!(
            "(! [V_x,V_y] : ({} => {}))",
            p("V_x", "V_y"),
            p("V_y", "V_x")
        ),
        Some("asymmetricProperty") => format!(
            "(! [V_x,V_y] : ({} => (~ {})))",
            p("V_x", "V_y"),
            p("V_y", "V_x")
        ),
        Some("reflexiveProperty") => format!("(! [V_x] : {})", p("V_x", "V_x")),
        Some("irreflexiveProperty") => {
            format!("(! [V_x] : (~ {}))", p("V_x", "V_x"))
        }
        _ => {
            return Err((
                "UNSUPPORTED_PROPERTY_CHARACTERISTIC".into(),
                format!(
                    "no FOF lowering exists for {}",
                    characteristic.characteristic
                ),
            ));
        }
    };
    Ok(body)
}

fn rule_body(
    rule: &LogicRule,
    declared_classes: &BTreeSet<String>,
) -> Result<String, (String, String)> {
    let mut variables = VariableEnv::default();
    let body: Result<Vec<_>, _> = rule
        .body
        .iter()
        .map(|atom| rule_atom(atom, declared_classes, &mut variables))
        .collect();
    let body = body?;
    let head = rule_atom(&rule.head, declared_classes, &mut variables)?;
    let sentence = if body.is_empty() {
        head
    } else {
        format!("(({}) => {head})", body.join(" & "))
    };
    Ok(variables.close_all(sentence))
}

fn rule_atom(
    atom: &LogicAxiom,
    declared_classes: &BTreeSet<String>,
    variables: &mut VariableEnv,
) -> Result<String, (String, String)> {
    let subject = compact_term(&AtomicTerm::resource(atom.subject.clone()), variables)?;
    let object = compact_term(&atom.obj, variables)?;
    relation_atom(&atom.predicate, &[subject, object], declared_classes)
}

fn formula_body(
    formula: &Formula,
    declared_classes: &BTreeSet<String>,
) -> Result<String, (String, String)> {
    let free = free_variables(formula);
    let mut env = VariableEnv::default();
    let free_tokens: Vec<_> = free.iter().map(|name| env.push(name.clone())).collect();
    let body = render_formula(formula, declared_classes, &mut env)?;
    for name in free.iter().rev() {
        env.pop(name);
    }
    if free_tokens.is_empty() {
        Ok(body)
    } else {
        Ok(format!("(! [{}] : {body})", free_tokens.join(",")))
    }
}

fn render_formula(
    formula: &Formula,
    declared_classes: &BTreeSet<String>,
    env: &mut VariableEnv,
) -> Result<String, (String, String)> {
    match formula {
        Formula::Atom { relation, args } => {
            let Term::Iri(relation) = relation else {
                return Err((
                    "NON_IRI_FORMULA_RELATION".into(),
                    "a FOF relation must be an IRI".into(),
                ));
            };
            let args: Result<Vec<_>, _> = args.iter().map(|term| formula_term(term, env)).collect();
            relation_atom(relation, &args?, declared_classes)
        }
        Formula::Not(inner) => Ok(format!(
            "(~ {})",
            render_formula(inner, declared_classes, env)?
        )),
        Formula::And(parts) | Formula::Or(parts) => {
            if parts.len() < 2 {
                return Err((
                    "MALFORMED_NARY_CONNECTIVE".into(),
                    "and/or requires at least two operands".into(),
                ));
            }
            let operator = if matches!(formula, Formula::And(_)) {
                " & "
            } else {
                " | "
            };
            let rendered: Result<Vec<_>, _> = parts
                .iter()
                .map(|part| render_formula(part, declared_classes, env))
                .collect();
            Ok(format!("({})", rendered?.join(operator)))
        }
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            let operator = if matches!(formula, Formula::Implies(_, _)) {
                "=>"
            } else {
                "<=>"
            };
            Ok(format!(
                "({} {operator} {})",
                render_formula(left, declared_classes, env)?,
                render_formula(right, declared_classes, env)?
            ))
        }
        Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
            if vars.is_empty() {
                return Err((
                    "EMPTY_QUANTIFIER".into(),
                    "a first-order quantifier must bind at least one variable".into(),
                ));
            }
            let tokens: Vec<_> = vars.iter().map(|name| env.push(name.clone())).collect();
            let body = render_formula(body, declared_classes, env)?;
            for name in vars.iter().rev() {
                env.pop(name);
            }
            let quantifier = if matches!(formula, Formula::Forall { .. }) {
                "!"
            } else {
                "?"
            };
            Ok(format!("({quantifier} [{}] : {body})", tokens.join(",")))
        }
    }
}

fn relation_atom(
    relation: &str,
    args: &[String],
    declared_classes: &BTreeSet<String>,
) -> Result<String, (String, String)> {
    if args.len() == 2 && matches!(relation, RDF_TYPE | concat_logic!("instanceOf")) {
        return Ok(format!("{}({},{})", member_symbol(), args[0], args[1]));
    }
    if args.len() == 1 && declared_classes.contains(relation) {
        return Ok(format!(
            "{}({},{})",
            member_symbol(),
            args[0],
            iri_constant(relation)
        ));
    }
    if args.len() == 2 && matches!(relation, OWL_SAME_AS | concat_logic!("sameAs")) {
        return Ok(format!("({} = {})", args[0], args[1]));
    }
    if args.len() == 2
        && matches!(
            relation,
            OWL_DIFFERENT_FROM | concat_logic!("differentFrom")
        )
    {
        return Ok(format!("({} != {})", args[0], args[1]));
    }
    if args.is_empty() {
        return Ok(predicate_symbol(relation, 0));
    }
    Ok(format!(
        "{}({})",
        predicate_symbol(relation, args.len()),
        args.join(",")
    ))
}

fn constant_term(term: &AtomicTerm) -> Result<String, (String, String)> {
    match term {
        AtomicTerm::Var(variable) => Err((
            "VARIABLE_IN_GROUND_SOURCE_ASSERTION".into(),
            format!("direct source assertion contains variable {variable}"),
        )),
        AtomicTerm::Iri(iri) => Ok(iri_constant(iri)),
        AtomicTerm::Blank(blank) => Ok(quote_atom(&format!("B|{blank}"))),
        AtomicTerm::Literal(literal) => Ok(quote_atom(&format!(
            "L|{}",
            AtomicTerm::Literal(literal.clone()).key()
        ))),
    }
}

fn compact_term(
    term: &AtomicTerm,
    variables: &mut VariableEnv,
) -> Result<String, (String, String)> {
    match term {
        AtomicTerm::Var(variable) => Ok(variables.free(variable.trim_start_matches('?'))),
        _ => constant_term(term),
    }
}

fn formula_term(term: &Term, env: &mut VariableEnv) -> Result<String, (String, String)> {
    match term {
        Term::Var(name) => Ok(env.resolve(name)),
        Term::Iri(iri) => Ok(iri_constant(iri)),
        Term::Literal(literal) => Ok(quote_atom(&format!(
            "L|{}",
            AtomicTerm::Literal(literal.clone()).key()
        ))),
        Term::SequenceMarker(name) => Err((
            "UNSUPPORTED_SEQUENCE_MARKER".into(),
            format!("sequence marker {name:?} cannot collapse to a first-order constant"),
        )),
        Term::App { symbol, args } => {
            let rendered: Result<Vec<_>, _> =
                args.iter().map(|arg| formula_term(arg, env)).collect();
            Ok(format!(
                "{}({})",
                function_symbol(symbol, args.len()),
                rendered?.join(",")
            ))
        }
    }
}

fn free_variables(formula: &Formula) -> Vec<String> {
    fn term(value: &Term, bound: &[String], free: &mut Vec<String>) {
        match value {
            Term::Var(name) if !bound.contains(name) && !free.contains(name) => {
                free.push(name.clone());
            }
            Term::App { args, .. } => {
                for arg in args {
                    term(arg, bound, free);
                }
            }
            Term::Var(_) | Term::Iri(_) | Term::Literal(_) | Term::SequenceMarker(_) => {}
        }
    }
    fn walk(formula: &Formula, bound: &mut Vec<String>, free: &mut Vec<String>) {
        match formula {
            Formula::Atom { relation, args } => {
                term(relation, bound, free);
                for arg in args {
                    term(arg, bound, free);
                }
            }
            Formula::Not(inner) => walk(inner, bound, free),
            Formula::And(parts) | Formula::Or(parts) => {
                for part in parts {
                    walk(part, bound, free);
                }
            }
            Formula::Implies(left, right) | Formula::Iff(left, right) => {
                walk(left, bound, free);
                walk(right, bound, free);
            }
            Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
                let previous = bound.len();
                bound.extend(vars.iter().cloned());
                walk(body, bound, free);
                bound.truncate(previous);
            }
        }
    }
    let mut free = Vec::new();
    walk(formula, &mut Vec::new(), &mut free);
    free
}

#[derive(Default)]
struct VariableEnv {
    bindings: Vec<(String, String)>,
    issued: BTreeSet<String>,
}

impl VariableEnv {
    fn push(&mut self, name: String) -> String {
        let base = variable_token(&name);
        let mut token = base.clone();
        let mut suffix = 1usize;
        while !self.issued.insert(token.clone()) {
            suffix += 1;
            token = format!("{base}_{suffix}");
        }
        self.bindings.push((name, token.clone()));
        token
    }

    fn pop(&mut self, name: &str) {
        let (bound, _) = self
            .bindings
            .pop()
            .expect("variable binding stack mirrors formula binders");
        assert_eq!(
            bound, name,
            "variable binding stack mirrors formula binders"
        );
    }

    fn resolve(&mut self, name: &str) -> String {
        self.bindings
            .iter()
            .rev()
            .find(|(bound, _)| bound == name)
            .map(|(_, token)| token.clone())
            .unwrap_or_else(|| self.free(name))
    }

    fn free(&mut self, name: &str) -> String {
        if let Some((_, token)) = self.bindings.iter().find(|(bound, _)| bound == name) {
            return token.clone();
        }
        self.push(name.to_owned())
    }

    fn close_all(&self, sentence: String) -> String {
        if self.bindings.is_empty() {
            sentence
        } else {
            format!(
                "(! [{}] : {sentence})",
                self.bindings
                    .iter()
                    .map(|(_, token)| token.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}

fn render_problem(admission: &SourceAdmission, sentences: &[FofSentence]) -> String {
    let mut lines = vec![
        "% GENERATED by gmeow; DO NOT EDIT.".to_owned(),
        "% Complete selected-source TPTP FOF projection.".to_owned(),
        format!("% profile: {}", admission.profile.id),
        format!("% source-digest: {}", admission.source_digest),
        format!("% program-digest: {}", admission.program_digest),
        format!("% selection-digest: {}", admission.selection_digest),
        format!("% sentences: {}", sentences.len()),
        String::new(),
    ];
    for sentence in sentences {
        lines.push(format!(
            "fof({}, {}, {}).",
            sentence.name, sentence.role, sentence.body
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn member_symbol() -> String {
    predicate_symbol("https://blackcatinformatics.ca/logic/instanceOf", 2)
}

fn predicate_symbol(iri: &str, arity: usize) -> String {
    quote_atom(&format!("P|{arity}|{iri}"))
}

fn function_symbol(iri: &str, arity: usize) -> String {
    quote_atom(&format!("F|{arity}|{iri}"))
}

fn iri_constant(iri: &str) -> String {
    quote_atom(&format!("I|{iri}"))
}

fn variable_token(name: &str) -> String {
    let mut out = String::from("V_");
    for byte in name.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    if name.is_empty() {
        out.push_str("00");
    }
    out
}

fn quote_atom(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '{' => out.push_str("u{7b};"),
            '}' => out.push_str("u{7d};"),
            ' '..='~' => out.push(character),
            other => {
                use std::fmt::Write as _;
                let _ = write!(out, "u{{{:x}}};", other as u32);
            }
        }
    }
    out.push('\'');
    out
}

fn missing(kind: &str, index: usize) -> (String, String) {
    (
        "ADMISSION_PROGRAM_INDEX_MISMATCH".into(),
        format!("admitted {kind} index {index} is absent from the bound program"),
    )
}

fn digest_bytes(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value);
    hasher.finalize().to_hex().to_string()
}
