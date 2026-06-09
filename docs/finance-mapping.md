# GMEOW Finance Mapping

<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

## Scope

The GMEOW financial slice models personal and SMB financial existence: accounts,
monetary amounts, transactions, ledgers, invoices, and holdings. It is grounded
in the existing GMEOW substrate (`Event`, `Participation`, `Agreement`, `Rights`,
`Trust`) and aligned to heavy standards (FIBO, ISO 20022) **by reference, never
imported** (Principle 5).

This document covers **Phase A** (foundations). Phases B–D are planned as
follow-up work.

## Design principles

1. **A transaction is a reified `Event`** (REA pattern): money movement with
   `Agent` participants in roles (payer / payee / intermediary). Reuses
   `Event`/`Participation`; never re-minted.
2. **Money is `MonetaryAmount` = amount (`xsd:decimal`) + currency**
   (`ReferenceFrame` individual). ISO 4217 codes are an **open value vocabulary**
   of individuals, never subclasses. DL-clean.
3. **Double-entry as structure** (Phase B): `LedgerAccount`, `JournalEntry ⊑ Event`,
   `Posting ⊑ gufo:Relator`. Balance (Σ debits = Σ credits) enforced by **SHACL,
   not OWL**.
4. **Loans/invoices are `Agreement`s; ownership is `rights`; banks are
   `Organization`s; wallets hold `trust` keys** — reuse, don't duplicate.
5. **Sensitive: P9/P10 + provenance.** Reversed/voided transactions are
   `displayable false`, retained as audit trail, never deleted; disputed amounts
   coexist `accordingTo`; every figure carries `confidence`/`wasAttributedTo`.

## gUFO grounding

| GMEOW term | gUFO category | Parent |
|---|---|---|
| `FinancialAccount` | `gufo:Kind` | `InformationObject` |
| `MonetaryAmount` | `gufo:Kind` | `Entity` |
| `FinancialAccountType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `TransactionType` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |
| `TransactionStatus` | `gufo:AbstractIndividualType` | `gufo:QualityValue` |

## Reuse map

| Finance concept | Reuses GMEOW term | Module |
|---|---|---|
| Transaction | `Event` + `Participation` | events |
| Loan / invoice | `Agreement` + `hasParty` | agreements |
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

### ISO 4217

Each GMEOW **ISO 4217** currency `ReferenceFrame` individual (e.g. `referenceFrameUSD`)
is `skos:exactMatch` to the corresponding FIBO `ISO4217-CurrencyCodes` individual
(e.g. `fibo-iso4217:USD`). Non-ISO assets (e.g. BTC/ETH) are mapped separately.

## Build order (phased)

- **Phase A** (this PR): `FinancialAccount`, `MonetaryAmount`, currency vocabulary,
  basic mappings, SHACL, tests.
- **Phase B**: `FinancialTransaction`, `LedgerAccount`, `JournalEntry`, `Posting`,
  double-entry SHACL (`JournalEntryBalancedShape`).
- **Phase C**: `Payment`, `Invoice` (⊑ `Document`), `Order`, `Asset`, `Holding`
  (⊑ `gufo:Relator`).
- **Phase D**: `CryptoWallet`, schema.org / OFX / ISO 20022 / ledger-CLI projections.

## Constitution principles

Principles 1 (SOTA), 4 (one canonical source), 5 (maximal bridging by reference),
7 (verified by construction), 9 (open value vocabularies), 10 (suppression, never
erasure), 11 (frame-relativity — currency as explicit ReferenceFrame).
