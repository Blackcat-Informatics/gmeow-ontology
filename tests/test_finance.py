"""Structural + DL-safety guards for the finance module (#64, Phase A).

These tests pin the decisions that keep the financial slice grounded in gUFO,
frame-relative (Principle 11), open-vocabulary (Principle 9), and free of
subclass explosion.
"""

from __future__ import annotations

from pathlib import Path

from rdflib import Graph, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"

EX_FIN = Namespace("https://blackcatinformatics.ca/gmeow/examples/finance/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_financial_account_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "FinancialAccount"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "FinancialAccount"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph


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
        URIRef(GUFO + "Kind"),
    ) in graph


def test_currency_vocab_is_open_values_not_subclasses() -> None:
    graph = _graph()
    # FinancialAccountType values are individuals, not classes.
    for value in (
        "accountTypeBank",
        "accountTypeCredit",
        "accountTypeInvestment",
        "accountTypeWallet",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "FinancialAccountType"),
        ) in graph
    # There must be no per-type subclasses of FinancialAccount.
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


def test_account_currency_is_non_functional() -> None:
    graph = _graph()
    account_currency = URIRef(GMEOW + "accountCurrency")
    assert (account_currency, RDF.type, OWL.ObjectProperty) in graph
    assert (
        account_currency,
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph, "accountCurrency must stay non-functional (multi-currency accounts)"


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
