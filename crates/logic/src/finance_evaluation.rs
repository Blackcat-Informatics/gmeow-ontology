// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Financial admission and result roles around the one native math execution.
//! Recipes and callable interfaces come from the caller's authenticated owner.

use crate::math_evaluation::{self, Evaluation, EvaluationContext};
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::shapes::{
    engine::validate_dataset,
    shapes::{Constraint, Shapes, Target},
};
use purrdf::{RdfDataset, TermValue};
use std::collections::{BTreeMap, BTreeSet};

mod source;
use source::Records;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const M: &str = "https://blackcatinformatics.ca/math/";
const L: &str = "https://blackcatinformatics.ca/logic/";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// Explicit finite exact-decimal financial operation selection.
pub const PROFILE: &str = "finance-exact-decimal-v1";

// Exact generated product identities, not merely class targets: losing a
// procedural product must never silently select declarative-only admission.
const PROCEDURAL_PRODUCTS: &[&str] = &[
    "Urn:gmeow:finance:constraint:nonnegative-period-countProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:currency-realmProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:FinancialPeriodRate-frameProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:FinancialPeriodCount-frameProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:FinancialReturnRatio-frameProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:HoldingAssetQuantity-frameProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:holding-asset-countProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:simple-interestRate-basisProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:simple-interestPeriodCount-basisProceduralConstraintShape",
    "Urn:gmeow:finance:constraint:return-currency-matchProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/FrameAxisTypeConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/FrameDimensionCountConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/FrameRealmPresentConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/FrameAxisPresentConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/math/ArgumentSlotIndexUniquenessConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/math/ArgumentSlotContiguityConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/math/ArgumentSlotIndexNonNegativeConstraintProceduralConstraintShape",
];

fn atom(subject: &str, predicate: &str, object: &str) -> Formula {
    Formula::Atom {
        relation: Term::Iri(predicate.into()),
        args: vec![Term::Iri(subject.into()), Term::Iri(object.into())],
    }
}

/// Complete read-only owner artifacts selected independently of the user's data.
pub struct FinanceOwner<'a> {
    /// Authenticated complete canonical owner dataset, including actual recipes.
    /// The native arithmetic program selects an explicit named operation-contract
    /// projection; it does not claim a complete ontology-native reasoning verdict.
    pub dataset: &'a RdfDataset,
    /// The actual generated owner projections, never authored runtime substitutes.
    pub shapes: &'a Shapes,
    /// Exact bundle or producer-selected owner identity, bound into native proof.
    pub identity: &'a str,
}

fn require_shapes(shapes: &Shapes) -> gmeow_errors::Result<()> {
    for class in [
        format!("{M}Quantity"),
        format!("{M}ApplicationExpression"),
        format!("{M}ArgumentSlot"),
        format!("{G}MonetaryAmount"),
        format!("{G}ReferenceFrame"),
        format!("{G}Measurement"),
        format!("{G}Observation"),
        format!("{M}SymbolReference"),
        format!("{G}SimpleInterestCalculation"),
        format!("{G}HoldingPeriodReturnCalculation"),
        format!("{G}FinancialPeriodRate"),
        format!("{G}FinancialPeriodCount"),
        format!("{G}FinancialReturnRatio"),
    ] {
        if !shapes.node_shapes.iter().any(|shape| {
            !shape.deactivated
                && shape.deactivated_when.is_empty()
                && shape
                    .targets
                    .iter()
                    .any(|target| matches!(target, Target::Class(iri) if iri.as_str() == class))
        }) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("authenticated owner lacks required generated shape target {class}"),
            ));
        }
    }
    for identity in PROCEDURAL_PRODUCTS {
        if !shapes.node_shapes.iter().any(|shape| {
            matches!(&shape.id, purrdf::shapes::term::Term::NamedNode(iri) if iri.as_str() == *identity)
                && !shape.deactivated && shape.deactivated_when.is_empty()
                && !shape.targets.is_empty()
                && shape.constraints.iter().any(|constraint| matches!(constraint, Constraint::Sparql { .. }))
        }) {
            return Err(crate::error::profile_result(PROFILE, format!("authenticated owner lacks active required procedural product {identity}")));
        }
    }
    Ok(())
}

fn validate(records: &Records, shapes: &Shapes, phase: &str) -> gmeow_errors::Result<()> {
    // The shared logic projection owner supplies the complete structural reader view.
    // Canonical axioms remain present; no independent classifier is implemented.
    let data = records.dataset(true)?;
    let report = validate_dataset(&data, shapes).map_err(|error| {
        crate::error::profile_result(PROFILE, format!("{phase} validation: {error}"))
    })?;
    if !report.results.is_empty() {
        return Err(crate::error::profile_result(
            PROFILE,
            format!(
                "{phase} violates the authenticated owner projections: {:?}",
                report.results
            ),
        ));
    }
    Ok(())
}

fn scalar_frame(records: &Records, frame: &str, realm: &str) -> gmeow_errors::Result<()> {
    records.require_type(frame, &format!("{G}ReferenceFrame"))?;
    if records.reference(frame, &format!("{G}frameKind"))? != format!("{G}frameKindScalar")
        || records.reference(frame, &format!("{G}frameRealm"))? != format!("{G}{realm}")
    {
        return Err(crate::error::profile_result(
            PROFILE,
            format!("{frame} requires scalar kind and {realm}; no frame conversion is implicit"),
        ));
    }
    // Cardinality/datatype/axis/hosting/determinacy obligations are also checked
    // against the complete actual ReferenceFrame projection, not a reduced copy.
    for property in ["dimensionCount", "determinacyModel", "requiresHost"] {
        records.one(frame, &format!("{G}{property}"))?;
    }
    if records.values(frame, &format!("{G}hasAxis")).len() != 1 {
        return Err(crate::error::profile_result(
            PROFILE,
            format!("{frame} requires exactly one axis"),
        ));
    }
    Ok(())
}

struct Signature {
    method: String,
    recipe: String,
    parameters: Vec<(String, String, String)>, // source predicate, value type, distinct symbol
    result_type: String,
}

fn signature(owner: &Records, method: String) -> gmeow_errors::Result<Signature> {
    if ![
        format!("{G}methodSimpleInterest"),
        format!("{G}methodHoldingPeriodReturn"),
    ]
    .contains(&method)
    {
        return Err(crate::error::profile_result(
            PROFILE,
            format!("unsupported financial method {method}"),
        ));
    }
    owner.require_type(&method, &format!("{L}CallableSignature"))?;
    owner.require_type(&method, &format!("{G}ObservationMethod"))?;
    let recipe = owner.reference(&method, &format!("{G}calculationExpression"))?;
    let mut parameters = vec![];
    let mut roles = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    for parameter in owner.list(&owner.reference(&method, &format!("{L}inputParameters"))?)? {
        owner.require_type(&parameter, &format!("{L}CallParameter"))?;
        let required = owner.one(&parameter, &format!("{L}parameterRequired"))?;
        let Term::Literal(required) = required else {
            return Err(crate::error::profile_result(
                PROFILE,
                "parameterRequired requires explicit true boolean",
            ));
        };
        let value = purrdf::xsd::parse_by_iri(&required.lexical_form, required.datatype_iri())
            .map_err(|error| crate::error::profile_result(PROFILE, error.to_string()))?;
        if !matches!(value, Some(purrdf::xsd::XsdValue::Boolean(true))) {
            return Err(crate::error::profile_result(
                PROFILE,
                "every selected financial input must be explicitly required",
            ));
        }
        let role = owner.reference(&parameter, &format!("{L}parameterPredicate"))?;
        let value_type = owner.reference(&parameter, &format!("{L}parameterValueType"))?;
        let symbol = owner.reference(&parameter, &format!("{M}hasMathematicalSymbol"))?;
        owner.require_type(&symbol, &format!("{M}MathematicalSymbol"))?;
        if !roles.insert(role.clone()) || !symbols.insert(symbol.clone()) {
            return Err(crate::error::profile_result(
                PROFILE,
                "conflicting role or symbol in authenticated callable signature",
            ));
        }
        parameters.push((role, value_type, symbol));
    }
    let outputs = owner.list(&owner.reference(&method, &format!("{L}outputParameters"))?)?;
    let [output] = outputs.as_slice() else {
        return Err(crate::error::profile_result(
            PROFILE,
            "financial signature requires exactly one output",
        ));
    };
    owner.require_type(output, &format!("{L}CallResult"))?;
    if owner.reference(output, &format!("{L}parameterPredicate"))?
        != format!("{G}observationResult")
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "financial output must use the canonical observationResult role",
        ));
    }
    let result_type = owner.reference(output, &format!("{L}parameterValueType"))?;
    if ![
        format!("{G}MonetaryAmount"),
        format!("{G}FinancialReturnRatio"),
    ]
    .contains(&result_type)
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "unsupported financial result type",
        ));
    }
    Ok(Signature {
        method,
        recipe,
        parameters,
        result_type,
    })
}

fn check_request(
    records: &Records,
    request: &str,
    signature: &Signature,
) -> gmeow_errors::Result<()> {
    let classes = [
        "SimpleInterestCalculation",
        "HoldingPeriodReturnCalculation",
    ];
    if classes
        .iter()
        .filter(|class| records.typed(request, &format!("{G}{class}")))
        .count()
        != 1
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "request requires exactly one supported concrete financial calculation type",
        ));
    }
    let mut allowed = BTreeSet::from([
        TYPE.to_owned(),
        format!("{G}vantage"),
        format!("{G}observedFeature"),
        format!("{G}observationMethod"),
        format!("{G}calculationExpression"),
        format!("{G}calculationPeriodBasis"),
        format!("{G}observedAt"),
    ]);
    allowed.extend(signature.parameters.iter().map(|(role, _, _)| role.clone()));
    for ((subject, predicate), _) in records.0.range((request.into(), String::new())..) {
        if subject != request {
            break;
        }
        if !allowed.contains(predicate) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "unsupported or precomputed request role {predicate}; only the selected signature and observation context are admitted"
                ),
            ));
        }
    }
    for property in [
        "vantage",
        "observedFeature",
        "observationMethod",
        "calculationExpression",
        "calculationPeriodBasis",
    ] {
        records.one(request, &format!("{G}{property}"))?;
    }
    Ok(())
}

struct BoundInputs {
    bindings: BTreeMap<String, String>,
    quantity_classes: BTreeSet<String>,
    currency: String,
}

fn bind_inputs(
    records: &Records,
    owner: &Records,
    request: &str,
    signature: &Signature,
    period: &str,
) -> gmeow_errors::Result<BoundInputs> {
    let mut bindings = BTreeMap::new();
    let mut classes = BTreeSet::new();
    let mut currencies = BTreeSet::new();
    for (role, value_type, symbol) in &signature.parameters {
        let quantity = records.reference(request, role)?;
        records.require_type(&quantity, value_type)?;
        // The owned axiom is retained as a source fact; the native program must
        // derive Quantity membership before its arithmetic premise can fire.
        if !gmeow_ns::SUB_CLASS_OF.iter().any(|predicate| {
            owner
                .values(value_type, predicate)
                .contains(&Term::Iri(format!("{M}Quantity")))
        }) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "authenticated owner does not ground {value_type} directly in math:Quantity"
                ),
            ));
        }
        classes.insert(value_type.clone());
        if value_type == &format!("{G}MonetaryAmount") {
            let currency = records.reference(&quantity, &format!("{G}currency"))?;
            scalar_frame(records, &currency, "frameRealmCurrency")?;
            for frame in records.values(&quantity, &format!("{G}hasReferenceFrame")) {
                if frame != &Term::Iri(currency.clone()) {
                    return Err(crate::error::profile_result(
                        PROFILE,
                        format!("{quantity}: currency conflicts with explicit reference frame"),
                    ));
                }
            }
            currencies.insert(currency);
        } else {
            if records.reference(&quantity, &format!("{G}hasReferenceFrame"))? != period {
                return Err(crate::error::profile_result(
                    PROFILE,
                    format!(
                        "{quantity}: rate/count period basis differs from calculationPeriodBasis"
                    ),
                ));
            }
        }
        bindings.insert(symbol.clone(), quantity);
    }
    if currencies.len() != 1 {
        return Err(crate::error::profile_result(
            PROFILE,
            "financial money inputs must share exactly one explicit currency frame",
        ));
    }
    Ok(BoundInputs {
        bindings,
        quantity_classes: classes,
        currency: currencies
            .into_iter()
            .next()
            .ok_or_else(|| crate::error::profile_result(PROFILE, "missing admitted currency"))?,
    })
}

struct Admission {
    records: Records,
    signature: Signature,
    period: String,
    expression: String,
    inputs: BoundInputs,
    canonical: String,
}

fn admit(
    source: &RdfDataset,
    owner: &FinanceOwner<'_>,
    request: &str,
) -> gmeow_errors::Result<Admission> {
    if owner.identity.is_empty() {
        return Err(crate::error::profile_result(
            PROFILE,
            "authenticated owner identity is required",
        ));
    }
    require_shapes(owner.shapes)?;
    let trusted = Records::read(owner.dataset, true)?;
    let mut records = Records::read(source, false)?;
    let retired = format!("{G}monetaryValue");
    if records.0.iter().any(|((subject, predicate), values)| {
        subject == &retired || predicate == &retired || values.contains(&Term::Iri(retired.clone()))
    }) {
        return Err(crate::error::profile_result(
            PROFILE,
            "retired monetaryValue declaration or usage; use canonical math:quantityValue on MonetaryAmount",
        ));
    }
    let signature = signature(
        &trusted,
        records.reference(request, &format!("{G}observationMethod"))?,
    )?;
    check_request(&records, request, &signature)?;
    let period = records.reference(request, &format!("{G}calculationPeriodBasis"))?;
    let expression = records.reference(request, &format!("{G}calculationExpression"))?;
    let mut roots = vec![
        signature.method.clone(),
        expression.clone(),
        period.clone(),
        format!("{G}currency"),
    ];
    roots.extend(
        signature
            .parameters
            .iter()
            .map(|(_, class, _)| class.clone()),
    );
    // Resolve every referenced owned seed (dimensions, axes, category values),
    // while leaving user individuals and their original facts untouched.
    roots.extend(records.0.values().flatten().filter_map(|value| {
        if let Term::Iri(iri) = value {
            Some(iri.clone())
        } else {
            None
        }
    }));
    records.admit_owner(&trusted, roots)?;
    scalar_frame(&records, &period, "frameRealmTemporal")?;
    let inputs = bind_inputs(&records, &trusted, request, &signature, &period)?;
    validate(&records, owner.shapes, "input")?;
    let effective = records.dataset(false)?;
    let selected = crate::math_expression::analysis::structural_key_at(&effective, &expression)
        .map_err(|error| crate::error::profile_result(PROFILE, error.to_string()))?;
    let canonical =
        crate::math_expression::analysis::structural_key_at(owner.dataset, &signature.recipe)
            .map_err(|error| {
                crate::error::profile_result(PROFILE, format!("authenticated recipe: {error}"))
            })?;
    if selected != canonical {
        return Err(crate::error::profile_result(
            PROFILE,
            "selected expression differs from the authenticated method recipe, operators, slot topology or symbol roles",
        ));
    }
    Ok(Admission {
        records,
        signature,
        period,
        expression,
        inputs,
        canonical,
    })
}

/// Admit one financial calculation and derive its result in one native execution.
///
/// # Errors
/// Fails closed for unavailable owner artifacts, malformed roles/frames/ASTs,
/// recipe mismatch, non-exact arithmetic, zero divisors and incomplete proof.
pub fn evaluate(
    source: &RdfDataset,
    owner: FinanceOwner<'_>,
    request: &str,
) -> gmeow_errors::Result<Evaluation> {
    let Admission {
        records,
        signature,
        period,
        expression,
        inputs,
        canonical,
    } = admit(source, &owner, request)?;
    let effective = records.dataset(false)?;
    let context = EvaluationContext {
        identity: format!(
            "{PROFILE}|{}|{request}|{}|{canonical}",
            owner.identity, signature.method
        ),
        premises: records.premises(),
        quantity_classes: inputs.quantity_classes,
    };
    let mut prepared =
        math_evaluation::prepare(&effective, &expression, &inputs.bindings, &context)?;
    // Keep the selected finance profile visible in the ordinary program receipt;
    // the shared preparation digest already binds the complete caller identity.
    prepared.program.source_iri = Some(format!(
        "urn:gmeow:{PROFILE}:{}",
        prepared
            .quantity
            .rsplit(':')
            .next()
            .ok_or_else(|| crate::error::profile_result(
                PROFILE,
                "missing prepared result identity"
            ))?
    ));
    let Formula::Implies(_, head) = &mut prepared.program.formulas[0] else {
        unreachable!("shared arithmetic preparation yields one implication")
    };
    let Formula::And(conclusions) = head.as_mut() else {
        unreachable!("shared arithmetic result is conjunctive")
    };
    conclusions.push(atom(&prepared.quantity, TYPE, &signature.result_type));
    if signature.result_type == format!("{G}MonetaryAmount") {
        conclusions.push(atom(
            &prepared.quantity,
            &format!("{G}currency"),
            &inputs.currency,
        ));
    } else {
        conclusions.push(atom(
            &prepared.quantity,
            &format!("{G}hasReferenceFrame"),
            &period,
        ));
    }
    conclusions.push(atom(
        request,
        &format!("{G}observationResult"),
        &prepared.quantity,
    ));
    let evaluation = math_evaluation::execute_prepared(&effective, prepared)?;
    let closure = crate::reason::native_closure_to_dataset(&evaluation.result)?;
    validate(
        &Records::read(&closure, false)?,
        owner.shapes,
        "derived output",
    )?;
    if !evaluation.result.inferred().iter().any(|row| {
        !row.is_edb
            && row.subject == request
            && row.predicate == format!("{G}observationResult")
            && row.object == TermValue::iri(&evaluation.quantity)
    }) {
        return Err(crate::error::profile_result(
            PROFILE,
            "native execution did not derive the requested observationResult",
        ));
    }
    Ok(evaluation)
}

#[cfg(test)]
mod tests;
