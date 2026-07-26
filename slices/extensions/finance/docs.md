# GMEOW Finance Mapping

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

## Scope

The GMEOW financial slice models personal and SMB financial existence: accounts,
monetary amounts, transactions, ledgers, invoices, orders, asset holdings, and
crypto wallets. It is grounded in the existing GMEOW substrate (`Event`,
`Participation`, `Agreement`, `Rights`, `Trust`) and aligned to heavy standards
(FIBO, ISO 20022, schema.org, FIGI, OFX) **by reference, never imported**
(Principle 5).

This document covers **all phases** (A–D) of the design.

## Design principles

1. **A transaction is a reified `Event`** (REA pattern): money movement with
   `Agent` participants in roles (payer / payee / intermediary). Reuses
   `Event`/`Participation`; never re-minted.
2. **Money is `MonetaryAmount` = amount (`xsd:decimal`) + currency**
   (`ReferenceFrame` individual). ISO 4217 codes are an **open value vocabulary**
   of individuals, never subclasses. DL-clean.
3. **Double-entry as structure**: `LedgerAccount`, `JournalEntry ⊑ Event`,
   `Posting ⊑ gufo:Relator` (account × amount × debit/credit). Balance
   (Σ debits = Σ credits) enforced by **SHACL, not OWL**.
4. **Loans are `Agreement`s; invoices are `Document`s; ownership is `rights`;
   banks are `Organization`s; wallets hold `trust` keys** — reuse, don't duplicate.
5. **Sensitive: P9/P10 + provenance.** Reversed/voided transactions are
   `displayable false`, retained as audit trail, never deleted; disputed amounts
   coexist `accordingTo`; every figure carries `confidence`/`wasAttributedTo`.
6. **Flat-first, reify on demand**: a flat `accountBalance` on `FinancialAccount`
   covers the 80% case; promote to a `Holding` relator when cost-basis, period,
   or provenance matters.

## gUFO grounding

| GMEOW term | gUFO category | Parent |
|---|---|---|
| `FinancialAccount` | `gufo:Kind` | `InformationObject` |
| `MonetaryAmount` | `gufo:Kind` | `Entity` |
| `FinancialAccountType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `TransactionType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `TransactionStatus` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `FinancialTransaction` | `gufo:EventType` | `Event` |
| `LedgerAccount` | `gufo:Kind` | `InformationObject` |
| `JournalEntry` | `gufo:EventType` | `Event` |
| `Posting` | `gufo:Kind` | `gufo:Relator` |
| `PostingDirection` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `LedgerAccountType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `Payment` | `gufo:EventType` | `FinancialTransaction` |
| `Invoice` | `gufo:SubKind` | `Document` |
| `InvoiceStatus` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `Order` | `gufo:SubKind` | `Agreement` |
| `OrderStatus` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `Asset` | `gufo:Kind` | `Entity` |
| `AssetType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `Holding` | `gufo:Kind` | `gufo:Relator` |
| `CryptoWallet` | `gufo:SubKind` | `FinancialAccount` |
| `WalletScheme` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |

## Reuse map

| Finance concept | Reuses GMEOW term | Module |
|---|---|---|
| Transaction | `Event` + `Participation` | events |
| Loan | `Agreement` + `hasParty` | agreements |
| Invoice | `Document` | documents |
| Bank / broker | `Organization` | organization |
| Crypto wallet keys | `CryptographicKey` + `holdsKey` | trust |
| Asset ownership | `RightsStatement` / `Copyright` | rights |
| Temporal scope | `TimeInterval` + four clocks | temporal |
| Standpoint dispute | `accordingTo` + `confidence` | standpoint |
| Suppression | `displayable` | provenance |

## External alignment

### FIBO

| GMEOW term | FIBO term | Strength |
|---|---|---|
| `ReferenceFrame` (currency realm) | `fibo-fnd-acc-cur:Currency` | `skos:closeMatch` |
| `hasReferenceFrame` | `fibo-fnd-acc-cur:hasCurrency` | `skos:closeMatch` |
| `MonetaryAmount` | `fibo-fnd-acc-cur:MonetaryAmount` | `skos:closeMatch` |
| `currency` | `fibo-fnd-acc-cur:hasCurrency` | `skos:closeMatch` |
| `FinancialTransaction` | `fibo-fbc-pas-fpas:TransactionEvent` | `skos:closeMatch` |
| `LedgerAccount` | `fibo-fnd-acc-ae:Account` | `skos:closeMatch` |
| `Posting` | `fibo-fnd-acc-ae:BookEntry` | `skos:closeMatch` |
| `Asset` | `fibo-fbc-fi-fi:FinancialInstrument` | `skos:closeMatch` |
| `Holding` | `fibo-fbc-fi-fi:Holding` | `skos:closeMatch` |

### schema.org financial extension

| GMEOW term | schema.org term | Strength |
|---|---|---|
| `FinancialAccount` | `schema:BankAccount` | `skos:closeMatch` |
| `FinancialAccount` | `schema:DepositAccount` | `skos:relatedMatch` |
| `MonetaryAmount` | `schema:MonetaryAmount` | `skos:closeMatch` |
| `monetaryValue` | `schema:value` | `skos:closeMatch` |
| `currency` | `schema:currency` | `skos:closeMatch` |
| `iban` | `schema:iban` | `skos:exactMatch` |
| `accountBalance` | `schema:accountBalance` | `skos:closeMatch` |
| `FinancialTransaction` | `schema:MoneyTransfer` | `skos:closeMatch` |
| `Payment` | `schema:Payment` | `skos:closeMatch` |
| `Invoice` | `schema:Invoice` | `skos:closeMatch` |
| `Order` | `schema:Order` | `skos:closeMatch` |
| `CryptoWallet` | `schema:BankAccount` | `skos:closeMatch` |

### ISO 4217

Each GMEOW **ISO 4217** currency `ReferenceFrame` individual (e.g. `referenceFrameUSD`)
is `skos:exactMatch` to the corresponding FIBO `ISO4217-CurrencyCodes` individual
(e.g. `fibo-iso4217:USD`). Non-ISO assets (e.g. BTC/ETH) are mapped separately.

## Projection lossiness

| Target | What drops | What survives |
|---|---|---|
| schema.org | Currency as ReferenceFrame → string code; participant roles flattened; ledger detail lost | Amount, account, invoice, order, payment |
| OFX / FDX | Standpoint, confidence, provenance; multi-currency detail | Account balances, transactions, holdings |
| ISO 20022 camt.053 | Standpoint metadata; GMEOW-specific relator structure | Bank statement accounts, entries, balances |
| Ledger-CLI / GnuCash | Currency frames, provenance, standpoint | Double-entry postings, accounts, amounts |

## SHACL validation

| Shape | What it enforces |
|---|---|
| `MonetaryAmountHasCurrencyShape` | Every `MonetaryAmount` has exactly one `currency` |
| `MonetaryAmountHasValueShape` | Every `MonetaryAmount` has exactly one `monetaryValue` |
| `FinancialAccountShape` | Every `FinancialAccount` has ≥1 holder, ≤1 type, currencies are ReferenceFrames |
| `TransactionHasPartiesShape` | Every `FinancialTransaction` has ≥1 participant |
| `LedgerAccountShape` | Every `LedgerAccount` has ≥1 holder, exactly 1 type |
| `JournalEntryHasPostingsShape` | Every `JournalEntry` has ≥2 postings |
| `PostingShape` | Every `Posting` has exactly 1 journal entry, account, amount, direction |
| `InvoiceShape` | Every `Invoice` has exactly 1 amount, ≥1 issuer, ≥1 recipient |
| `OrderShape` | Every `Order` has exactly 1 amount, ≥1 buyer, ≥1 seller |
| `HoldingShape` | Every `Holding` has exactly 1 agent, 1 asset, 1 quantity |
| `CryptoWalletShape` | Every `CryptoWallet` has exactly 1 scheme |

## Build order (phased)

- **Phase A** (merged): `FinancialAccount`, `MonetaryAmount`, currency vocabulary,
  basic mappings, SHACL, tests.
- **Phase B**: `FinancialTransaction`, `LedgerAccount`, `JournalEntry`,
  `Posting`, double-entry SHACL, transaction/ledger/posting mappings.
- **Phase C**: `Payment`, `Invoice` (⊑ `Document`), `Order`, `Asset`,
  `Holding` (⊑ `gufo:Relator`), invoice/order/asset/holding mappings.
- **Phase D**: `CryptoWallet`, schema.org / OFX / ISO 20022 / ledger-CLI
  projections, crypto mappings.

## Terms

### gmeow:FinancialAccount · gmeow:accountHolder · gmeow:accountBalance · gmeow:accountCurrency · gmeow:accountType

A financial account held by an agent with a financial institution — a bank,
credit, investment, or wallet account — modelled as an `InformationObject`,
distinct from `OnlineAccount`. `accountHolder` is non-functional (joint accounts
have co-equal holders); `accountBalance` is a `MonetaryAmount`; `accountCurrency`
is non-functional (multi-currency accounts); `accountType` is one functional
`FinancialAccountType` value.

### gmeow:accountNumber · gmeow:iban · gmeow:bic

The account's institution-level identifiers: a free-form `accountNumber`, the
ISO 13616 `iban`, and the ISO 9362 SWIFT `bic` of the holding institution.

### gmeow:FinancialAccountType

The kind of a financial account as an open value vocabulary (bank, credit,
investment, wallet) — a value pointed at by `accountType`, never a subclass.

### gmeow:FinancialTransaction · gmeow:transactionAmount · gmeow:transactionType · gmeow:transactionStatus

A money-movement event (REA pattern) reusing the `Event`/`Participation`
substrate: payer, payee, and intermediary are `ParticipantRole` values, never
subproperties. `transactionAmount` is one functional `MonetaryAmount`; type and
status are non-functional open value vocabularies.

### gmeow:TransactionType · gmeow:TransactionStatus

The open value vocabularies for transaction kind (payment, transfer, deposit,
withdrawal, fee, interest, refund) and status (pending, completed, reversed,
failed) — reversed and voided records are retained `displayable` false.

### gmeow:LedgerAccount · gmeow:ledgerAccountType · gmeow:ledgerAccountHolder · gmeow:ledgerAccountCurrency

A double-entry book-keeping account (asset, liability, equity, revenue, expense)
— an `InformationObject` distinct from the bank-level `FinancialAccount`.
`ledgerAccountType` is functional; holder and currency are non-functional.

### gmeow:LedgerAccountType · gmeow:PostingDirection

Open value vocabularies for the kind of ledger account and the direction of a
posting (debit, credit) — values, never subclasses.

### gmeow:JournalEntry · gmeow:journalEntryPostings · gmeow:Posting · gmeow:postingJournalEntry · gmeow:postingAccount · gmeow:postingAmount · gmeow:postingDirection

A balanced double-entry event (`JournalEntry ⊑ Event`) composed of two or more
`Posting` relators. Each `Posting` carries exactly one journal entry, ledger
account, `MonetaryAmount`, and direction; balance (Σ debits = Σ credits) is
SHACL-enforced, never OWL.

### gmeow:Payment · gmeow:paymentMethod

A payment event — a thin subclass of `FinancialTransaction` distinguished by the
`paymentMethod` facet, drawn from the open `PaymentMethod` value vocabulary
(cash, cheque, credit card, bank transfer, cryptocurrency) owned by the core
`accounts` slice. Non-functional: split payments carry several methods.

### gmeow:Invoice · gmeow:invoiceAmount · gmeow:invoiceIssuer · gmeow:invoiceRecipient · gmeow:invoiceDueDate · gmeow:invoiceStatus · gmeow:InvoiceStatus

A billing document (`Invoice ⊑ Document`) with a functional total `invoiceAmount`,
non-functional issuer and recipient, a DL-clean `xsd:dateTime` due date, and an
open `InvoiceStatus` value (draft, sent, paid, overdue, cancelled).

### gmeow:Order · gmeow:orderAmount · gmeow:orderBuyer · gmeow:orderSeller · gmeow:orderStatus · gmeow:OrderStatus

A purchase or sales order (`Order ⊑ Agreement`) between buyer and seller with a
functional total `orderAmount` and an open `OrderStatus` value (pending,
confirmed, shipped, delivered, cancelled).

### gmeow:Asset · gmeow:assetType · gmeow:assetIdentifier · gmeow:AssetType

A financial asset — stock, bond, cryptocurrency, real estate, commodity — the
thing that is held, distinct from the `Holding` relator. `assetType` is a
functional `AssetType` value; `assetIdentifier` carries FIGI/ISIN/ticker.

### gmeow:Holding · gmeow:holdingAgent · gmeow:holdingAsset · gmeow:holdingQuantity · gmeow:holdingCostBasis · gmeow:holdingPeriod

A reify-on-demand relator connecting agent × asset × quantity × cost-basis ×
optional period — promoted from a flat `accountBalance` when cost-basis, period,
or provenance must be recorded.

### gmeow:CryptoWallet · gmeow:walletAddress · gmeow:walletScheme · gmeow:walletKey · gmeow:WalletScheme

A digital wallet holding cryptocurrency (`CryptoWallet ⊑ FinancialAccount`) with
one or more public `walletAddress`es, a functional `walletScheme` (Bitcoin,
Ethereum, Solana, Monero — the open `WalletScheme` vocabulary), and `walletKey`
linkage to the controlling `CryptographicKey`(s).

## Constitution principles

Principles 1 (SOTA), 4 (one canonical source), 5 (maximal bridging by reference),
7 (verified by construction), 9 (open value vocabularies), 10 (suppression, never
erasure), 11 (frame-relativity — currency as explicit ReferenceFrame).

## References (SOTA)

- **FIBO** (Financial Industry Business Ontology, EDM Council / OMG) —
  Accounting/CurrencyAmount, ISO4217-CurrencyCodes, LoanOrCredit, FinancialInstruments.
  <https://spec.edmcouncil.org/fibo/>
- **schema.org financial extension** (FIBO-derived). <https://schema.org/docs/financial.html>
- **ISO 4217** (currency), **ISO 20022** (camt.053 / pain / pacs), **FIGI**
  (instrument id), **OFX / FDX** (personal-finance interchange), **REA**
  ontology (Resources-Events-Agents accounting).
