<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Notation and Symbolic Systems — modelling & interoperability guide

Most vocabularies conflate symbol systems with languages: a musical score is
"in English", mathematical notation is "a language", emoji are "characters in
a language". GMEOW rejects this collapse. A symbol system is **not** a language
by default; it becomes one only through a **standpointed claim** (Principle 9).

## Governing tenet: neutral symbolic systems

A **`gmeow:SymbolicSystem`** is a first-class `InformationObject` — a system of
symbols, signs, or conventions used for communication, representation, or
expression. It sits alongside `Language` and `WritingSystem` as a sibling under
`InformationObject`, not as a subclass of either.

A **`gmeow:NotationSystem`** is a structured symbolic system with defined rules
for representing information in a specific domain. It is a SubKind of
`SymbolicSystem`.

**Why the sibling approach?** Making `WritingSystem` a subclass of
`NotationSystem` would force all writing systems to carry notation-specific
properties (domain, encoding scheme) and all notation systems to carry
writing-system properties (ISO 15924 code, text direction). The sibling
approach keeps each class minimal and lets explicit bridging properties
(`hasNotationSystem`, `writingSystemAsNotation`) do the linking.

## The boundary: language vs notation vs symbolic system

| Criterion | `Language` | `NotationSystem` | `SymbolicSystem` |
|---|---|---|---|
| Generative syntax/semantics | **Yes** (independent) | **No** (parasitic or domain-bound) | **No** (convention-based) |
| Parseable serialization | Often | Sometimes (as encoding) | Rarely |
| Human native speakers | May have | Never | Never |
| Domain specificity | General communication | Specific domain (math, music, crypto) | Any convention |
| Structured rules | Grammar | Representation rules | Social/platform conventions |

## Decision table

| System | GMEOW classification | Rationale |
|---|---|---|
| **IPA** | `NotationSystem` (transcription) | Phonetic representation of spoken language; parasitic, not generative |
| **Morse code** | `NotationSystem` (encoding) | Signal encoding of text; no independent syntax/semantics |
| **Stenography** | `NotationSystem` (shorthand) | Speed-writing system for a specific language |
| **Cipher systems** | `NotationSystem` (cryptographic) | Transform scheme; not a language unless standpointed |
| **Emoji conventions** | `SymbolicSystem` (communication) | Convention-based symbols without generative syntax |
| **Mathematical notation** | `NotationSystem` (mathematical) | Domain-specific representational rules |
| **TeX / LaTeX** | `FormalLanguage` | Grammar-defined with parseable syntax and semantics |
| **MathML** | `FormalLanguage` | XML grammar with defined semantics |
| **MusicXML / MEI** | `FormalLanguage` **or** `NotationSystem` | Grammar-defined encoding; also musical notation — **co-modelable via standpoint** |
| **MIDI** | `FormalLanguage` **or** `NotationSystem` | Protocol with defined structure; also encoding — **co-modelable via standpoint** |
| **ABC notation** | `FormalLanguage` **or** `NotationSystem` | Text-based music notation with grammar — **co-modelable via standpoint** |
| **LilyPond** | `FormalLanguage` **or** `NotationSystem` | Programming language for music engraving; also music notation — **co-modelable via standpoint** |

### Boundary rules

1. **Stenography is usually a notation system for an existing language.** It has
   no independent generative syntax; it encodes an existing language in
   abbreviated form.

2. **Cryptographic ciphers / encodings are not languages by default.** They are
   transform schemes. A standpoint may claim a cipher as a language (e.g. a
   conlang built on cipher principles), but the default classification is
   `NotationSystem`.

3. **IPA and Morse code are notation / transcription / encoding systems, not
   natural languages.** They lack independent syntax and semantics.

4. **TeX, MathML, OpenMath, MusicXML, MEI, MIDI, LilyPond, and ABC notation may
   be `FormalLanguage` instances when treated as grammar-defined encodings.**
   They have parseable syntax and defined semantics. They may ALSO be modeled as
   `NotationSystem` via co-modeling — a single entity can carry both
   classifications from different standpoints (Principle 9).

5. **Emoji, gesture, meme, and platform conventions are notation or
   communication conventions unless modeled as full languages by a standpointed
   claim.** They lack generative syntax and are convention-based.

## Usage pattern: NotationSystemUsage

The reified relator `NotationSystemUsage` binds an entity to a notation system
with a role and interval, mirroring `WritingSystemUsage`:

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/notation/> .

ex:english a gmeow:Language .

ex:ipa a gmeow:NotationSystem ;
    gmeow:notationSystemKind gmeow:symbolicKindTranscription .

ex:englishUsesIpa a gmeow:NotationSystemUsage ;
    gmeow:notationUsageTarget ex:english ;
    gmeow:notationUsageNotationSystem ex:ipa ;
    gmeow:notationUsageRole gmeow:notationRoleTranscription ;
    gmeow:notationUsageInterval ex:ipaUsageInterval .
```

A musical work using staff notation:

```turtle
ex:beethoven9 a gmeow:CreativeWork .

ex:staffNotation a gmeow:NotationSystem ;
    gmeow:notationSystemKind gmeow:symbolicKindMusical .

ex:beethoven9UsesStaff a gmeow:NotationSystemUsage ;
    gmeow:notationUsageTarget ex:beethoven9 ;
    gmeow:notationUsageNotationSystem ex:staffNotation ;
    gmeow:notationUsageRole gmeow:notationRoleRepresentation ;
    gmeow:notationUsageInterval ex:staffUsageInterval .
```

## Co-modeling: when a system is both language and notation

A system may be classified as both a `FormalLanguage` and a `NotationSystem`
from different standpoints. GMEOW handles this through co-equal,
standpoint-indexed claims (Principle 9), not subclass overlap:

```turtle
ex:musicxml a gmeow:InformationObject .

# Standpoint A: MusicXML is a formal language (grammar-defined XML)
ex:claimA a gmeow:StandpointClaim ;
    gmeow:vantage ex:softwareEngineer ;
    gmeow:observedFeature ex:musicxml ;
    gmeow:observationResult gmeow:originFormal .

# Standpoint B: MusicXML is a musical notation system
ex:claimB a gmeow:StandpointClaim ;
    gmeow:vantage ex:musicLibrarian ;
    gmeow:observedFeature ex:musicxml ;
    gmeow:observationResult gmeow:symbolicKindMusical .
```

## Projections and lossy drops

| Target vocabulary | What maps | What's dropped |
|---|---|---|
| **SKOS** | `SymbolicSystem` → `skos:ConceptScheme`; `NotationSystem` → `skos:ConceptScheme` | Domain specificity, usage roles, temporal scope |
| **schema.org** | `SymbolicSystem` → `schema:DefinedTermSet` | Structured rules, reified usage |
| **MathML / OpenMath** | `NotationSystem` (mathematical) → math element container | Notation metadata, standpoint, temporal scope |
| **MusicXML / MEI** | `NotationSystem` (musical) → score container | Usage relator, confidence, standpoint |
| **MIDI** | `NotationSystem` (musical) → track/sequence | Human-readable notation semantics |

## Related work

- Parent epic: #169
- Lexical items and forms: #171
- Mathematical realm / OpenMath: #86
- Maximal projections: #98
