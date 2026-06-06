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
@prefix ex:    <https://example.org/names/> .

ex:patrick a gmeow:Person ; gmeow:hasName ex:nameLatin , ex:nameHan .   # co-equal

ex:nameLatin a gmeow:PersonName ;
    gmeow:fullName "Patrick Colm Audley"@en ; gmeow:nameLanguage "en" ;
    gmeow:namePurpose gmeow:namePurposeLegal ; gmeow:wasAttributedTo ex:patrick .

ex:nameHan a gmeow:PersonName ;
    gmeow:fullName "欧德理"@zh-Hans ; gmeow:nameLanguage "zh" ; gmeow:nameScript "Hani" ;
    gmeow:romanization "Ōu Délǐ"@zh-Latn ;          # romanization of THIS name only
    gmeow:namePurpose gmeow:namePurposeChosen ; gmeow:wasAttributedTo ex:patrick .
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
ex:gennyMrs  a gmeow:PersonName ; gmeow:fullName "Mrs Smith"@en .
ex:gennyAunt a gmeow:PersonName ; gmeow:fullName "Aunt Genny"@en .

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
    gmeow:fullName "Alex Rivera"@en ; gmeow:namePurpose gmeow:namePurposeChosen ;
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
inventory and sourcing live in [`identity-mapping.md`](./identity-mapping.md#pronoun-set-inventory-the-address-axis).

**Linkage & projection.** `gmeow:PronounSet` / `gmeow:hasPronounSet` `closeMatch` Wikidata's
*personal pronoun set* `wd:Q65067284` / *personal pronoun* `wdt:P6553` (verified live; Wikidata's
2025 RfC converged on the five-form model). The projection layer flattens a set's five forms to
the compact `"she/her"` string for the **vCard 4 PRONOUNS** property (RFC 9554) via
`fnPronounSetToText`, emitted on the `vcardx:pronouns` extension term (the W3C vCard RDF ontology
has no pronoun predicate) — a documented lossy downcast.

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
an entity's other names). `gmeow:alternateName` (places module) remains for
gazetteer matching.

## What's deliberately non-standard (and why)

| GMEOW choice | The "standard" alternative | Why we reject it |
|---|---|---|
| A name is a reified `Appellation` + `NameUsage` relator | `familyName` datatype property | Flat properties can't carry time, context, audience, or evidence |
| No `preferredForDisplay` / primary name | one canonical name + alternates | A primary-name slot encodes colonial hierarchy between co-equal names |
| Display selection is locale-relative | a single "display name" | A global display name silently re-centers one language/script |
| Pronouns/honorifics independent of sex | derive pronoun from gender | Conflates distinct facets; erases self-identification |
| `partOrder` descriptive, surname optional | given + family, in that order | Parochial; breaks for the majority of the world's naming systems |
| Claimed vs detected media type coexist | trust the extension | The name can lie; the model should record disagreement, not hide it |
