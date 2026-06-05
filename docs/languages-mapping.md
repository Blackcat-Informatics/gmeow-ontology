<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Languages — modelling & interoperability guide

Most vocabularies treat a language as an opaque tag: `inLanguage "ja"`. That single
assumption — *a language **is** its ISO/BCP-47 code* — structurally excludes the
languages that matter most to a forward-looking model:

| Real case | What an ISO code can't do |
|---|---|
| **Ithkuil** (engineered conlang) | has no ISO 639 code, and several incompatible versions |
| an **AI-minted interlingua** | has no code at all, and a *software* creator |
| an **under-coded sign / minority language** | the registry lags reality |
| **Japanese** orthography | co-mingles four scripts in one sentence |
| a **bespoke / non-linear** conlang script | isn't in ISO 15924 |

GMEOW inverts the assumption, exactly as the names block did for personal names.

## Governing tenet: registry-independent identity

A **`gmeow:Language` has a self-minted IRI.** Registry codes — BCP-47, ISO 639,
Glottolog, Wikidata — are **optional alignments** (`gmeow:languageCode`,
`gmeow:authorityLink`, `skos:exactMatch`), never the primary key. A code-less
conlang or AI-language is therefore a **fully first-class, co-equal** language.
`tests/test_languages.py` enforces that nothing requires a code.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/lang/> .

ex:ithkuil a gmeow:Language ;                          # NO languageCode — and that's fine
    gmeow:languageOrigin gmeow:originConstructedEngineered ;
    gmeow:designGoal "Maximal cognitive precision with minimal ambiguity." ;
    gmeow:wasAttributedTo ex:quijada ;                 # a human creator…
    gmeow:hasAppellation [ gmeow:fullName "Ithkuil"@en ; gmeow:namePurpose gmeow:namePurposeGlossonym ] .

ex:aiLang a gmeow:Language ;
    gmeow:languageOrigin gmeow:originAiGenerated ;
    gmeow:wasAttributedTo ex:modelAgent .              # …or a gmeow:SoftwareAgent
```

`★` The registry tag isn't discarded — it's **reconstructed on demand** by the
projection layer (see below), so schema.org consumers still get `ja-Hani` etc.

## Scope: one umbrella, many kinds

`gmeow:Language` covers natural, constructed (auxiliary / engineered / artistic /
ritual), AI-generated, historical / reconstructed, sign / whistled / tactile, and
formal languages. The single structural split is **`gmeow:FormalLanguage`** →
**`gmeow:ProgrammingLanguage`** (grammar-defined, machine modality, no native
speakers); a software project links to one via `gmeow:writtenInLanguage`.
Everything else is
distinguished by **value vocabularies**: `languageOrigin`, `languageModality`
(spoken / signed / written / whistled / tactile / machine / multimodal),
`languageStatus`.

## Writing systems are first-class — and co-mingled

A language uses **many co-equal scripts at once**. Japanese interleaves four, each
in a distinct *role* — so "a language has one script" is as wrong as "a person has
one name", and the fix is the same reified-usage relator (`WritingSystemUsage`,
mirroring names' `NameUsage`):

| Script | `scriptCode` | Role (`scriptRole`) | Example |
|---|---|---|---|
| Han (kanji) | `Hani` | `scriptRoleLogographicContent` | 山田 (content words) |
| Hiragana | `Hira` | `scriptRoleSyllabicGrammar` | は、を (grammar/okurigana) |
| Katakana | `Kana` | `scriptRoleLoanword` | コンピュータ (loanwords) |
| Rōmaji | `Latn` | `scriptRoleTransliteration` | "Yamada" (transliteration) |

```turtle
ex:japanese gmeow:usesWritingSystem ex:wsHan , ex:wsHiragana , ex:wsKatakana , ex:wsRomaji .
ex:wsuJaHan a gmeow:WritingSystemUsage ;
    gmeow:usageLanguage ex:japanese ; gmeow:usageWritingSystem ex:wsHan ;
    gmeow:scriptRole gmeow:scriptRoleLogographicContent .
# …three more usages, one per script + role.
```

The usage also carries a **period** (`gmeow:scriptUsageInterval` /
`validFrom`/`validUntil`), so a script change over time is just a closed usage —
Turkish in Arabic script *until 1928*, then Latin. Bespoke and non-linear scripts
(Ithkuil) are first-class: a `WritingSystem` may have **no** `scriptCode` and
`writingSystemType gmeow:wsTypeNonLinear`.

## Versions are a first-class lineage

A language evolves; AI languages version fast. Each version is itself a usable
`gmeow:Language` (a `gmeow:LanguageVersion`), linked up to its lineage by
`gmeow:versionOf` and ordered by `gmeow:supersedes` / `gmeow:wasDerivedFrom` —
older versions stay first-class.

```turtle
ex:ithkuil2011 a gmeow:LanguageVersion ;
    gmeow:versionOf ex:ithkuil ; gmeow:versionLabel "2011" ;
    gmeow:supersedes ex:ithkuil1993 ; gmeow:wasDerivedFrom ex:ithkuil1993 .
```

## Proficiency — reified, per-skill, leveled

A person knows a language *to a degree*, and differently per skill. GMEOW reifies
this (`gmeow:LanguageProficiency`, again the `NameUsage` idiom) — mint one per
(agent, language, modality), so "native overall" and "B2 writing" coexist:

| Scale (`proficiencyScale`) | Levels (`proficiencyLevel`) |
|---|---|
| CEFR | `cefrA1`…`cefrC2` |
| ILR / ACTFL | added as further individuals |
| (scale-free) | `levelNative`, `levelHeritage` |

```turtle
ex:profDeWriting a gmeow:LanguageProficiency ;
    gmeow:proficiencyAgent ex:learner ; gmeow:proficiencyLanguage ex:german ;
    gmeow:proficiencyModality gmeow:profModalityWriting ;
    gmeow:proficiencyLevel gmeow:cefrB2 ; gmeow:proficiencyScale gmeow:scaleCEFR .
```

The base `gmeow:knowsLanguage` (≈ `schema:knowsLanguage`) and `gmeow:nativeLanguage`
relations state *that* an agent knows a language; `gmeow:LanguageProficiency` adds
the level and the skill.

## Transformations are functions (FnO)

Transliteration, transcription and translation are *functions* — `script→script`
or `language→language` — so GMEOW catalogues them declaratively as
**`fno:Function`s** (`projections/transforms.fno.ttl`): Hepburn, Kunrei, Pinyin,
Wade-Giles, Revised Romanization, ISO 233 / 15919, IPA transcription, and a generic
translate. This closes a loop in the names block: `gmeow:romanization` now **names
the system that produced it** via `gmeow:transliterationScheme`, and each scheme
links to its FnO function.

```turtle
ex:yamadaName gmeow:romanization "Yamada Tarō"@ja-Latn ;
    gmeow:transliterationScheme gmeow:schemeHepburn .   # records HOW, not just WHAT
```

## Interoperability — the four-layer stack

| Layer | Carries | Artifact |
|---|---|---|
| SSSOM | 1:1 term links (Language ≈ `schema:Language` / `wd:Q34770`; `knowsLanguage` ≈ `schema:knowsLanguage`) | `mappings/gmeow-languages.sssom.tsv` |
| EDOAL | complex correspondences (relator→flat, code composition) | `projections/schema-org.edoal.ttl` |
| FnO | the transform functions | `projections/functions.fno.ttl` (+ `transforms.fno.ttl`) |
| CONSTRUCT | executor → pure schema.org | `queries/projections/schema-org.rq` |

The schema.org projection (run `gmeow project`) emits `schema:Language` /
`schema:ComputerLanguage`, **both** endonym and exonym as co-equal `schema:name`s,
the flattened `schema:knowsLanguage`, and — via `fnComposeBcp47` — the BCP-47 tag
as `schema:alternateName` (`de`+`Latn` → `de-Latn`; Japanese yields `ja-Hani`,
`ja-Hira`, `ja-Kana`, `ja-Latn`). Lossy drops: `WritingSystemUsage`, origin /
modality / status, version lineage, `LanguageCreation`, and the proficiency level.
Language **instances** coreference Lexvo (`http://lexvo.org/id/iso639-3/…`),
Glottolog and Wikidata in data via `skos:exactMatch` + `gmeow:authorityLink`.

## What's deliberately non-standard (and why)

| GMEOW choice | The "standard" alternative | Why we reject it |
|---|---|---|
| Self-minted IRI; codes optional | a language *is* its ISO/BCP-47 code | Excludes code-less conlangs, AI-languages, under-coded sign/minority languages |
| First-class `WritingSystem` + co-mingling relator | one `script` subtag | Can't represent Japanese's four roles, script change over time, or bespoke/non-linear scripts |
| Reified per-skill `LanguageProficiency` | proficiency stuffed in a label | Loses the scale, the skill, and the time |
| Version lineage of first-class languages | a `version` string | An Ithkuil version is itself a usable language with its own scripts/names |
| Transliteration/translation as FnO functions | a bare romanization literal | A romanization should record *how* it was derived (Hepburn vs Kunrei) |
| AI/software creator (`SoftwareAgent`) | only human authorship | Languages an AI invents are first-class, forward-looking |
