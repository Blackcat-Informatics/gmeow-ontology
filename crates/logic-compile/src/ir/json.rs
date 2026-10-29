// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native canonical AST transport with checked term/atom admission.
use super::{
    AtomicTerm, ComplexityClass, Formula, LogicModality, PreservationKind, ReasoningContract,
    SemanticProfileId, Term, literal_serde,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within, items_with},
};

impl ToJson for SemanticProfileId {
    fn to_json(&self) -> Value {
        match self {
            Self::PositiveHorn => "PositiveHorn",
            Self::StratifiedNaf => "StratifiedNaf",
            Self::WellFounded => "WellFounded",
            Self::StableModel => "StableModel",
            Self::ProceduralProlog => "ProceduralProlog",
            Self::Probabilistic => "Probabilistic",
        }
        .to_json()
    }
}

impl FromJson for SemanticProfileId {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "SemanticProfileId")? {
            "PositiveHorn" => Ok(Self::PositiveHorn),
            "StratifiedNaf" => Ok(Self::StratifiedNaf),
            "WellFounded" => Ok(Self::WellFounded),
            "StableModel" => Ok(Self::StableModel),
            "ProceduralProlog" => Ok(Self::ProceduralProlog),
            "Probabilistic" => Ok(Self::Probabilistic),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "PositiveHorn",
                    "StratifiedNaf",
                    "WellFounded",
                    "StableModel",
                    "ProceduralProlog",
                    "Probabilistic",
                ],
            )),
        }
    }
}

impl ToJson for LogicModality {
    fn to_json(&self) -> Value {
        match self {
            Self::None => "None",
            Self::Alethic => "Alethic",
            Self::Epistemic => "Epistemic",
            Self::Doxastic => "Doxastic",
            Self::Telic => "Telic",
            Self::Deontic => "Deontic",
            Self::Representational => "Representational",
            Self::Counterfactual => "Counterfactual",
        }
        .to_json()
    }
}

::purrdf_lex::json_record!(impl ToJson for ComplexityClass {
    "label" => label,
});

impl FromJson for ComplexityClass {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ComplexityClass")?;
        // This is retained metadata, not a newly authored complexity declaration.
        Ok(Self {
            label: fields.required("label")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for ReasoningContract {
    "preset" => preset,
    "formula_fragment" => formula_fragment,
    "model_semantics" => model_semantics,
    "truth_algebra" => truth_algebra,
    "admissible_valuation" => admissible_valuation,
    "designated_values" => designated_values,
    "evolution" => evolution,
    "argumentation" => argumentation,
    "revision" => revision,
    "equality_policy" => equality_policy,
    "default_closure" => default_closure,
    "negation_operators" => negation_operators,
    "context_axes" => context_axes,
    "uncertainty_measures" => uncertainty_measures,
    "resource_policies" => resource_policies,
    "projection_targets" => projection_targets,
    "closure_entries" => closure_entries,
    "complexity" => complexity,
});

impl FromJson for ReasoningContract {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ReasoningContract")?;
        Ok(Self {
            preset: fields.optional("preset")?,
            formula_fragment: fields.optional("formula_fragment")?,
            model_semantics: fields.optional("model_semantics")?,
            truth_algebra: fields.optional("truth_algebra")?,
            admissible_valuation: fields.optional("admissible_valuation")?,
            designated_values: fields.optional("designated_values")?,
            evolution: fields.optional("evolution")?,
            argumentation: fields.optional("argumentation")?,
            revision: fields.optional("revision")?,
            equality_policy: fields.optional("equality_policy")?,
            default_closure: fields.optional("default_closure")?,
            negation_operators: fields.required("negation_operators")?,
            context_axes: fields.required("context_axes")?,
            uncertainty_measures: fields.required("uncertainty_measures")?,
            resource_policies: fields.required("resource_policies")?,
            projection_targets: fields.required("projection_targets")?,
            closure_entries: fields.required("closure_entries")?,
            complexity: fields.optional("complexity")?,
        })
    }
}

// Atomic inspection values retain their explicit kind and verbatim payload.
impl ToJson for AtomicTerm {
    fn to_json(&self) -> Value {
        match self {
            Self::Var(value) => Object::new().with("Var", value.to_json()).into(),
            Self::Iri(value) => Object::new().with("Iri", value.to_json()).into(),
            Self::Blank(value) => Object::new().with("Blank", value.to_json()).into(),
            Self::Literal(value) => Object::new()
                .with("Literal", literal_serde::single::to_json(value))
                .into(),
        }
    }
}
impl FromJson for AtomicTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "AtomicTerm", "one AtomicTerm variant")?;
        let decoded = match tag {
            "Var" => String::from_json(payload).map(Self::Var),
            "Iri" => String::from_json(payload).map(Self::Iri),
            "Blank" => String::from_json(payload).map(Self::Blank),
            "Literal" => literal_serde::single::from_json(payload).map(Self::Literal),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Var", "Iri", "Blank", "Literal"],
            )),
        };
        decoded.map_err(|error| error.within(tag))
    }
}

impl ToJson for Term {
    fn to_json(&self) -> Value {
        match self {
            Self::Var(value) => Object::new().with("Var", value.to_json()).into(),
            Self::Iri(value) => Object::new().with("Iri", value.to_json()).into(),
            Self::Literal(value) => Object::new()
                .with("Literal", literal_serde::single::to_json(value))
                .into(),
            Self::SequenceMarker(value) => {
                Object::new().with("SequenceMarker", value.to_json()).into()
            }
            Self::App { symbol, args } => Object::new()
                .with(
                    "App",
                    Value::from(
                        Object::new()
                            .with("symbol", symbol.to_json())
                            .with("args", args.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for Term {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_term::<Checked>(value)
    }
}

trait AstAdmission {
    fn term(term: Term) -> Result<Term, DecodeError>;
    fn atom(relation: Term, args: Vec<Term>) -> Result<Formula, DecodeError>;
}

struct Checked;
struct Observed;

impl AstAdmission for Checked {
    fn term(term: Term) -> Result<Term, DecodeError> {
        match term {
            Term::Var(name) => Term::var(name),
            Term::Iri(iri) => Term::iri(iri),
            Term::Literal(literal) => Term::rdf_literal(literal),
            Term::SequenceMarker(name) => Term::sequence_marker(name),
            Term::App { symbol, args } => Term::app(symbol, args),
        }
        .map_err(DecodeError::custom)
    }

    fn atom(relation: Term, args: Vec<Term>) -> Result<Formula, DecodeError> {
        Formula::atom(relation, args).map_err(DecodeError::custom)
    }
}

impl AstAdmission for Observed {
    fn term(term: Term) -> Result<Term, DecodeError> {
        Ok(term)
    }

    fn atom(relation: Term, args: Vec<Term>) -> Result<Formula, DecodeError> {
        Ok(Formula::Atom { relation, args })
    }
}

fn read_term<P: AstAdmission>(value: &Value) -> Result<Term, DecodeError> {
    let (tag, payload) = gmeow_errors::json::read_variant(value, "Term", "one Term variant")?;
    let decode = || -> Result<Term, DecodeError> {
        let term = match tag {
            "Var" => Term::Var(String::from_json(payload)?),
            "Iri" => Term::Iri(String::from_json(payload)?),
            "Literal" => Term::Literal(literal_serde::single::from_json(payload)?),
            "SequenceMarker" => Term::SequenceMarker(String::from_json(payload)?),
            "App" => {
                let mut fields = Record::new(payload, "struct variant Term::App")?;
                Term::App {
                    symbol: fields.required("symbol")?,
                    args: fields
                        .required_with("args", |value| items_with(value, read_term::<P>))?,
                }
            }
            name => {
                return Err(DecodeError::unknown_variant(
                    name,
                    &["Var", "Iri", "Literal", "SequenceMarker", "App"],
                ));
            }
        };
        P::term(term)
    };
    decode().map_err(|error| error.within(tag))
}
impl ToJson for Formula {
    fn to_json(&self) -> Value {
        match self {
            Self::Atom { relation, args } => Object::new()
                .with(
                    "Atom",
                    Value::from(
                        Object::new()
                            .with("relation", relation.to_json())
                            .with("args", args.to_json()),
                    ),
                )
                .into(),
            Self::Not(body) => Object::new().with("Not", body.to_json()).into(),
            Self::And(body) => Object::new().with("And", body.to_json()).into(),
            Self::Or(body) => Object::new().with("Or", body.to_json()).into(),
            Self::Implies(left, right) => Object::new()
                .with("Implies", (left, right).to_json())
                .into(),
            Self::Iff(left, right) => Object::new().with("Iff", (left, right).to_json()).into(),
            Self::Forall { vars, body } => Object::new()
                .with(
                    "Forall",
                    Value::from(
                        Object::new()
                            .with("vars", vars.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
            Self::Exists { vars, body } => Object::new()
                .with(
                    "Exists",
                    Value::from(
                        Object::new()
                            .with("vars", vars.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for Formula {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_formula::<Checked>(value)
    }
}

impl Formula {
    /// Hydrate a retained observation, without authoring or restoring proof authority.
    /// Literal component admission remains shared with the checked AST reader.
    ///
    /// # Errors
    /// Retains typed member, variant, sequence and literal component refusals.
    pub(crate) fn from_observation_json(value: &Value) -> Result<Self, DecodeError> {
        read_formula::<Observed>(value)
    }
}

fn read_formula<P: AstAdmission>(value: &Value) -> Result<Formula, DecodeError> {
    let (tag, payload) = gmeow_errors::json::read_variant(value, "Formula", "one Formula variant")?;
    let decode = || -> Result<Formula, DecodeError> {
        match tag {
            "Atom" => {
                let mut fields = Record::new(payload, "struct variant Formula::Atom")?;
                let relation = fields.required_with("relation", read_term::<P>)?;
                let args =
                    fields.required_with("args", |value| items_with(value, read_term::<P>))?;
                P::atom(relation, args)
            }
            "Not" => read_formula::<P>(payload).map(Box::new).map(Formula::Not),
            "And" => items_with(payload, read_formula::<P>).map(Formula::And),
            "Or" => items_with(payload, read_formula::<P>).map(Formula::Or),
            "Implies" => {
                let (left, right) = binary_operands::<P>(payload)?;
                Ok(Formula::Implies(left, right))
            }
            "Iff" => {
                let (left, right) = binary_operands::<P>(payload)?;
                Ok(Formula::Iff(left, right))
            }
            "Forall" => {
                let mut fields = Record::new(payload, "struct variant Formula::Forall")?;
                Ok(Formula::Forall {
                    vars: fields.required("vars")?,
                    body: fields
                        .required_with("body", |value| read_formula::<P>(value).map(Box::new))?,
                })
            }
            "Exists" => {
                let mut fields = Record::new(payload, "struct variant Formula::Exists")?;
                Ok(Formula::Exists {
                    vars: fields.required("vars")?,
                    body: fields
                        .required_with("body", |value| read_formula::<P>(value).map(Box::new))?,
                })
            }
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Atom", "Not", "And", "Or", "Implies", "Iff", "Forall", "Exists",
                ],
            )),
        }
    };
    decode().map_err(|error| error.within(tag))
}
/// Admit exactly two ordered operands for the two binary AST constructors.
fn binary_operands<P: AstAdmission>(
    value: &Value,
) -> Result<(Box<Formula>, Box<Formula>), DecodeError> {
    let cells = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, "two Formula operands"))?;
    if cells.len() != 2 {
        return Err(DecodeError::invalid_value(
            value,
            "exactly two Formula operands",
        ));
    }
    let left = read_formula::<P>(&cells[0])
        .map(Box::new)
        .map_err(|error| error.within_index_native(0, None))?;
    let right = read_formula::<P>(&cells[1])
        .map(Box::new)
        .map_err(|error| error.within_index_native(1, None))?;
    Ok((left, right))
}
impl ToJson for PreservationKind {
    fn to_json(&self) -> Value {
        match self {
            Self::Exact => "Exact",
            Self::SoundUnder => "SoundUnder",
            Self::CompleteOver => "CompleteOver",
            Self::ValidationOnly => "ValidationOnly",
            Self::InconsistencyPreserving => "InconsistencyPreserving",
            Self::InconsistencyReflecting => "InconsistencyReflecting",
            Self::Unsupported => "Unsupported",
        }
        .to_json()
    }
}
impl FromJson for PreservationKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "PreservationKind")? {
            "Exact" => Ok(Self::Exact),
            "SoundUnder" => Ok(Self::SoundUnder),
            "CompleteOver" => Ok(Self::CompleteOver),
            "ValidationOnly" => Ok(Self::ValidationOnly),
            "InconsistencyPreserving" => Ok(Self::InconsistencyPreserving),
            "InconsistencyReflecting" => Ok(Self::InconsistencyReflecting),
            "Unsupported" => Ok(Self::Unsupported),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Exact",
                    "SoundUnder",
                    "CompleteOver",
                    "ValidationOnly",
                    "InconsistencyPreserving",
                    "InconsistencyReflecting",
                    "Unsupported",
                ],
            )),
        }
    }
}

#[cfg(test)]
mod tests;
