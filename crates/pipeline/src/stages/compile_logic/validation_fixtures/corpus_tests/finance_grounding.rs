// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consume authenticated producer records; never compile or produce an ontology.

use gmeow_logic::finance_evaluation::{FinanceOwner, evaluate};
use purrdf::shapes::engine::parse_shapes;

fn example() -> &'static str {
    let bytes = super::fixtures()
        .inputs
        .get("slices/extensions/finance/examples/exact-computations.ttl")
        .expect("authenticated producer selected the exact financial example input");
    std::str::from_utf8(bytes).expect("producer-selected example is UTF-8")
}

fn owner() -> (
    std::sync::Arc<purrdf::RdfDataset>,
    purrdf::shapes::shapes::Shapes,
    String,
) {
    let mut quads = Vec::new();
    let mut shapes = String::new();
    let mut identity = purrdf_hash::blake3::Hasher::new();
    for path in [
        "slices/extensions/finance/module.ttl",
        "slices/core/observations/module.ttl",
        "slices/grounding/math/module.ttl",
        "slices/core/places/module.ttl",
    ] {
        let product = super::module(path);
        identity.update(product.source_rdf.as_bytes());
        let source = purrdf::parse_dataset(product.source_rdf.as_bytes(), "application/trig", None)
            .expect("authenticated complete producer source");
        quads.extend(purrdf::flat_rdf_quads_from_dataset(&source));
        for shape in &product.shapes {
            identity.update(shape.shacl.as_bytes());
            shapes.push_str(&shape.shacl);
        }
        identity.update(product.procedural_shacl.as_bytes());
        shapes.push_str(&product.procedural_shacl);
    }
    (
        purrdf::native_quads::flat_dataset_from_quads(&quads).expect("retained owner records"),
        parse_shapes(&shapes, None).expect("complete actual generated financial profiles"),
        format!(
            "blake3:{}",
            purrdf_hash::hex::encode(identity.finalize().as_bytes())
        ),
    )
}

#[test]
fn finance_grounding_actual_owned_recipes_derive_interest_and_return() {
    let (dataset, shapes, identity) = owner();
    let source = purrdf::parse_dataset(example().as_bytes(), "text/turtle", None).unwrap();
    for (subject, expected) in [
        ("urn:finance:interest", "100"),
        ("urn:finance:return", "0.2"),
    ] {
        let evaluated = evaluate(
            &source,
            FinanceOwner {
                dataset: &dataset,
                shapes: &shapes,
                identity: &identity,
            },
            subject,
        )
        .unwrap_or_else(|error| panic!("{subject}: {error}"));
        let value = evaluated
            .result
            .inferred()
            .iter()
            .find(|row| {
                !row.is_edb
                    && row.subject == evaluated.quantity
                    && row.predicate == "https://blackcatinformatics.ca/math/quantityValue"
            })
            .expect("fresh native exact financial result");
        let purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            ..
        } = &value.object
        else {
            panic!("numeric result literal")
        };
        let actual = purrdf::xsd::parse_by_iri(lexical_form, datatype)
            .unwrap()
            .unwrap();
        let expected =
            purrdf::xsd::parse_by_iri(expected, "http://www.w3.org/2001/XMLSchema#decimal")
                .unwrap()
                .unwrap();
        assert!(
            purrdf::xsd::numeric_cmp(&actual, &expected)
                .unwrap()
                .is_eq()
        );
        let proof = gmeow_logic::result_rdf::project_reasoning_result(&evaluated.result).unwrap();
        assert!(proof.contains("subClassOf"));
        assert!(proof.contains("observationResult"));
        assert!(proof.contains("parameterPredicate"));
    }
}

#[test]
fn finance_grounding_actual_owner_refuses_bad_admission_and_computation() {
    let (dataset, shapes, identity) = owner();
    for (name, from, to) in [
        (
            "wrong monetary dimension",
            "m:hasDimension m:dimensionless",
            "m:hasDimension m:length",
        ),
        ("missing exact amount", "m:quantityValue 1000.00 ;", ""),
        (
            "float amount",
            "m:quantityValue 1000.00",
            "m:quantityValue \"1000\"^^xsd:double",
        ),
        (
            "currency mismatch",
            "m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameUSD",
            "m:quantityValue 1200.00 ; m:hasDimension m:dimensionless ; g:currency g:referenceFrameEUR",
        ),
        (
            "missing determinacy",
            "g:determinacyModel g:determinacyCrisp ;",
            "",
        ),
        (
            "wrong axis count",
            "g:dimensionCount \"1\"^^xsd:nonNegativeInteger",
            "g:dimensionCount \"2\"^^xsd:nonNegativeInteger",
        ),
        (
            "wrong axis type",
            "g:hasAxis g:axisScalar",
            "g:hasAxis <urn:finance:alice>",
        ),
        (
            "missing period frame",
            "g:hasReferenceFrame <urn:finance:contract-year>",
            "g:hasReferenceFrame g:referenceFrameUSD",
        ),
        (
            "negative period count",
            "m:quantityValue 2.0",
            "m:quantityValue -2.0",
        ),
        (
            "wrong method recipe",
            "<urn:gmeow:finance:simple:recipe>",
            "<urn:gmeow:finance:return:recipe>",
        ),
        (
            "retired scalar",
            "m:quantityValue 1000.00",
            "g:monetaryValue 1000.00",
        ),
        (
            "zero opening amount",
            "m:quantityValue 1000.00",
            "m:quantityValue 0.00",
        ),
        (
            "unsupported nonterminating return",
            "m:quantityValue 1000.00",
            "m:quantityValue 7.00",
        ),
    ] {
        assert!(
            example().contains(from),
            "control must change input: {name}"
        );
        let input = example().replace(from, to);
        let source = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None).unwrap();
        let subject = if [
            "currency mismatch",
            "zero opening amount",
            "unsupported nonterminating return",
        ]
        .contains(&name)
        {
            "urn:finance:return"
        } else {
            "urn:finance:interest"
        };
        assert!(
            evaluate(
                &source,
                FinanceOwner {
                    dataset: &dataset,
                    shapes: &shapes,
                    identity: &identity
                },
                subject
            )
            .is_err(),
            "{name}"
        );
    }
}

#[test]
fn finance_grounding_actual_holding_count_preserves_asset_and_frame() {
    let (dataset, shapes, identity) = owner();
    let holding = r#"
<urn:finance:alice> a g:Agent .
<urn:finance:asset> a g:Asset . <urn:finance:other-asset> a g:Asset .
<urn:finance:holding> a g:Holding ; g:holdingAgent <urn:finance:alice> ;
    g:holdingAsset <urn:finance:asset> ; g:holdingQuantity <urn:finance:shares> .
<urn:finance:shares> a g:HoldingAssetQuantity ; m:quantityValue 10.0 ;
    m:hasDimension m:dimensionless ; g:countedAsset <urn:finance:asset> ;
    g:hasReferenceFrame <urn:finance:share-count-frame> .
<urn:finance:share-count-frame> a g:ReferenceFrame ;
    g:determinacyModel g:determinacyCrisp ; g:dimensionCount "1"^^xsd:nonNegativeInteger ;
    g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmMathematical ;
    g:hasAxis g:axisScalar ; g:requiresHost false .
"#;
    for (name, body, valid) in [
        ("framed count", holding.to_owned(), true),
        (
            "scalar count",
            holding.replace(
                "g:holdingQuantity <urn:finance:shares>",
                "g:holdingQuantity 10.0",
            ),
            false,
        ),
        (
            "wrong asset",
            holding.replace(
                "g:countedAsset <urn:finance:asset>",
                "g:countedAsset <urn:finance:other-asset>",
            ),
            false,
        ),
        (
            "wrong count frame",
            holding.replace(
                "g:frameRealm g:frameRealmMathematical",
                "g:frameRealm g:frameRealmCurrency",
            ),
            false,
        ),
        (
            "missing count dimension",
            holding.replace(
                "m:hasDimension m:dimensionless ; g:countedAsset",
                "g:countedAsset",
            ),
            false,
        ),
    ] {
        let input = format!("{}\n{body}", example());
        let source = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None).unwrap();
        let result = evaluate(
            &source,
            FinanceOwner {
                dataset: &dataset,
                shapes: &shapes,
                identity: &identity,
            },
            "urn:finance:interest",
        );
        assert_eq!(
            result.is_ok(),
            valid,
            "{name}: {}",
            result
                .err()
                .map_or_else(String::new, |error| error.to_string())
        );
    }
}
