<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Names — modelling & interoperability guide

Most data models treat a name as an *attribute of a thing*:
`person.familyName = "Smith"`. That single assumption fails the moment a name
**changes** (marriage, divorce, transition), is **context-dependent** (Aunt Genny
to family, Mrs Smith to students), carries **facets** (pronouns, honorifics), or
belongs to a **non-person** that can *lie about itself* (a `.pdf` that isn't a
PDF). GMEOW therefore models a name as a **reified, time-bounded, context-scoped,
source-attributed relationship** — never a bare property.

This guide is longer than most because the model is deliberately **non-standard**.
That non-standardness is the point: standard name models encode assumptions that
are, at best, parochial and, at worst, harmful.

## The reframe

| Real-world fact | What a flat `familyName` cannot express | GMEOW axis |
|---|---|---|
| Karen Anne Alpha → Karen Beta → Charles Alpha | a name is not stable | **time** |
| Aunt Genny vs Mrs Smith | a name is not universal | **context / audience** |
| names travel with pronouns & titles | a name is not just a string | **facets** |
| `invoice.pdf` is actually a ZIP | names aren't only for people, and can be wrong | **scope + claim-vs-reality** |
| Patrick Colm Audley **and** 欧德理 | a person has co-equal names in different scripts | **co-equality** |

Two classes carry the whole model:

- **`gmeow:Appellation`** *(an `InformationObject`)* — the name **as an object**:
  its surface form (`gmeow:fullName`), its structured parts (`gmeow:hasNamePart`),
  its language/script, its purpose. Subclasses: `gmeow:PersonName`,
  `gmeow:Filename`, `gmeow:PlaceName`, `gmeow:OrganizationName`.
- **`gmeow:NameUsage`** *(a `gufo:Relator`)* — the **use** of an appellation in
  context: who is named, by/among whom, in what register, over what period. This
  is the same reification idiom as `gmeow:Certification` and
  `gmeow:InterpersonalRelationship`.

The appellation is the noun; the usage is the relator that situates it.

## Governing tenet: co-equality of names (anti-colonial naming)

The schema's **shape** can enact colonial hierarchy. A single canonical `name`
slot plus a bag of `alternateName`s declares "one identity is the truth, the rest
are deviations." GMEOW structurally refuses this.

> The project owner is **Patrick Colm Audley** *and* **欧德理**. These are
> **co-equal full names** of one person. 欧德理 is not an `alternateName`, not a
> romanization of "Audley", not a footnote — it is a name, family-first (姓 欧 →
> 名 德理), chosen and meaningful in its own right.

Binding rules, enforced by `tests/test_names.py`:

1. **No primary name.** There is deliberately **no `preferredForDisplay`,
   `primaryName`, or `canonicalName` term.** A person bears many co-equal
   `gmeow:PersonName`s via `gmeow:hasName`.
2. **No derivation arrow between co-equal names.** Even where one name arose from
   sound-mapping another, origin ≠ subordination. `gmeow:romanization` relates a
   name only to a transliteration of *itself* — it never bridges two names.
3. **Display selection is locale-relative and symmetric.** Match the name's
   `gmeow:nameLanguage` / `gmeow:nameScript` to the audience's locale. For a
   Sinophone audience 欧德理 is shown and "Patrick" is fallback; for an Anglophone
   audience, the reverse. Neither is *the* name.
4. **Self-assertion is top authority.** A name the named person asserts
   (`gmeow:wasAttributedTo` the person) outranks registry/import assertions and
   must not be silently overwritten — the same root as deadname suppression.
5. **No imposed structure.** No required given+family split; mononyms and
   non-Western part systems are first-class; `gmeow:partOrder` is descriptive,
   never normative.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix ex:    <https://example.org/names/> .

# --- Languages ---
ex:langEn a gmeow:Language ; gmeow:languageCode "en" .
ex:langZh a gmeow:Language ; gmeow:languageCode "zh" .

# --- Person and Names ---
ex:patrick a gmeow:Person ; gmeow:hasName ex:nameLatin , ex:nameHan .   # co-equal

ex:nameLatin a gmeow:PersonName ;
    gmeow:fullName "Patrick Colm Audley"@x-gmeow-english ;
    gmeow:nameLanguage ex:langEn ;
    gmeow:namePurpose gmeow:namePurposeLegal ;
    gmeow:wasAttributedTo ex:patrick .

ex:nameHan a gmeow:PersonName ;
    gmeow:fullName "欧德理"@x-gmeow-chinese ;
    gmeow:nameLanguage ex:langZh ;
    gmeow:nameScript lang:hanScript ;                    # a first-class lang:Script, not a bare code
    gmeow:romanization "Ōu Délǐ"@x-gmeow-chinese-latn ;  # romanization of THIS name only
    gmeow:namePurpose gmeow:namePurposeChosen ;
    gmeow:wasAttributedTo ex:patrick .

# --- The naming→referent bridge (Principle 19): the name's form MEANS the
#     person. That meaning is a reified lang:Denotation, reached from the
#     appellation through gmeow:appellationDenotation. Bearing (hasName) stays;
#     this layers the reference reading on top. Co-reference = shared target.
ex:nameHan gmeow:appellationDenotation ex:hanDenotation .

ex:hanForm a lang:WordForm ; lang:inSignSystem ex:langZh .

ex:hanDenotation a lang:Denotation ;
    lang:denotedForm ex:hanForm ;
    lang:denotationKind lang:denotesEntity ;   # the target is an entity, the person
    lang:denotationTarget ex:patrick ;
    lang:denotationContext ex:sinophoneAddress .
```

> **Multilingualism is a separate, forthcoming building block.** This module is
> *multilingual-ready* (co-equal names, per-name language/script, locale-relative
> display) but the full locale-resolution machinery is deferred.

## Structured parts — a multi-cultural value vocabulary

A `gmeow:NamePart` is a reified, typed, optionally-ordered component. Its kind is
the **value** `gmeow:namePartType` (open vocabulary, never a subclass — the
`placeType` idiom). `gmeow:partOrder` records *observed* order and never implies a
given-before-family default.

| Naming system | Parts (`namePartType` value) | Example |
|---|---|---|
| Anglo | `namePartGiven`, `namePartMiddle`, `namePartSurname` | Mary Lucille Smith |
| East-Asian (family-first) | `namePartSurname` (order 0), `namePartGiven` (order 1) | 欧 德理 / 山田 太郎 |
| Spanish double surname | `namePartGiven`, `namePartPaternalSurname`, `namePartMaternalSurname` | José **García** **Pérez** |
| Arabic | `namePartIsm`, `namePartKunya`, `namePartNasab`, `namePartLaqab`, `namePartNisba` | Muhammad **ibn Musa** **al-Khwarizmi** |
| Icelandic / Slavic | `namePartPatronymic` / `namePartMatronymic` | Sigríður **Jónsdóttir** |
| South-Indian (Tamil) | `namePartInitial` + `gmeow:partExpansion` | **R.** Kannan |
| Mononym | `namePartMononym` (no surname) | Plato, Sukarno |
| Regnal / religious | `namePartReligiousName` | Pope **Francis** |
| East-Asian courtesy / pen name | `namePartCourtesyName` | zi / hao; "Mark Twain" |
| Nobiliary particle | `namePartParticle` | **von**, **de**, **van**, **al-**, **bin** |
| Generational suffix | `namePartGenerationalSuffix` | Jr., **Sr.** |
| Generational ordinal | `namePartGenerationalOrdinal` (distinct from the suffix) | Charles Beaumont Sr. **III** / "the Third" |
| East-Asian generation name | `namePartGenerationName` | 林**文**豪 (文 shared by same-generation kin) |
| Clan / lineage name | `namePartClanName` | Korean **김해** (Gimhae) bon-gwan; Mongolian *ovog* |
| Birth-order / day name | `namePartBirthOrderName` | Balinese **Wayan**; Akan **Kofi** (Friday-born) |
| Teknonym (parent-of) | `namePartTeknonym` | Indonesian **Ibu Sari** (mother of Sari) |
| House / estate name | `namePartHouseName` | Germanic **Müllers** Hans (*Hofname*) |
| Roman *nomina* | `namePartPraenomen` / `namePartNomen` / `namePartCognomen` / `namePartAgnomen` | **Publius Cornelius Scipio Africanus** |
| Filename | `namePartStem`, `namePartExtension` | `invoice` + `pdf` |

The authoritative display string is always `gmeow:fullName` (in the name's natural
order). **Parts are for matching and decomposition, not for reassembling order** —
this is the core W3C *Personal names around the world* lesson.

## Context: the `NameUsage` relator (Aunt Genny)

"Mrs Smith to students, Aunt Genny to family" is one person, one era, two names —
distinguished by **who is using the name and in what register**. A `NameUsage`
binds the parts:

```turtle
ex:genny a gmeow:Person ; gmeow:hasName ex:gennyMrs , ex:gennyAunt .
ex:gennyMrs  a gmeow:PersonName ; gmeow:fullName "Mrs Smith"@x-gmeow-english .
ex:gennyAunt a gmeow:PersonName ; gmeow:fullName "Aunt Genny"@x-gmeow-english .

ex:usageStudents a gmeow:NameUsage ;          # formal, toward an audience
    gmeow:usageNamed ex:genny ; gmeow:usageAppellation ex:gennyMrs ;
    gmeow:usageRegister gmeow:registerFormal ; gmeow:usageAudience ex:students .

ex:usageFamily a gmeow:NameUsage ;            # intimate, scoped to a relationship
    gmeow:usageNamed ex:genny ; gmeow:usageAppellation ex:gennyAunt ;
    gmeow:usageRegister gmeow:registerIntimate ;
    gmeow:usageRelationshipScope ex:auntNieceTie .
```

`usageNamer`/`usageAudience` are non-functional and **perspectival** — a usage is
*somebody's*, never a global fact, so the model never derives a "true" name.

## Temporal change & inclusive transition

Names change; former names may be deadnames. Model each life-stage name as its own
co-equal `PersonName`, link the cause to the events module's event spine
(`gmeow:conferredByEvent` → a `gmeow:LifeEvent` with `gmeow:eventType`
`gmeow:eventTypeNameChange` / `gmeow:eventTypeMarriage`), and use the
**only** display control — `gmeow:displayable` — to suppress a deadname:

```turtle
ex:alex a gmeow:Person ; gmeow:hasName ex:alexChosen , ex:alexFormer .
ex:alexChosen a gmeow:PersonName ;
    gmeow:fullName "Alex Rivera"@x-gmeow-english ; gmeow:namePurpose gmeow:namePurposeChosen ;
    gmeow:displayable true ; gmeow:wasAttributedTo ex:alex .
ex:alexFormer a gmeow:PersonName ;
    gmeow:namePurpose gmeow:namePurposeDeadname ;
    gmeow:displayable false ;                      # consumers MUST honour this
    gmeow:conferredByEvent ex:alexNameChange .
```

There is no priority ranking — `displayable false` is a hard suppression, and the
chosen name surfaces by locale match. The `DeadnameSuppressionShape`
(`shapes/gmeow-shapes.ttl`) warns when a superseded/deadname omits the flag.

## Pronouns & honorifics — facets, independent of sex

Pronouns and honorifics are **first-class, contextual, temporal, and sex/gender
independent** — there is **no** axiom tying `gmeow:hasPronounSet` or
`gmeow:honorific` to `gmeow:sex`, and `test_names.py` enforces that nothing ever
infers one from the other.

```turtle
ex:alex gmeow:hasPronounSet ex:faeSet ; gmeow:honorific gmeow:honorificMx .

# Known sets are value individuals (she/her, they/them, xe/xem, ze/hir, …); any
# other set is expressed by filling the five English forms.
ex:faeSet a gmeow:PronounSet ;
    gmeow:pronounSubject "fae" ; gmeow:pronounObject "faer" ;
    gmeow:pronounPossessiveDeterminer "faer" ; gmeow:pronounPossessive "faers" ;
    gmeow:pronounReflexive "faerself" .
```

The seeded `gmeow:PronounSet` anchors are a **maximal, source-cited inventory** of 21
stably-declinable English sets (she/her, he/him, they/them, it/its; Spivak ey/em and
Elverson e/em; ze/hir, ze/zir, xe/xem, fae/faer, ae/aer, ve/ver, vi/vir, per/per, ne/nem,
thon, co/cos, hu/hum, ki/kin, zhe/zher, generic one) — declensions **verified against the
[pronouns.page](https://en.pronouns.page) structured database** — plus three non-specifying
values that carry no forms by design: `pronounAny`, `pronounAsk`, and the explicit
**`pronounNameOnly`** ("use my name (no pronouns)") nounself stance. The anchors are not a
fence: mint a fresh `PronounSet` filling the five forms for anything unseeded. The full
inventory and sourcing live in [`identity-mapping.md`](../../../docs/identity-mapping.md#pronoun-set-inventory-the-address-axis).

**Linkage & projection.** `gmeow:PronounSet` / `gmeow:hasPronounSet` `closeMatch` Wikidata's
*personal pronoun set* `wd:Q65067284` / *personal pronoun* `wdt:P6553` (verified live; Wikidata's
2025 RfC calls for full-declension sets, aligning with GMEOW's five-form English model). The
projection layer renders a set's **full five-form declension** as one slash-joined string
(`"she/her/her/hers/herself"`) for the **vCard 4 PRONOUNS** property (RFC 9554) via
`fnPronounSetToText`, emitted on the `vcardx:pronouns` extension term (the W3C vCard RDF ontology
has no pronoun predicate). PRONOUNS is free text, so the declension is carried **losslessly** —
no compact `"she/her"` flatten; only period/standpoint and the non-specifying values are dropped.

Honorifics carry `gmeow:honorificPosition` (prefix `Dr Smith` vs suffix
`Tanaka-san`) and `gmeow:honorificClass`; gender-neutral (`Mx`) and non-Western
(`-san`, `Sri`, `Sayyid`) honorifics are first-class.

## Filenames: claim vs reality

A filename's extension **claims** a content type; the bytes **detect** one. GMEOW
records both as coexisting claims — never a contradiction (no disjointness, no
`owl:differentFrom`, so benign aliasing like `pdf`/`x-pdf` can't make the graph
inconsistent):

```turtle
ex:invoiceFile a gmeow:Filename ;
    gmeow:fullName "invoice.pdf" ;
    gmeow:claimedMediaType "application/pdf" ;     # from the ".pdf" extension
    gmeow:detectedMediaType "application/zip" .    # from the magic bytes
```

A consumer flags the lie by string-comparing the two; the reasoner stays silent.
Attach `gmeow:confidence` / `gmeow:wasDerivedFrom` to weight the detector over the
extension.

## Interoperability

Term alignments live in `mappings/gmeow-names.sssom.tsv` (schema.org, vCard 4,
GEDCOM X, FOAF, Wikidata). There are **no flat given/family name properties** in
the canonical model — a name component is always a typed `gmeow:NamePart`, and a
"First Last" rendering for vCard / schema.org / FOAF is produced by **downcasting**
that structured model in the projection layer (`gmeow project`), never stored.

In the table below, a `namePart…` token is a **`gmeow:NamePartType` value** carried
by `gmeow:namePartType` on a `gmeow:NamePart` resource — *not* a predicate:

| vCard | GMEOW (structured `gmeow:NamePart`) | downcast to |
|---|---|---|
| `FN` | `gmeow:fullName` | `vcard:fn` / `schema:name` |
| `N` given-name | `gmeow:namePartType gmeow:namePartGiven` | `vcard:given-name` / `schema:givenName` |
| `N` family-name | `gmeow:namePartType gmeow:namePartSurname` | `vcard:family-name` / `schema:familyName` |
| `N` additional-name | `gmeow:namePartType gmeow:namePartMiddle` | `vcard:additional-name` |
| `N` honorific-prefix | `gmeow:namePartType gmeow:namePartHonorificPrefix` (+ `gmeow:honorific`) | `vcard:honorific-prefix` |
| `N` honorific-suffix | `gmeow:namePartType gmeow:namePartHonorificSuffix` | `vcard:honorific-suffix` |
| `NICKNAME` | `gmeow:namePartType gmeow:namePartNickname` | `vcard:nickname` |

The only flat name term retained is `gmeow:name` — the simple `rdfs:label` tier for
entities that don't need the full naming apparatus (it carries no precedence over
an entity's other names).

## Place naming — `hasPlaceName`, `PlaceNaming`, endonym/exonym (place-naming design)

A place's names are not a flat literal. A `gmeow:Place` bears co-equal
`gmeow:PlaceName` toponyms via **`gmeow:hasPlaceName`** (the place-scoped
specialization of `gmeow:hasAppellation`, mirroring `gmeow:hasName` for persons) —
the structured replacement for the **retired** flat `gmeow:alternateName` literal
(Principle 6, greenfield). Each `PlaceName` carries its own first-class
`gmeow:nameLanguage` (a `gmeow:Language`, never a bare tag) and an optional
`gmeow:namePurpose` of **`namePurposeEndonym`** (the name a place's own inhabitants
use) or **`namePurposeExonym`** (the name outsiders use) — co-equal, never a
preferred-vs-alternate pair. So *München* (endonym, German) and *Munich* (exonym,
English) are co-equal names of one place; a superseded historical name sets
`gmeow:displayable false`, never deleted (Principle 10).

The time/audience/authority-scoped *use* of a toponym reuses the existing
`gmeow:NameUsage` relator: **`gmeow:PlaceNaming` is a DEFINED class**,
`≡ gmeow:NameUsage ⊓ ∃gmeow:usageNamed.gmeow:Place` — the first `owl:equivalentClass`
in GMEOW. A name-usage that names a `Place` is *classified* as a `PlaceNaming` by the
reasoner (entailed, authored nowhere — see the `place-namings` competency query and
the entailment test), so no parallel place-naming relator is minted. Such a usage may
carry **`gmeow:usageAuthority`** (the toponymic / naming authority — a national
mapping agency, a standards body, an indigenous community), which is non-functional so
joint or competing authorities coexist with no privileged claimant (Principle 9).

| GMEOW | External alignment |
|---|---|
| `gmeow:PlaceName` | `crm:E48_Place_Name` (CIDOC-CRM) |
| `gmeow:hasPlaceName` | `gn:name`, `schema:name` (broad, downcast) |
| `gmeow:nameLanguage` | `dcterms:language`, `schema:inLanguage`, `wdt:P407` |
| `gmeow:namePurposeEndonym` | `wd:Q1266782` (endonym) |
| `gmeow:namePurposeExonym` | `wd:Q81639` (exonym) |

**Projection (schema.org).** `fnSelectEndonym` emits the displayable endonym as
`schema:name` (a projection *frame* choice, not a canonical primary), and
`fnSelectExonym` emits exonyms as `schema:alternateName`; historical/superseded and
competing-standpoint names are dropped (documented lossy drops).

## Cross-cutting multilingual labels — Organization, CreativeWork, Agreement, Software (multilingual-label design)

The Appellation pattern is not limited to persons and places. Every realm that bears names gets the same multilingual, anti-colonial machinery:

| Realm | Bearer property | Appellation subclass | Flat fallback |
|---|---|---|---|
| **Person** | `gmeow:hasName` | `gmeow:PersonName` | — |
| **Place** | `gmeow:hasPlaceName` | `gmeow:PlaceName` | — (replaces `alternateName`) |
| **Organization** | `gmeow:hasOrganizationName` | `gmeow:OrganizationName` | `gmeow:name` |
| **CreativeWork** | `gmeow:hasTitle` | `gmeow:CreativeWorkTitle` | `gmeow:title` |
| **Agreement** | `gmeow:hasAgreementName` | `gmeow:AgreementName` | — |
| **SoftwareProject** | `gmeow:hasSoftwareName` | `gmeow:SoftwareName` | `gmeow:name` |

Each bearer property is a `subPropertyOf` `gmeow:hasAppellation`, so the full multilingual stack applies: `gmeow:nameLanguage` → first-class `gmeow:Language`, `gmeow:nameScript`, `gmeow:romanization` + `gmeow:transliterationScheme`, `gmeow:displayable`, and `gmeow:namePurpose`. Co-equal multilingual names are **separate Appellation instances** — an organization's English legal name and its French exonym are peers, not primary-vs-alternate.

The flat fallbacks (`gmeow:name`, `gmeow:title`) remain for the 80 % case where multilingual depth is not needed, following the "flat-first, reify-on-demand" pattern.

## Terms

The anchors below index the slice's declared terms; the prose above is the full doctrine.

### gmeow:Appellation · gmeow:PersonName · gmeow:PlaceName · gmeow:OrganizationName · gmeow:Filename · gmeow:NamePart · gmeow:PronounSet

`gmeow:Appellation` is the name **as an information object** — the bearer of the surface form and the structured parts, with multiple appellations on one entity strictly **co-equal** (none canonical or primary). `gmeow:PersonName`, `gmeow:PlaceName`, `gmeow:OrganizationName`, and `gmeow:Filename` are structural subkinds for person, place, organization, and digital-file bearers; `gmeow:NamePart` is a reified, typed component of an appellation; `gmeow:PronounSet` is a sex/gender-independent set of third-person pronoun forms.

### gmeow:NameUsage · gmeow:usageNamed · gmeow:usageAppellation · gmeow:usageNamer · gmeow:usageAudience · gmeow:usageRelationshipScope · gmeow:usageRegister · gmeow:usageInterval · gmeow:usageAuthority

`gmeow:NameUsage` is the `gufo:Relator` situating the **use** of an appellation in context — who is named (`gmeow:usageNamed`), by which appellation (`gmeow:usageAppellation`), by whom (`gmeow:usageNamer`), toward what audience (`gmeow:usageAudience`) or within which standing tie (`gmeow:usageRelationshipScope`), in what register (`gmeow:usageRegister`), over what period (`gmeow:usageInterval`), and on whose authority (`gmeow:usageAuthority`). A usage is always *somebody's*, never a global fact, so it never derives a preferred or canonical name; the audience (who the name is used toward) is orthogonal to the authority (who asserts the usage).

### gmeow:PlaceNaming

A **defined** specialization of `gmeow:NameUsage` whose named entity is a `gmeow:Place` — `≡ gmeow:NameUsage ⊓ ∃gmeow:usageNamed.gmeow:Place`, the first `owl:equivalentClass` in GMEOW. Any name-usage that names a place is *inferred* to be a `gmeow:PlaceNaming`, so place naming reuses the relator rather than minting a parallel one; competing and historical toponyms coexist as co-equal place namings with no primary.

### gmeow:hasAppellation · gmeow:hasName · gmeow:hasPlaceName · gmeow:hasOrganizationName · gmeow:hasAgreementName · gmeow:hasNamePart

`gmeow:hasAppellation` is the universal name-bearing property; `gmeow:hasName`, `gmeow:hasPlaceName`, `gmeow:hasOrganizationName`, and `gmeow:hasAgreementName` are its person-, place-, organization-, and agreement-scoped specializations. All are non-functional — an entity bears many **co-equal** names, none primary (Principle 9). `gmeow:hasNamePart` (under the universal `gmeow:hasPart` spine) attaches the reified, typed components of an appellation.

### gmeow:namePartType · gmeow:NamePartType · gmeow:partText · gmeow:partOrder · gmeow:partExpansion

A `gmeow:NamePart`'s kind is the **value** `gmeow:namePartType` (a `gmeow:NamePartType` from the open, multi-cultural vocabulary — given, surname, patronymic, Arabic nisba, filename extension, …), never a subclass. `gmeow:partText` carries its language-/script-tagged string, `gmeow:partOrder` records the *observed* 0-based surface position **without** implying a given-before-family default, and `gmeow:partExpansion` carries the full word an abbreviated initial stands for. Parts are for matching and decomposition, not for reassembling display order — `gmeow:fullName` is authoritative for that.

### gmeow:fullName · gmeow:nameLanguage · gmeow:nameScript · gmeow:romanization · gmeow:namePurpose · gmeow:NamePurpose · gmeow:displayable · gmeow:conferredByEvent

`gmeow:fullName` is the complete surface form in the culture's natural order, authoritative for display. `gmeow:nameLanguage` is the appellation's single first-class `gmeow:Language` (functional — co-equal multilingual names are separate appellations, never one multi-tagged name), `gmeow:nameScript` the first-class **`lang:Script`** its surface is written in (regrounded onto the shared lang: script layer under Principle 19 — the ISO 15924 code rides the script's `skos:notation`, never a bare per-name tag), and `gmeow:romanization` a Latin transliteration of *this same* name that never bridges two co-equal names. `gmeow:namePurpose` tags the intrinsic kind(s) of a name (a `gmeow:NamePurpose` value — legal, chosen, deadname, endonym/exonym, …); `gmeow:displayable` is the **only** display control — there is deliberately no preferred/primary marker, so a superseded name or deadname sets it `false` and consumers MUST honour that. `gmeow:conferredByEvent` is the seam to the events spine, linking an appellation to the `gmeow:LifeEvent` that conferred or changed it.

### gmeow:appellationDenotation — the naming→referent bridge (lang: graft)

Bearing a name (`gmeow:hasName`/`gmeow:hasAppellation`) is not the whole story: a name's form **means** the entity it names, and that meaning is the reference corner of the Frege triangle. Under Principle 19 the names slice grounds it in the `lang:` layer instead of minting a local record. `gmeow:appellationDenotation` relates a `gmeow:Appellation` to a reified **`lang:Denotation`** whose `lang:denotedForm` is the appellation's analyzed (non-surface) `lang:Form`, whose `lang:denotationKind` is `lang:denotesEntity`, whose `lang:denotationTarget` is the named entity, and whose `lang:denotationContext` scopes the reading. Reification makes disagreement, revision, and ambiguity over *who a name refers to* representable rather than collapsed into a bare edge; the property is non-functional, and **co-reference is established by two appellations whose denotations share a `lang:denotationTarget`** — not by a same-string heuristic. The `gmeow:NameUsage` relator is retained (it carries the audience/register/period/authority/evidence axes a denotation has no term for); the denotation layers the meaning reading *on top of* the bearing.

### gmeow:claimedMediaType · gmeow:detectedMediaType

A filename's extension **claims** a content type (`gmeow:claimedMediaType`); the bytes **detect** one (`gmeow:detectedMediaType`). Both are non-functional and may disagree — the mismatch is recorded as coexisting confidence-weighted claims, never reasoned into an OWL contradiction.

### gmeow:hasPronounSet · gmeow:pronounSubject · gmeow:pronounObject · gmeow:pronounPossessiveDeterminer · gmeow:pronounPossessive · gmeow:pronounReflexive

`gmeow:hasPronounSet` relates a person to a `gmeow:PronounSet` they go by — a form of **address**, non-functional and contextual, that MUST NOT be inferred from (nor imply) gender identity, expression, sex assigned at birth, or orientation. A custom set is defined by filling the five English forms: `gmeow:pronounSubject`, `gmeow:pronounObject`, `gmeow:pronounPossessiveDeterminer`, `gmeow:pronounPossessive`, and `gmeow:pronounReflexive`.

### gmeow:honorific · gmeow:Honorific · gmeow:honorificPosition · gmeow:HonorificPosition · gmeow:honorificClass · gmeow:HonorificClass

`gmeow:honorific` records an honorific or title of address (a `gmeow:Honorific` value) — like pronouns, a sex/gender-independent, non-functional form of address. Each honorific carries `gmeow:honorificPosition` (a `gmeow:HonorificPosition`: rendered as prefix `Dr Smith` or suffix `Tanaka-san`) and `gmeow:honorificClass` (a `gmeow:HonorificClass`: academic, clerical, noble, military, judicial, social).

### gmeow:usageRegister · gmeow:NameRegister · gmeow:Register

A name-usage's social register is the value `gmeow:usageRegister` (a `gmeow:NameRegister` — formal, intimate, professional, casual — itself a `gmeow:Register`); register is a fact of the *use*, not of the name, distinct from the intrinsic `gmeow:namePurpose`.

## What's deliberately non-standard (and why)

| GMEOW choice | The "standard" alternative | Why we reject it |
|---|---|---|
| A name is a reified `Appellation` + `NameUsage` relator | `familyName` datatype property | Flat properties can't carry time, context, audience, or evidence |
| No `preferredForDisplay` / primary name | one canonical name + alternates | A primary-name slot encodes colonial hierarchy between co-equal names |
| Display selection is locale-relative | a single "display name" | A global display name silently re-centers one language/script |
| Pronouns/honorifics independent of sex | derive pronoun from gender | Conflates distinct facets; erases self-identification |
| `partOrder` descriptive, surname optional | given + family, in that order | Parochial; breaks for the majority of the world's naming systems |
| Claimed vs detected media type coexist | trust the extension | The name can lie; the model should record disagreement, not hide it |
