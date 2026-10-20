// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::frontend::{
    AdmittedSemanticUnit, CompiledTheory, PropertyCharacteristic, SemanticUnitDisposition,
    SourceAdmission, SourceAdmissionStatus, SourceAssertion, SourceClassDefinition,
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

/// One emitted sentence and every admitted source unit it represents. An empty
/// source-unit set identifies a law of the explicit semantic profile itself.
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
    let mut blockers = if admission.matches(theory) {
        admission_blockers(admission)
    } else {
        vec![FofProjectionBlocker {
            unit_id: None,
            code: "ADMISSION_THEORY_IDENTITY_MISMATCH".into(),
            reason: "projection requires an unchanged live compiler admission for this exact source and program; deserialized receipts require source re-admission".into(),
        }]
    };
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
            match bodies_for(payload, theory) {
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

    if blockers.is_empty() {
        match profile_semantic_laws() {
            Ok(laws) => {
                for law in laws {
                    bodies.entry(law).or_default();
                }
            }
            Err((code, reason)) => blockers.push(FofProjectionBlocker {
                unit_id: None,
                code,
                reason,
            }),
        }
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
) -> Result<Vec<String>, (String, String)> {
    match payload {
        AdmittedSemanticUnit::Assertion(assertion) => {
            assertion_body(assertion).map(|body| vec![body])
        }
        AdmittedSemanticUnit::ClassDefinition(definition) => {
            class_definition_body(definition).map(|body| vec![body])
        }
        AdmittedSemanticUnit::Formula { index } => theory
            .program()
            .formulas
            .get(*index)
            .ok_or_else(|| missing("formula", *index))
            .and_then(formula_body)
            .map(|body| vec![body]),
        AdmittedSemanticUnit::Rule { index } => theory
            .program()
            .rules
            .get(*index)
            .ok_or_else(|| missing("rule", *index))
            .and_then(rule_body)
            .map(|body| vec![body]),
        AdmittedSemanticUnit::PropertyCharacteristic(characteristic) => {
            property_characteristic_body(characteristic).map(|body| vec![body])
        }
    }
}

fn assertion_body(assertion: &SourceAssertion) -> Result<String, (String, String)> {
    let subject = constant_term(&assertion.subject)?;
    let object = constant_term(&assertion.object)?;
    match assertion.predicate.as_str() {
        RDFS_SUBPROPERTY
        | concat_logic!("subPropertyOf")
        | OWL_EQUIVALENT_PROPERTY
        | concat_logic!("equivalentProperty")
        | OWL_INVERSE_OF
        | concat_logic!("inverseOf") => {
            property_pair(assertion)?;
        }
        RDFS_DOMAIN | concat_logic!("domain") | RDFS_RANGE | concat_logic!("range") => {
            property_subject(assertion)?;
            require_class_resource(assertion)?;
        }
        _ => {}
    }
    Ok(relation_atom(&assertion.predicate, &[subject, object]))
}

/// Canonical extensional meaning, shared by direct assertions, formula atoms and
/// the interpretation laws for reified schema-predicate objects.
fn schema_constraint(predicate: &str, left: &str, right: &str) -> Option<String> {
    let binary = |property: &str, x: &str, y: &str| application_atom(property, &[x, y]);
    let member = member_atom;
    Some(match predicate {
        RDFS_SUBCLASS | concat_logic!("subClassOf") => format!(
            "(! [V_x] : ({} => {}))",
            member("V_x", left),
            member("V_x", right)
        ),
        OWL_EQUIVALENT_CLASS | concat_logic!("equivalentClass") => format!(
            "(! [V_x] : ({} <=> {}))",
            member("V_x", left),
            member("V_x", right)
        ),
        OWL_DISJOINT_WITH | concat_logic!("disjointWith") => format!(
            "(! [V_x] : (~ ({} & {})))",
            member("V_x", left),
            member("V_x", right)
        ),
        RDFS_SUBPROPERTY | concat_logic!("subPropertyOf") => format!(
            "(! [V_x,V_y] : ({} => {}))",
            binary(left, "V_x", "V_y"),
            binary(right, "V_x", "V_y")
        ),
        OWL_EQUIVALENT_PROPERTY | concat_logic!("equivalentProperty") => format!(
            "(! [V_x,V_y] : ({} <=> {}))",
            binary(left, "V_x", "V_y"),
            binary(right, "V_x", "V_y")
        ),
        OWL_INVERSE_OF | concat_logic!("inverseOf") => format!(
            "(! [V_x,V_y] : ({} <=> {}))",
            binary(left, "V_x", "V_y"),
            binary(right, "V_y", "V_x")
        ),
        RDFS_DOMAIN | concat_logic!("domain") => format!(
            "(! [V_x,V_y] : ({} => {}))",
            binary(left, "V_x", "V_y"),
            member("V_x", right)
        ),
        RDFS_RANGE | concat_logic!("range") => format!(
            "(! [V_x,V_y] : ({} => {}))",
            binary(left, "V_x", "V_y"),
            member("V_y", right)
        ),
        _ => return None,
    })
}

fn property_pair(assertion: &SourceAssertion) -> Result<(&str, &str), (String, String)> {
    let subject = property_subject(assertion)?;
    let AtomicTerm::Iri(object) = &assertion.object else {
        return Err((
            "NON_IRI_SCHEMA_OBJECT".into(),
            format!("{} requires an IRI property object", assertion.predicate),
        ));
    };
    Ok((subject, object))
}

fn property_subject(assertion: &SourceAssertion) -> Result<&str, (String, String)> {
    let AtomicTerm::Iri(subject) = &assertion.subject else {
        return Err((
            "NON_IRI_SCHEMA_SUBJECT".into(),
            format!(
                "{} requires an IRI property/class subject",
                assertion.predicate
            ),
        ));
    };
    Ok(subject)
}

fn require_class_resource(assertion: &SourceAssertion) -> Result<(), (String, String)> {
    if matches!(assertion.object, AtomicTerm::Iri(_) | AtomicTerm::Blank(_)) {
        Ok(())
    } else {
        Err((
            "NON_RESOURCE_SCHEMA_CLASS".into(),
            format!(
                "{} requires an IRI or blank class object",
                assertion.predicate
            ),
        ))
    }
}

fn property_characteristic_body(
    characteristic: &PropertyCharacteristic,
) -> Result<String, (String, String)> {
    let p = |left: &str, right: &str| binary_application(&characteristic.property, left, right);
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

fn rule_body(rule: &LogicRule) -> Result<String, (String, String)> {
    let mut variables = VariableEnv::default();
    let body: Result<Vec<_>, _> = rule
        .body
        .iter()
        .map(|atom| rule_atom(atom, &mut variables))
        .collect();
    let body = body?;
    let head = rule_atom(&rule.head, &mut variables)?;
    let sentence = if body.is_empty() {
        head
    } else {
        format!("(({}) => {head})", body.join(" & "))
    };
    Ok(variables.close_all(sentence))
}

fn rule_atom(atom: &LogicAxiom, variables: &mut VariableEnv) -> Result<String, (String, String)> {
    let subject = compact_term(&AtomicTerm::resource(atom.subject.clone()), variables)?;
    let object = compact_term(&atom.obj, variables)?;
    Ok(relation_atom(&atom.predicate, &[subject, object]))
}

fn formula_body(formula: &Formula) -> Result<String, (String, String)> {
    let free = free_variables(formula);
    let mut env = VariableEnv::default();
    let free_tokens: Vec<_> = free.iter().map(|name| env.push(name.clone())).collect();
    let body = render_formula(formula, &mut env)?;
    for name in free.iter().rev() {
        env.pop(name);
    }
    if free_tokens.is_empty() {
        Ok(body)
    } else {
        Ok(format!("(! [{}] : {body})", free_tokens.join(",")))
    }
}

fn class_definition_body(definition: &SourceClassDefinition) -> Result<String, (String, String)> {
    let free = free_variables(&definition.formula);
    if free.len() != definition.constants.len()
        || free
            .iter()
            .any(|name| !definition.constants.contains_key(name))
    {
        return Err((
            "INVALID_CLASS_CONSTANT_BINDINGS".into(),
            "a class formula requires an exact ground binding for each free term".into(),
        ));
    }
    let mut env = VariableEnv::default();
    for (name, value) in &definition.constants {
        env.bindings.push((name.clone(), constant_term(value)?));
    }
    render_formula(&definition.formula, &mut env)
}

fn render_formula(formula: &Formula, env: &mut VariableEnv) -> Result<String, (String, String)> {
    match formula {
        Formula::Atom { relation, args } => {
            let Term::Iri(relation_iri) = relation else {
                return Err((
                    "NON_IRI_FORMULA_RELATION".into(),
                    "a FOF relation must be an IRI".into(),
                ));
            };
            Formula::check_atom(relation, args).map_err(|error| {
                (
                    "MALFORMED_CONTEXTUAL_APPLICATION".into(),
                    error.message().into(),
                )
            })?;
            let args: Result<Vec<_>, _> = args.iter().map(|term| formula_term(term, env)).collect();
            Ok(relation_atom(relation_iri, &args?))
        }
        Formula::Not(inner) => Ok(format!("(~ {})", render_formula(inner, env)?)),
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
            let rendered: Result<Vec<_>, _> =
                parts.iter().map(|part| render_formula(part, env)).collect();
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
                render_formula(left, env)?,
                render_formula(right, env)?
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
            let body = render_formula(body, env)?;
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

fn relation_atom(relation: &str, args: &[String]) -> String {
    if args.len() == 2
        && let Some(meaning) = binary_meaning(relation, &args[0], &args[1])
    {
        return meaning;
    }
    application_atom(&iri_constant(relation), args)
}

fn binary_meaning(relation: &str, left: &str, right: &str) -> Option<String> {
    Some(match relation {
        RDF_TYPE | concat_logic!("instanceOf") => member_atom(left, right),
        OWL_SAME_AS | concat_logic!("sameAs") => format!("({left} = {right})"),
        OWL_DIFFERENT_FROM | concat_logic!("differentFrom") => format!("({left} != {right})"),
        _ => return schema_constraint(relation, left, right),
    })
}

/// The relation object is an ordinary first-order argument at every arity,
/// including zero-argument IR applications. Source grammar admission remains
/// separate. Equality therefore supplies congruence
/// without a quadratic catalogue of conditional predicate-pair axioms.
fn application_atom(property: &str, args: &[impl AsRef<str>]) -> String {
    let mut body = format!("'A|{}'({property}", args.len());
    for argument in args {
        body.push(',');
        body.push_str(argument.as_ref());
    }
    body.push(')');
    body
}

fn binary_application(property: &str, left: &str, right: &str) -> String {
    application_atom(&iri_constant(property), &[left, right])
}

fn member_atom(value: &str, class: &str) -> String {
    const RELATION: &str = concat!("'I|", concat_logic!("instanceOf"), "'");
    application_atom(RELATION, &[value, class])
}

fn profile_class_laws() -> Result<Vec<String>, (String, String)> {
    [
        (concat_logic!("Thing"), true),
        (concat_logic!("Nothing"), false),
        ("http://www.w3.org/2002/07/owl#Thing", true),
        ("http://www.w3.org/2002/07/owl#Nothing", false),
    ]
    .into_iter()
    .map(|(class, universal)| {
        let membership = Formula::Atom {
            relation: Term::Iri(concat_logic!("instanceOf").into()),
            args: vec![Term::Var("profile_member".into()), Term::Iri(class.into())],
        };
        formula_body(&Formula::Forall {
            vars: vec!["profile_member".into()],
            body: Box::new(if universal {
                membership
            } else {
                Formula::Not(Box::new(membership))
            }),
        })
    })
    .collect()
}

/// The applicative representation must agree with intrinsic operators, including
/// when an ordinary property object is equal to one of their reified names.
/// These fixed profile laws replace pairwise predicate-congruence expansion.
fn profile_semantic_laws() -> Result<Vec<String>, (String, String)> {
    let mut laws = profile_class_laws()?;
    let left = "V_profile_left";
    let right = "V_profile_right";
    let unary = application_atom(left, &[right]);
    let membership = member_atom(right, left);
    laws.push(format!("(! [{left},{right}] : ({unary} <=> {membership}))"));
    for relation in [
        RDF_TYPE,
        concat_logic!("sameAs"),
        OWL_SAME_AS,
        concat_logic!("differentFrom"),
        OWL_DIFFERENT_FROM,
        concat_logic!("subClassOf"),
        RDFS_SUBCLASS,
        concat_logic!("equivalentClass"),
        OWL_EQUIVALENT_CLASS,
        concat_logic!("disjointWith"),
        OWL_DISJOINT_WITH,
        concat_logic!("subPropertyOf"),
        RDFS_SUBPROPERTY,
        concat_logic!("equivalentProperty"),
        OWL_EQUIVALENT_PROPERTY,
        concat_logic!("inverseOf"),
        OWL_INVERSE_OF,
        concat_logic!("domain"),
        RDFS_DOMAIN,
        concat_logic!("range"),
        RDFS_RANGE,
    ] {
        let application = binary_application(relation, left, right);
        let meaning = binary_meaning(relation, left, right).ok_or_else(|| {
            (
                "PROFILE_OPERATOR_WITHOUT_MEANING".into(),
                format!("the required profile operator {relation} has no canonical interpretation"),
            )
        })?;
        laws.push(format!(
            "(! [{left},{right}] : ({application} <=> {meaning}))"
        ));
    }
    Ok(laws)
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

fn function_symbol(iri: &str, arity: usize) -> String {
    quote_atom(&format!("F|{arity}|{iri}"))
}

fn iri_constant(iri: &str) -> String {
    quote_atom(&format!("I|{iri}"))
}

fn variable_token(name: &str) -> String {
    let mut out = String::from("V_");
    purrdf_hash::hex::encode_into(name.as_bytes(), &mut out);
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
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value);
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}
