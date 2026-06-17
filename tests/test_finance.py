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
from tests._graph_nt import run_shacl

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


# --------------------------------------------------------------------------- #
# Phase B — Transactions, Ledger, Posting
# --------------------------------------------------------------------------- #


def test_financial_transaction_is_event() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "FinancialTransaction"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Event"),
    ) in graph
    assert (
        URIRef(GMEOW + "FinancialTransaction"),
        RDF.type,
        URIRef(GUFO + "EventType"),
    ) in graph


def test_journal_entry_is_event() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "JournalEntry"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Event"),
    ) in graph
    assert (
        URIRef(GMEOW + "JournalEntry"),
        RDF.type,
        URIRef(GUFO + "EventType"),
    ) in graph


def test_posting_is_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Posting"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
    assert (
        URIRef(GMEOW + "Posting"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph


def test_ledger_account_is_information_object() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "LedgerAccount"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    assert (
        URIRef(GMEOW + "LedgerAccount"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph


def test_transaction_type_vocab_is_open_values() -> None:
    graph = _graph()
    for value in (
        "transactionTypePayment",
        "transactionTypeTransfer",
        "transactionTypeDeposit",
        "transactionTypeWithdrawal",
        "transactionTypeFee",
        "transactionTypeInterest",
        "transactionTypeRefund",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "TransactionType"),
        ) in graph
    for value in (
        "transactionStatusPending",
        "transactionStatusCompleted",
        "transactionStatusReversed",
        "transactionStatusFailed",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "TransactionStatus"),
        ) in graph
    for value in (
        "ledgerAccountTypeAsset",
        "ledgerAccountTypeLiability",
        "ledgerAccountTypeEquity",
        "ledgerAccountTypeRevenue",
        "ledgerAccountTypeExpense",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "LedgerAccountType"),
        ) in graph
    for value in (
        "postingDirectionDebit",
        "postingDirectionCredit",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "PostingDirection"),
        ) in graph
    # There must be no per-type subclasses.
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
    graph = _graph()
    for role in ("rolePayer", "rolePayee", "roleIntermediary"):
        assert (
            URIRef(GMEOW + role),
            RDF.type,
            URIRef(GMEOW + "ParticipantRole"),
        ) in graph
    # There must be no hasPayer / hasPayee subproperties.
    for rejected in ("hasPayer", "hasPayee", "hasIntermediary"):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.ObjectProperty,
        ) not in graph, f"{rejected} must not exist as a property"


def test_transaction_amount_is_functional() -> None:
    graph = _graph()
    txn_amount = URIRef(GMEOW + "transactionAmount")
    assert (txn_amount, RDF.type, OWL.ObjectProperty) in graph
    assert (
        txn_amount,
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph, "transactionAmount must be functional"


def test_posting_properties_are_functional() -> None:
    graph = _graph()
    for prop in (
        "postingJournalEntry",
        "postingAccount",
        "postingAmount",
        "postingDirection",
    ):
        p = URIRef(GMEOW + prop)
        assert (p, RDF.type, OWL.ObjectProperty) in graph
        assert (
            p,
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} must be functional"


def test_double_entry_fixture_conforms() -> None:
    """A balanced journal entry passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-transaction.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)


# --------------------------------------------------------------------------- #
# Phase C — Payment, Invoice, Order, Asset, Holding
# --------------------------------------------------------------------------- #


def test_payment_is_financial_transaction() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Payment"),
        RDFS.subClassOf,
        URIRef(GMEOW + "FinancialTransaction"),
    ) in graph


def test_invoice_is_document() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Invoice"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Document"),
    ) in graph


def test_order_is_agreement() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Order"),
        RDFS.subClassOf,
        URIRef(GMEOW + "Agreement"),
    ) in graph


def test_asset_type_vocab_is_open_values() -> None:
    graph = _graph()
    for value in (
        "assetTypeStock",
        "assetTypeBond",
        "assetTypeCryptocurrency",
        "assetTypeRealEstate",
        "assetTypeCommodity",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "AssetType"),
        ) in graph
    for rejected in ("StockAsset", "BondAsset", "CryptoAsset"):
        assert (
            URIRef(GMEOW + rejected),
            RDF.type,
            OWL.Class,
        ) not in graph, f"{rejected} must not exist as a class"


def test_payment_invoice_order_vocab_is_open_values() -> None:
    graph = _graph()
    for value in (
        "paymentMethodCash",
        "paymentMethodCheque",
        "paymentMethodCreditCard",
        "paymentMethodBankTransfer",
        "paymentMethodCrypto",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "PaymentMethod"),
        ) in graph
    for value in (
        "invoiceStatusSent",
        "invoiceStatusPaid",
        "invoiceStatusOverdue",
        "invoiceStatusCancelled",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "InvoiceStatus"),
        ) in graph
    for value in (
        "orderStatusConfirmed",
        "orderStatusShipped",
        "orderStatusDelivered",
        "orderStatusCancelled",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "OrderStatus"),
        ) in graph


def test_wallet_scheme_vocab_is_open_values() -> None:
    graph = _graph()
    for value in (
        "walletSchemeBTC",
        "walletSchemeETH",
        "walletSchemeSOL",
        "walletSchemeXMR",
    ):
        assert (
            URIRef(GMEOW + value),
            RDF.type,
            URIRef(GMEOW + "WalletScheme"),
        ) in graph


def test_holding_is_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "Holding"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph
    assert (
        URIRef(GMEOW + "Holding"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph


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
# Phase D — CryptoWallet
# --------------------------------------------------------------------------- #


def test_crypto_wallet_is_financial_account() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CryptoWallet"),
        RDFS.subClassOf,
        URIRef(GMEOW + "FinancialAccount"),
    ) in graph
    assert (
        URIRef(GMEOW + "CryptoWallet"),
        RDF.type,
        URIRef(GUFO + "SubKind"),
    ) in graph


def test_wallet_address_non_functional() -> None:
    graph = _graph()
    wallet_address = URIRef(GMEOW + "walletAddress")
    assert (wallet_address, RDF.type, OWL.DatatypeProperty) in graph
    assert (
        wallet_address,
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph, "walletAddress must stay non-functional (multi-address wallets)"


def test_wallet_scheme_is_functional() -> None:
    graph = _graph()
    wallet_scheme = URIRef(GMEOW + "walletScheme")
    assert (wallet_scheme, RDF.type, OWL.ObjectProperty) in graph
    assert (
        wallet_scheme,
        RDF.type,
        OWL.FunctionalProperty,
    ) in graph, "walletScheme must be functional"


def test_crypto_fixture_conforms() -> None:
    """A well-formed crypto wallet data graph passes SHACL validation."""
    g = _graph()
    g.parse(COVERAGE_FIXTURES / "finance-crypto.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
