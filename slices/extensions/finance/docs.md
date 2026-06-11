# GMEOW Finance Mapping

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

## Scope

The GMEOW financial slice models personal and SMB financial existence: accounts,
monetary amounts, transactions, ledgers, invoices, orders, asset holdings, and
crypto wallets. It is grounded in the existing GMEOW substrate (`Event`,
`Participation`, `Agreement`, `Rights`, `Trust`) and aligned to heavy standards
(FIBO, ISO 20022, schema.org, FIGI, OFX) **by reference, never imported**
(Principle 5).

This document covers **all phases** (A–D) of issue #64.

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
| `PaymentMethod` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
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
- **Phase B** (this PR): `FinancialTransaction`, `LedgerAccount`, `JournalEntry`,
  `Posting`, double-entry SHACL, transaction/ledger/posting mappings.
- **Phase C** (this PR): `Payment`, `Invoice` (⊑ `Document`), `Order`, `Asset`,
  `Holding` (⊑ `gufo:Relator`), invoice/order/asset/holding mappings.
- **Phase D** (this PR): `CryptoWallet`, schema.org / OFX / ISO 20022 / ledger-CLI
  projections, crypto mappings.

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
