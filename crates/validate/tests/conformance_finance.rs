// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_finance.py (#867)
//!
//! Each test loads a fixture file from `tests/fixtures/coverage/` and validates
//! it against the full ontology + SHACL shapes corpus, mirroring the Python
//! tests that called `g = _graph(); g.parse(...); run_shacl(g)`.
//!
//! Because each fixture is parsed on top of the merged ontology (the Python
//! tests used `_graph()` which is `load_merged_graph(include_imports=False)`),
//! these tests use `validate_with_ontology(&nt)`.
//!
//! Retained in Python (not migrated):
//!   - `test_monetary_amount_is_entity`: cross-slice TBox check (`_graph()` only,
//!     no `run_shacl`); requires SPARQL/graph-pattern query on the merged graph.
//!   - `test_monetary_value_is_functional`: cross-slice TBox check, no `run_shacl`.
//!   - `test_currency_is_functional`: cross-slice TBox check, no `run_shacl`.
//!   - `test_currency_is_subproperty_of_has_reference_frame`: cross-slice TBox check.
//!   - `test_currency_frames_have_realm_currency`: dynamic cross-slice sweep.
//!   - `test_no_transaction_subclass_explosion`: whole-graph absence sweep.
//!   - `test_currency_vocab_is_open_values_not_subclasses` (negative half): sweep.
//!   - `test_transaction_type_vocab_is_open_values` (negative half): sweep.
//!   - `test_asset_type_vocab_is_open_values` (negative half): sweep.
//!   - `test_transaction_uses_participation_not_subproperty` (negative half): sweep.

mod conformance_support;
use conformance_support::*;

/// `test_finance_fixture_conforms` — a well-formed finance data graph passes
/// SHACL validation when merged with the full ontology.
#[test]
fn finance_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-wellformed");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-wellformed.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_double_entry_fixture_conforms` — a balanced journal entry passes
/// SHACL validation when merged with the full ontology.
#[test]
fn double_entry_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-transaction");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-transaction.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_invoice_fixture_conforms` — a well-formed invoice data graph passes
/// SHACL validation when merged with the full ontology.
#[test]
fn invoice_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-invoice");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-invoice.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_order_fixture_conforms` — a well-formed order data graph passes
/// SHACL validation when merged with the full ontology.
#[test]
fn order_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-order");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-order.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_holding_fixture_conforms` — a well-formed holding data graph passes
/// SHACL validation when merged with the full ontology.
#[test]
fn holding_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-holding");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-holding.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_crypto_fixture_conforms` — a well-formed crypto wallet data graph
/// passes SHACL validation when merged with the full ontology.
#[test]
fn crypto_fixture_conforms() {
    let nt = fixture_as_nt("coverage", "finance-crypto");
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "finance-crypto.ttl must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
