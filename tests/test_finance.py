"""Retained dynamic guards for the finance module (#64).

Asserted-TBox invariants whose subjects are defined in
slices/extensions/finance/module.ttl have been migrated to
slices/extensions/finance/tests/structural.ttl as declarative
gmeow:StructuralAssertion cells (DSL twin, #867).

RETAINED here (not migratable to scopeModule cells):
  - test_monetary_amount_is_entity: gmeow:MonetaryAmount is cross-slice.
  - test_monetary_value_is_functional: gmeow:monetaryValue is cross-slice.
  - test_currency_is_functional: gmeow:currency is cross-slice.
  - test_currency_is_subproperty_of_has_reference_frame: cross-slice.
  - test_currency_frames_have_realm_currency: reference-frame individuals
    are defined outside the finance module; dynamic cross-slice check.
  - test_no_transaction_subclass_explosion: whole-graph sweep asserting
    the absence of three named classes; dynamic, not module-scoped.
  - test_currency_vocab_is_open_values_not_subclasses (negative half):
    whole-graph sweep for absent BankAccount/CreditAccount/etc. classes.
  - test_transaction_type_vocab_is_open_values (negative half): sweep
    for absent PaymentTransaction/DebitPosting/etc. classes.
  - test_asset_type_vocab_is_open_values (negative half): sweep for
    absent StockAsset/BondAsset/CryptoAsset classes.
  - test_transaction_uses_participation_not_subproperty (negative half):
    sweep for absent hasPayer/hasPayee/hasIntermediary properties.
  - All run_shacl calls: ExampleConformance, not TBox invariants.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
LOGIC = "https://blackcatinformatics.ca/logic/"

COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_monetary_amount_is_entity() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "MonetaryAmount"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Entity"),
    ) in graph
    assert (
        URIRef(GMEOW + "MonetaryAmount"),
        RDF.type,
        URIRef(LOGIC + "Kind"),
    ) in graph


def test_currency_vocab_is_open_values_not_subclasses() -> None:
    # Negative half: no per-type subclasses of FinancialAccount may exist.
    # (Positive half migrated to structural.ttl: ex:saFinancialAccountTypeSeeds)
    graph = _graph()
    for rejected in (
        "BankAccount",
        "CreditAccount",
        "InvestmentAccount",
        "WalletAccount",
    ):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.Class,
        ) not in graph, f"{rejected} must not exist as a class"


def test_monetary_value_is_functional() -> None:
    graph = _graph()
    monetary_value = URIRef(GMEOW + "monetaryValue")
    assert (monetary_value, RDF.type, OWL.DatatypeProperty) in graph
    assert (
        monetary_value,
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph, "monetaryValue must be functional"


def test_currency_is_functional() -> None:
    graph = _graph()
    currency = URIRef(GMEOW + "currency")
    assert (currency, RDF.type, OWL.ObjectProperty) in graph
    assert (
        currency,
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph, "currency must be functional"


def test_currency_is_subproperty_of_has_reference_frame() -> None:
    graph = _graph()
    currency = URIRef(GMEOW + "currency")
    has_ref_frame = URIRef(GMEOW + "hasReferenceFrame")
    assert (currency, RDFS.subPropertyOf, has_ref_frame) in graph


def test_currency_frames_have_realm_currency() -> None:
    graph = _graph()
    frame_realm_currency = URIRef(GMEOW + "frameRealmCurrency")
    for currency in (
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
    ):
        assert (
            URIRef(GMEOW + currency),
            URIRef(GMEOW + "frameRealm"),
            frame_realm_currency,
        ) in graph, f"{currency} must have frameRealmCurrency"


def test_no_transaction_subclass_explosion() -> None:
    graph = _graph()
    for rejected in (
        "PaymentTransaction",
        "InvoiceTransaction",
        "TransferTransaction",
    ):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.Class,
        ) not in graph, (
            f"{rejected} must not exist as a class (Phase B will use value vocab)"
        )


def test_finance_fixture_conforms() -> None:
    """A well-formed finance data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-wellformed.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# --------------------------------------------------------------------------- #
# Phase B -- Transactions, Ledger, Posting
# --------------------------------------------------------------------------- #


def test_transaction_type_vocab_is_open_values() -> None:
    # Negative half: no per-type subclasses may exist (whole-graph sweep).
    # (Positive halves migrated to structural.ttl: ex:saTransactionTypeSeeds,
    # ex:saTransactionStatusSeeds, ex:saLedgerAccountTypeSeeds,
    # ex:saPostingDirectionSeeds)
    graph = _graph()
    for rejected in (
        "PaymentTransaction",
        "InvoiceTransaction",
        "TransferTransaction",
        "DebitPosting",
        "CreditPosting",
    ):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.Class,
        ) not in graph, f"{rejected} must not exist as a class"


def test_transaction_uses_participation_not_subproperty() -> None:
    # Negative half: no shortcut subproperties may exist (whole-graph sweep).
    # (Positive half migrated to structural.ttl: ex:saTransactionRoleSeeds)
    graph = _graph()
    for rejected in ("hasPayer", "hasPayee", "hasIntermediary"):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.ObjectProperty,
        ) not in graph, f"{rejected} must not exist as a property"


def test_double_entry_fixture_conforms() -> None:
    """A balanced journal entry passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-transaction.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# --------------------------------------------------------------------------- #
# Phase C -- Payment, Invoice, Order, Asset, Holding
# --------------------------------------------------------------------------- #


def test_asset_type_vocab_is_open_values() -> None:
    # Negative half: no per-type Asset subclasses may exist (whole-graph
    # sweep). (Positive half migrated to structural.ttl: ex:saAssetTypeSeeds)
    graph = _graph()
    for rejected in ("StockAsset", "BondAsset", "CryptoAsset"):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.Class,
        ) not in graph, f"{rejected} must not exist as a class"


def test_invoice_fixture_conforms() -> None:
    """A well-formed invoice data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-invoice.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_order_fixture_conforms() -> None:
    """A well-formed order data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-order.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


def test_holding_fixture_conforms() -> None:
    """A well-formed holding data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-holding.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# --------------------------------------------------------------------------- #
# Phase D -- CryptoWallet
# --------------------------------------------------------------------------- #


def test_crypto_fixture_conforms() -> None:
    """A well-formed crypto wallet data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-crypto.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
