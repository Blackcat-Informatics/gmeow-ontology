<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rights, IP, trademark & licensing

GMEOW records the rights of the things it describes as **reified, attributed,
machine-readable, time-scoped claims** — who may do what with a work, under what
licence, with what copyright, attributed to whom, over what term. This is the
doctrine document for the rights facility (`ontology/modules/rights.ttl`); its
companion is the [alignment & projection reference](../slices/core/rights/docs.md).

## Why a cross-cutting facility

Two different things both get called "licensing" in an ontology toolchain, and
GMEOW keeps them apart:

- **The ontology-alignment link policy** (`src/gmeow_tools/config.py` —
  `LinkPolicy` / `policy_for_license`) governs whether an external vocabulary's
  *axioms* may be copied **into** GMEOW. That is a build-time concern about GMEOW
  itself.
- **This facility** governs the rights of the **instances GMEOW describes** — a
  photograph's copyright, a brand's trademark, a dataset's licence. That is a
  data-modelling concern about everything else.

Today GMEOW states rights about *itself* (`dcterms:license` / `dc:rights` on the
root ontology). The rights facility **generalises that to arbitrary instances**.
It is **foundational**: the planned Images super-ontology (image rights) and the
Employment / publication block (work & publication licensing) both build on it, so
it ships first.

## The model

Rights are modelled with the GMEOW relator idiom — the same reify-a-relationship
pattern as `gmeow:Agreement`, `gmeow:Certification` and `gmeow:NameUsage` — so the
facility composes with every other slice instead of re-inventing agents, works, or
time.

| Concept | GMEOW term | gUFO grounding | Reuses |
|---|---|---|---|
| Machine-readable rights statement | `gmeow:RightsStatement` | `gufo:Kind` ⊑ `gufo:Relator` | the ODRL Policy idea |
| Copyright | `gmeow:Copyright` | `gufo:Kind` ⊑ `gufo:Relator` | `gmeow:wasAttributedTo` |
| Licence | `gmeow:License` | `gufo:SubKind` ⊑ `gmeow:Agreement` | `gmeow:hasParty` |
| Trademark | `gmeow:Trademark` | `gufo:Kind` ⊑ `gufo:Relator` | `gmeow:wasAttributedTo` |
| Mark / brand sign | `gmeow:Mark` | `gufo:Kind` ⊑ `gmeow:InformationObject` | — |
| Deontic rule | `gmeow:Rule` → `gmeow:Permission` / `gmeow:Prohibition` / `gmeow:Duty` | `gufo:Category` / `gufo:Kind` ⊑ `gufo:Relator` | the ODRL rule trio |

Open **value vocabularies** (individuals, never per-value subclasses —
Principle 9): `gmeow:RightsAction` (the full ODRL action vocabulary — ≈47 actions),
`gmeow:LicenseFamily`, `gmeow:TrademarkStatus`, `gmeow:CopyrightStatus` (all twelve
RightsStatements.org statements), `gmeow:RightsType` (the kinds of IP right —
copyright, trademark, patent, industrial design, trade secret, related/moral rights,
database right, plant breeders' rights), and the ODRL constraint-algebra value sets
`gmeow:LeftOperand` (≈34), `gmeow:ConstraintOperator` (12), `gmeow:ConstraintLogic`,
`gmeow:ConflictStrategy`.

### Deontic logic, not just structure (Principle 1)

A real machine-readable policy is *conditional*. GMEOW models the ODRL **constraint
algebra** — `gmeow:AtomicConstraint` (a `leftOperand` / `operator` / `rightOperand`
comparison, e.g. *dateTime ≤ 2036*, *spatial = EU*, *count ≤ 5*) and
`gmeow:LogicalConstraint` (boolean `and` / `or` / `xone` / `andSequence` over
constraints) — plus a policy **conflict-resolution strategy**
(`gmeow:conflictStrategy`: permission-wins / prohibition-wins / void) and duty
**consequence / remedy** chaining (`gmeow:ruleConsequence`). The logic of the standard
is modelled, then projected back to ODRL losslessly for the atomic case.

### Temporally bound, and provenanced

Every rights claim is **temporally bound** and **carries provenance**. Validity rides
the temporal module's `gmeow:validFrom` / `gmeow:validUntil` (a licence valid 2026–2036,
a lapsed trademark with an end date — suppressed, never deleted); a deontic temporal
bound is also expressible as an `odrl:dateTime` constraint. Who asserted a rights claim,
when, from which source, and with what confidence — and contested rival claims indexed
by standpoint — ride GMEOW's RDF-1.2 statement layer (`dsl/statements/rights.ttl`,
`gmeow:wasAttributedTo` / `mappedFrom` / `assertedAt` / `confidence` / `accordingTo`).

### Reuse, never duplicate

- **A licence *is* an agreement.** `gmeow:License ⊑ gmeow:Agreement` inherits
  `gmeow:hasParty`, specialised by `gmeow:licensor` / `gmeow:licensee`.
- **A rights holder *is* an agent.** Holder attribution reuses
  `gmeow:wasAttributedTo`, specialised by `gmeow:copyrightHolder` /
  `gmeow:trademarkHolder` — there is no parallel `hasHolder`.
- **Terms expire in time.** Validity windows ride the temporal module's
  `gmeow:validFrom` / `gmeow:validUntil`; uncertainty rides `gmeow:confidence`.
- **`gmeow:hasLicense`, not `gmeow:license`** — `gmeow:license` is already the
  mapping-DSL set-license datatype property, so the object property is
  `gmeow:hasLicense`.

### Flat-first, reify on demand (Principle 4)

The common case is a flat `gmeow:hasLicense` / `gmeow:hasCopyright` /
`gmeow:hasTrademark` edge. Promote to a `gmeow:RightsStatement` with
`gmeow:Permission` / `gmeow:Prohibition` / `gmeow:Duty` rules only when the
permitted / prohibited / required **actions** must be expressed — at which point
the facility is a true superset of an ODRL policy.

### Machine-readable identifiers (SPDX)

`gmeow:spdxLicenseId` carries the SPDX License List short identifier (`MIT`,
`Apache-2.0`, `CC-BY-4.0`, `GPL-3.0-only`, …) — the canonical machine-readable
licence id and the bridge to the SBOM / package-manager world. The SPDX List also
assigns each id a stable IRI under `http://spdx.org/licenses/`. ODRL/CC REL say
*what you may do*; dcterms/schema give a licence *URL*; SPDX gives the *identifier*.

## Worked example

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

# A photograph, CC BY 4.0, © 2026 Jane Doe, with an explicit machine-readable policy.
ex:photo a gmeow:MediaObject ;
    gmeow:hasCopyright ex:photo-copyright ;
    gmeow:hasLicense   ex:cc-by-4 ;
    gmeow:hasRightsStatement ex:photo-rights ;
    gmeow:attributionText "Photo by Jane Doe / CC BY 4.0" .

ex:photo-copyright a gmeow:Copyright ;
    gmeow:copyrightWork ex:photo ;
    gmeow:copyrightHolder ex:jane ;          # ⊑ gmeow:wasAttributedTo
    gmeow:copyrightYear "2026" ;
    gmeow:copyrightStatus gmeow:copyrightStatusInCopyright .

ex:cc-by-4 a gmeow:License ;                 # ⊑ gmeow:Agreement
    gmeow:licensor ex:jane ;                 # ⊑ gmeow:hasParty
    gmeow:licensedWork ex:photo ;
    gmeow:licenseFamily gmeow:licenseFamilyCC ;
    gmeow:spdxLicenseId "CC-BY-4.0" .

ex:photo-rights a gmeow:RightsStatement ;    # the ODRL-superset policy
    gmeow:statementAbout ex:photo ;
    gmeow:hasPermission ex:perm-reuse ;
    gmeow:hasDuty       ex:duty-attribute .

ex:perm-reuse  a gmeow:Permission ; gmeow:ruleAction gmeow:actionReproduce .
ex:duty-attribute a gmeow:Duty ;    gmeow:ruleAction gmeow:actionAttribute .
```

`gmeow project --profile odrl` turns `ex:photo-rights` into a pure `odrl:Set` with
an `odrl:permission` (action `gmeow:actionReproduce`, aligned `exactMatch` to
`odrl:reproduce`) and an `odrl:obligation`; `--profile schema-org` emits
`schema:copyrightHolder` / `schema:copyrightYear` / `schema:license` /
`schema:creditText`; `--profile cc` emits `cc:license` + `cc:attributionName`;
`--profile dcterms` emits `dcterms:license` / `dcterms:rightsHolder` /
`dcterms:rights` (the flat Dublin Core rights view).

## SOTA and how GMEOW transcends it

| Standard | What it gives | What GMEOW adds |
|---|---|---|
| **ODRL 2.2** | permission / prohibition / duty policies | grounds the policy in a gUFO relator that also carries copyright, trademark, attribution and time-scope; ODRL is a *generated projection* |
| **CC REL** | licence + attribution | the same, plus full deontic detail (projected to ODRL) and the SPDX identifier |
| **dcterms / schema.org** | flat `license` / `rightsHolder` / `copyright*` | the reified relator they flatten *from* — lossless source, lossy projection |
| **SPDX** | licence identifiers + SBOM | ties the identifier to a first-class `gmeow:License` agreement with parties and a deontic policy |
| **RightsStatements.org / PREMIS 3** | cultural-heritage / preservation rights statements | all 12 RightsStatements.org statuses + the verified PREMIS 3 rights basis (`premis:Copyright`/`License`/`RightsStatus`/`act`/`allows`/`restriction`) on a reasoning-grounded model |
| **W3C Ontology for Media Resources** | media copyright / policy properties | aligned to `ma:copyright` / `isCopyrightedBy` / `hasPolicy` / `hasPermissions` |
| **WIPO / trademark registries** | registration data | a reified `gmeow:Trademark` (mark × holder × registration × ™/®/status), linked to Wikidata; the IP-right *types* are a `gmeow:RightsType` vocabulary |
| **MPEG-21 (ISO/IEC 21000) REL / IPROnto** | XML rights-expression language / IPR ontology | the canonical antecedents — bridged by reference via ODRL + the Rights-Expression-Language and MPEG-21 Wikidata items (no fabricated IRIs) |

## Doctrine

- **Principle 4 — one canonical source, lossy projections.** Rights are authored
  once as GMEOW relators; ODRL, CC REL and schema.org are generated downcasts.
- **Principle 5 — maximal superset, by reference.** One canonical term per
  concept, aligned to ODRL / CC REL / dcterms / schema.org / SPDX / RightsStatements
  / PREMIS / Wikidata — never importing an external axiom.
- **Principle 6 — greenfield.** `gmeow:hasLicense` (not a back-compatible
  `gmeow:license`); the reified relator, not a flat-string shortcut, is canonical.
- **Principle 9 — no overtyping, no single winner.** Actions / families / statuses
  are open value vocabularies; there is no preferred / primary right (enforced by
  `gmeow:NoPreferredClaimShape` and a term-absence test).
- **Principle 10 — suppression, never erasure.** An expired / cancelled right keeps
  its record (`gmeow:trademarkStatusExpired`, `gmeow:validUntil`) and is suppressed
  for display, never deleted.
