// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_finance.py
//!
//! Each fixture-based test loads a fixture file from `tests/fixtures/coverage/` and
//! validates it against the full ontology + SHACL shapes corpus, mirroring the
//! Python tests that called `g = _graph(); g.parse(...); run_shacl(g)`.
//!
//! Cross-slice TBox and whole-graph sweep assertions are also migrated here using
//! the native graph-query helper.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

// ── Fixture-only SHACL conformance cases ──────────────────────────────────────

#[rstest]
#[case::finance_fixture_conforms(Case::file("coverage", "finance-wellformed").with_ontology())]
#[case::double_entry_fixture_conforms(Case::file("coverage", "finance-transaction").with_ontology())]
#[case::invoice_fixture_conforms(Case::file("coverage", "finance-invoice").with_ontology())]
#[case::order_fixture_conforms(Case::file("coverage", "finance-order").with_ontology())]
#[case::holding_fixture_conforms(Case::file("coverage", "finance-holding").with_ontology())]
#[case::crypto_fixture_conforms(Case::file("coverage", "finance-crypto").with_ontology())]
fn finance(#[case] case: Case) {
    case.run();
}

// ── Cross-slice TBox assertions ───────────────────────────────────────────────

#[test]
fn monetary_amount_is_entity() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}MonetaryAmount")),
        Some(RDFS_SUBCLASS_OF),
        Some(&format!("{GMEOW}Entity")),
    ));
    assert!(g.has(
        Some(&format!("{GMEOW}MonetaryAmount")),
        Some(RDF_TYPE),
        Some(&format!("{LOGIC}Kind")),
    ));
}

#[test]
fn monetary_value_is_functional() {
    let g = GraphStore::ontology();
    let iri = format!("{GMEOW}monetaryValue");
    assert!(g.has(Some(&iri), Some(RDF_TYPE), Some(OWL_DATATYPE_PROPERTY)));
    assert!(
        g.is_functional_carrier(&iri),
        "gmeow:monetaryValue must carry a logic: functionalProperty characteristic"
    );
}

#[test]
fn currency_is_functional() {
    let g = GraphStore::ontology();
    let iri = format!("{GMEOW}currency");
    assert!(g.has(Some(&iri), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)));
    assert!(
        g.is_functional_carrier(&iri),
        "gmeow:currency must carry a logic: functionalProperty characteristic"
    );
}

#[test]
fn currency_is_subproperty_of_has_reference_frame() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&format!("{GMEOW}currency")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&format!("{GMEOW}hasReferenceFrame")),
    ));
}

#[test]
fn currency_frames_have_realm_currency() {
    let g = GraphStore::ontology();
    let frame_realm_currency = format!("{GMEOW}frameRealmCurrency");
    for currency in [
        "referenceFrameUSD",
        "referenceFrameEUR",
        "referenceFrameGBP",
        "referenceFrameJPY",
        "referenceFrameCAD",
        "referenceFrameCHF",
        "referenceFrameAUD",
        "referenceFrameCNY",
        "referenceFrameBTC",
        "referenceFrameETH",
    ] {
        assert!(
            g.has(
                Some(&format!("{GMEOW}{currency}")),
                Some(&format!("{GMEOW}frameRealm")),
                Some(&frame_realm_currency),
            ),
            "{currency} must have frameRealmCurrency"
        );
    }
}

// ── Whole-graph negative sweeps (open vocabularies, not subclass explosion) ───

#[test]
fn currency_vocab_is_open_values_not_subclasses() {
    let g = GraphStore::ontology();
    for rejected in [
        "BankAccount",
        "CreditAccount",
        "InvestmentAccount",
        "WalletAccount",
    ] {
        assert!(
            !g.has(
                Some(&format!("{GMEOW}{rejected}")),
                Some(RDF_TYPE),
                Some(OWL_CLASS),
            ),
            "{rejected} must not exist as a class"
        );
    }
}

#[test]
fn no_transaction_subclass_explosion() {
    let g = GraphStore::ontology();
    for rejected in [
        "PaymentTransaction",
        "InvoiceTransaction",
        "TransferTransaction",
    ] {
        assert!(
            !g.has(
                Some(&format!("{GMEOW}{rejected}")),
                Some(RDF_TYPE),
                Some(OWL_CLASS),
            ),
            "{rejected} must not exist as a class"
        );
    }
}

#[test]
fn transaction_type_vocab_is_open_values() {
    let g = GraphStore::ontology();
    for rejected in [
        "PaymentTransaction",
        "InvoiceTransaction",
        "TransferTransaction",
        "DebitPosting",
        "CreditPosting",
    ] {
        assert!(
            !g.has(
                Some(&format!("{GMEOW}{rejected}")),
                Some(RDF_TYPE),
                Some(OWL_CLASS),
            ),
            "{rejected} must not exist as a class"
        );
    }
}

#[test]
fn transaction_uses_participation_not_subproperty() {
    let g = GraphStore::ontology();
    for rejected in ["hasPayer", "hasPayee", "hasIntermediary"] {
        assert!(
            !g.has(
                Some(&format!("{GMEOW}{rejected}")),
                Some(RDF_TYPE),
                Some(OWL_OBJECT_PROPERTY),
            ),
            "{rejected} must not exist as a property"
        );
    }
}

#[test]
fn asset_type_vocab_is_open_values() {
    let g = GraphStore::ontology();
    for rejected in ["StockAsset", "BondAsset", "CryptoAsset"] {
        assert!(
            !g.has(
                Some(&format!("{GMEOW}{rejected}")),
                Some(RDF_TYPE),
                Some(OWL_CLASS),
            ),
            "{rejected} must not exist as a class"
        );
    }
}
