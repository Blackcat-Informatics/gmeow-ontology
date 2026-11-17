// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Financial admission and result roles around the one native math execution.
//! Recipes and callable interfaces come from the caller's authenticated owner.

use crate::math_evaluation::{self, Evaluation, EvaluationContext};
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::shapes::{
    engine::validate_dataset,
    shapes::{AnnotatedConstraint, Constraint, Shape, Shapes, Target},
};
use purrdf::{RdfDataset, TermValue};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;

mod source;
use source::Records;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const M: &str = "https://blackcatinformatics.ca/math/";
const L: &str = "https://blackcatinformatics.ca/logic/";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// Explicit finite exact-decimal financial operation selection.
pub const PROFILE: &str = "finance-exact-decimal-v1";
/// Canonical financial callable member shared by the bundle caller and fixture producer.
pub const CALLABLE_SOURCE_PATH: &str = "slices/extensions/finance/mappings/callable-signatures.ttl";
/// The selected callable member is bounded independently of the surrounding bundle.
pub const CALLABLE_SOURCE_MAX_BYTES: usize = 256 * 1024;

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
    "https://blackcatinformatics.ca/gmeow/UnitPriceFrameConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/UnitPriceUnitMatchConstraintProceduralConstraintShape",
    "https://blackcatinformatics.ca/gmeow/UnitPriceCurrencyMatchConstraintProceduralConstraintShape",
];

fn atom(subject: &str, predicate: &str, object: &str) -> Formula {
    Formula::Atom {
        relation: Term::Iri(predicate.into()),
        args: vec![Term::Iri(subject.into()), Term::Iri(object.into())],
    }
}

/// Complete read-only owner artifacts selected independently of the user's data.
pub struct FinanceOwner<'a, D: purrdf::DatasetView<ReadError = Infallible> = RdfDataset> {
    /// Authenticated complete canonical owner dataset, including actual recipes.
    /// The native arithmetic program selects an explicit named operation-contract
    /// projection; it does not claim a complete ontology-native reasoning verdict.
    pub dataset: &'a D,
    /// The actual generated owner projections, never authored runtime substitutes.
    pub shapes: &'a Shapes,
    /// Exact bundle or producer-selected owner identity, bound into native proof.
    pub identity: &'a str,
}

// Exported roots retain targets, while the owning native definition retains
// constraint bodies and conditional deactivation. Both must be active, and the
// selected root must actually delegate to its own definition.
fn active_definition(shape: &Shape) -> Option<&Shape> {
    if shape.deactivated
        || !shape.deactivated_when.is_empty()
        || !shape
            .constraints
            .iter()
            .any(|constraint| matches!(constraint, Constraint::Reference(id) if id == &shape.id))
    {
        return None;
    }
    let definition = shape.owner.as_ref()?.definition(&shape.id)?;
    if definition.deactivated || !definition.deactivated_when.is_empty() {
        return None;
    }
    Some(definition)
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
        format!("{M}RatioQuantity"),
        format!("{M}QuotientReferenceFrame"),
        format!("{G}UnitPrice"),
        format!("{G}HoldingAssetQuantity"),
        format!("{G}UnitPriceTotalCalculation"),
    ] {
        if !shapes.node_shapes.iter().any(|shape| {
            active_definition(shape).is_some()
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
            if !matches!(&shape.id, purrdf::shapes::term::Term::NamedNode(iri) if iri.as_str() == *identity)
                || shape.targets.is_empty()
            {
                return false;
            }
            let Some(definition) = active_definition(shape) else {
                return false;
            };
            let mut has_sparql = false;
            for (index, constraint) in definition.constraints.iter().enumerate() {
                if matches!(constraint, Constraint::Sparql { .. }) {
                    has_sparql = true;
                    if definition.constraint_annotations.iter().any(|annotation| {
                        annotation.constraint == AnnotatedConstraint::Constraint(index)
                            && !annotation.deactivated_when.is_empty()
                    }) {
                        return false;
                    }
                }
            }
            has_sparql
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
    scalar_frame_profile(records, frame, realm)
}

fn scalar_frame_profile(records: &Records, frame: &str, realm: &str) -> gmeow_errors::Result<()> {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    SimpleInterest,
    HoldingPeriodReturn,
    UnitPriceTotal,
}

impl Operation {
    fn from_method(method: &str) -> gmeow_errors::Result<Self> {
        match method.strip_prefix(G) {
            Some("methodSimpleInterest") => Ok(Self::SimpleInterest),
            Some("methodHoldingPeriodReturn") => Ok(Self::HoldingPeriodReturn),
            Some("methodUnitPriceTotal") => Ok(Self::UnitPriceTotal),
            _ => Err(crate::error::profile_result(
                PROFILE,
                format!("unsupported financial method {method}"),
            )),
        }
    }

    fn request_class(self) -> &'static str {
        match self {
            Self::SimpleInterest => "SimpleInterestCalculation",
            Self::HoldingPeriodReturn => "HoldingPeriodReturnCalculation",
            Self::UnitPriceTotal => "UnitPriceTotalCalculation",
        }
    }
}

enum FrameContext {
    Period(String),
    PriceCurrency(String),
}

struct Signature {
    operation: Operation,
    method: String,
    recipe: String,
    parameters: Vec<(String, String, String)>, // source predicate, value type, distinct symbol
    result_type: String,
}

fn signature(owner: &Records, method: String) -> gmeow_errors::Result<Signature> {
    let operation = Operation::from_method(&method)?;
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
    let expected_result = if operation == Operation::HoldingPeriodReturn {
        format!("{G}FinancialReturnRatio")
    } else {
        format!("{G}MonetaryAmount")
    };
    if result_type != expected_result {
        return Err(crate::error::profile_result(
            PROFILE,
            "unsupported financial result type",
        ));
    }
    Ok(Signature {
        operation,
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
        "UnitPriceTotalCalculation",
    ];
    if classes
        .iter()
        .filter(|class| records.typed(request, &format!("{G}{class}")))
        .count()
        != 1
        || !records.typed(
            request,
            &format!("{G}{}", signature.operation.request_class()),
        )
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
        format!("{G}observedAt"),
    ]);
    let context_role = if signature.operation == Operation::UnitPriceTotal {
        "hasReferenceFrame"
    } else {
        "calculationPeriodBasis"
    };
    allowed.insert(format!("{G}{context_role}"));
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
        context_role,
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

fn require_quantity_owner(owner: &Records, class: &str) -> gmeow_errors::Result<()> {
    let quantity = format!("{M}Quantity");
    let parent = if class == format!("{G}UnitPrice") {
        format!("{M}RatioQuantity")
    } else {
        quantity.clone()
    };
    let mut edges = vec![(class, parent.as_str())];
    if parent != quantity {
        edges.push((parent.as_str(), quantity.as_str()));
    }
    for (child, parent) in edges {
        if !gmeow_ns::SUB_CLASS_OF.iter().any(|predicate| {
            owner
                .values(child, predicate)
                .contains(&Term::Iri(parent.into()))
        }) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "authenticated owner lacks required Quantity ancestry edge {child} to {parent}"
                ),
            ));
        }
    }
    Ok(())
}

fn bind_period_inputs(
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
        require_quantity_owner(owner, value_type)?;
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

fn unit_mismatch(price: &str, quantity: &str, detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::UnitPriceUnitMismatch {
        price: price.into(),
        quantity: quantity.into(),
        detail: detail.into(),
    })
}

fn bind_unit_price_inputs(
    records: &Records,
    owner: &Records,
    request: &str,
    signature: &Signature,
    currency: &str,
) -> gmeow_errors::Result<BoundInputs> {
    if signature.parameters.len() != 2 {
        return Err(crate::error::profile_result(
            PROFILE,
            "unit-price signature requires its price and counted-quantity inputs",
        ));
    }
    let price = records.reference(request, &format!("{G}unitPrice"))?;
    let count = records.reference(request, &format!("{G}pricedQuantity"))?;
    let mut bindings = BTreeMap::new();
    let mut classes = BTreeSet::new();
    for (role, class, quantity) in [
        ("unitPrice", "UnitPrice", &price),
        ("pricedQuantity", "HoldingAssetQuantity", &count),
    ] {
        let value_type = format!("{G}{class}");
        let (_, declared_type, symbol) = signature
            .parameters
            .iter()
            .find(|(predicate, _, _)| predicate == &format!("{G}{role}"))
            .ok_or_else(|| {
                crate::error::profile_result(
                    PROFILE,
                    format!("unit-price signature lacks required {role} input"),
                )
            })?;
        if declared_type != &value_type {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("unit-price input {role} requires owned type {value_type}"),
            ));
        }
        if role == "pricedQuantity" && !records.typed(quantity, &value_type) {
            return Err(unit_mismatch(
                &price,
                &count,
                "the selected multiplication requires an explicit HoldingAssetQuantity, never a mass or bare scalar",
            ));
        }
        records.require_type(quantity, &value_type)?;
        require_quantity_owner(owner, &value_type)?;
        classes.insert(value_type);
        bindings.insert(symbol.clone(), quantity.clone());
    }
    if records.typed(&price, &format!("{G}MonetaryAmount"))
        || !records.values(&price, &format!("{G}currency")).is_empty()
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "a UnitPrice is a ratio quantity; its currency is the numerator frame, never a MonetaryAmount currency assertion",
        ));
    }
    let frame = records.reference(&price, &format!("{G}hasReferenceFrame"))?;
    records.require_type(&frame, &format!("{M}QuotientReferenceFrame"))?;
    scalar_frame_profile(records, &frame, "frameRealmMathematical")?;
    if records.reference(&frame, &format!("{G}determinacyModel"))? != format!("{G}determinacyCrisp")
        || records.values(&frame, &format!("{G}hasAxis")) != [Term::Iri(format!("{G}axisScalar"))]
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "unit-price quotient frame requires the crisp scalar-axis profile",
        ));
    }
    let Term::Literal(host) = records.one(&frame, &format!("{G}requiresHost"))? else {
        return Err(crate::error::profile_result(
            PROFILE,
            "unit-price quotient requiresHost must be false",
        ));
    };
    let host =
        purrdf::xsd::parse_by_iri(&host.lexical_form, host.datatype_iri()).map_err(|error| {
            crate::error::profile_result(
                PROFILE,
                format!("unit-price quotient requiresHost: {error}"),
            )
        })?;
    if !matches!(host, Some(purrdf::xsd::XsdValue::Boolean(false))) {
        return Err(crate::error::profile_result(
            PROFILE,
            "unit-price quotient requiresHost must be false",
        ));
    }
    let numerator = records.reference(&frame, &format!("{M}numeratorFrame"))?;
    let denominator = records.reference(&frame, &format!("{M}denominatorFrame"))?;
    scalar_frame(records, &numerator, "frameRealmCurrency")?;
    scalar_frame(records, &denominator, "frameRealmMathematical")?;
    if numerator != currency {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::UnitPriceCurrencyMismatch {
                price: price.clone(),
                quoted: numerator,
                requested: currency.into(),
            },
        ));
    }
    if records.reference(&count, &format!("{G}hasReferenceFrame"))? != denominator {
        return Err(unit_mismatch(
            &price,
            &count,
            "count frame differs from the quotient's denominator frame",
        ));
    }
    if records.reference(&count, &format!("{M}hasDimension"))? != format!("{M}dimensionless") {
        return Err(unit_mismatch(
            &price,
            &count,
            "the selected count must be physically dimensionless, not a mass or other dimensioned quantity",
        ));
    }
    if records.reference(&price, &format!("{G}pricedAsset"))?
        != records.reference(&count, &format!("{G}countedAsset"))?
    {
        return Err(unit_mismatch(
            &price,
            &count,
            "priced and counted asset identities differ",
        ));
    }
    Ok(BoundInputs {
        bindings,
        quantity_classes: classes,
        currency: currency.into(),
    })
}

struct Admission {
    records: Records,
    signature: Signature,
    context: FrameContext,
    expression: String,
    inputs: BoundInputs,
    canonical: String,
}

fn admit<D: purrdf::DatasetView<ReadError = Infallible>>(
    source: &RdfDataset,
    owner: &FinanceOwner<'_, D>,
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
    let context = if signature.operation == Operation::UnitPriceTotal {
        FrameContext::PriceCurrency(records.reference(request, &format!("{G}hasReferenceFrame"))?)
    } else {
        FrameContext::Period(records.reference(request, &format!("{G}calculationPeriodBasis"))?)
    };
    let expression = records.reference(request, &format!("{G}calculationExpression"))?;
    let mut roots = vec![
        signature.method.clone(),
        expression.clone(),
        match &context {
            FrameContext::Period(frame) | FrameContext::PriceCurrency(frame) => frame.clone(),
        },
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
    let inputs = match &context {
        FrameContext::Period(period) => {
            scalar_frame(&records, period, "frameRealmTemporal")?;
            bind_period_inputs(&records, &trusted, request, &signature, period)?
        }
        FrameContext::PriceCurrency(currency) => {
            scalar_frame(&records, currency, "frameRealmCurrency")?;
            bind_unit_price_inputs(&records, &trusted, request, &signature, currency)?
        }
    };
    validate(&records, owner.shapes, "input")?;
    let effective = records.dataset(false)?;
    let selected = crate::math_expression::analysis::structural_key_at(&effective, &expression)
        .map_err(|error| crate::error::profile_result(PROFILE, error.to_string()))?;
    let mut recipe = Records::default();
    recipe.admit_owner(&trusted, [signature.recipe.clone()])?;
    let recipe = recipe.dataset(false)?;
    let canonical = crate::math_expression::analysis::structural_key_at(&recipe, &signature.recipe)
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
        context,
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
pub fn evaluate<D: purrdf::DatasetView<ReadError = Infallible>>(
    source: &RdfDataset,
    owner: FinanceOwner<'_, D>,
    request: &str,
) -> gmeow_errors::Result<Evaluation> {
    let Admission {
        records,
        signature,
        context,
        expression,
        inputs,
        canonical,
    } = admit(source, &owner, request)?;
    let effective = records.dataset(false)?;
    let execution_context = EvaluationContext {
        identity: format!(
            "{PROFILE}|{}|{request}|{}|{canonical}",
            owner.identity, signature.method
        ),
        premises: records.premises(),
        quantity_classes: inputs.quantity_classes,
    };
    let mut prepared = math_evaluation::prepare(
        &effective,
        &expression,
        &inputs.bindings,
        &execution_context,
    )?;
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
        let FrameContext::Period(period) = &context else {
            return Err(crate::error::profile_result(
                PROFILE,
                "authenticated nonmonetary result requires a temporal calculation context",
            ));
        };
        conclusions.push(atom(
            &prepared.quantity,
            &format!("{G}hasReferenceFrame"),
            period,
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
