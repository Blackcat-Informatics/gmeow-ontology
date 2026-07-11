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
expression.

A **`gmeow:NotationSystem`** is a structured symbolic system with defined rules
for representing information in a specific domain. It is a SubKind of
`SymbolicSystem`.

**The lang: graft (Principle 19).** Both classes are grafted onto the language
grounding layer: `gmeow:SymbolicSystem` and `gmeow:NotationSystem` are
`rdfs:subClassOf lang:SignSystem`. A symbolic system therefore *is* a sign
system, distinguished not by a local class ladder but by a **`lang:SignSystemKind`**
through `lang:signSystemKind` (a notation system carries `lang:notationalKind`)
and a **`lang:Modality`** through `lang:modality` (written, spoken, signed,
tactile). The former language-vs-notation boundary survives as a kind-individual
distinction — `lang:notationalKind` versus `lang:naturalLanguageKind` /
`lang:formalLanguageKind` — rather than a subclass overlap. A `Language` is
likewise a `lang:SignSystem` (grafted in the language slice), so a language and
a notation are co-equal sign systems separated by their `lang:signSystemKind`,
and the domain axis of a symbol system (musical, mathematical, cryptographic)
rides the orthogonal `gmeow:symbolicSystemKind` / `gmeow:notationSystemKind`.

The explicit bridging properties (`hasNotationSystem`, `writingSystemAsNotation`)
still do the cross-family linking that the grounding kinds keep out of the class
hierarchy: a script is a `lang:Script`, and `writingSystemAsNotation` bridges it
to the `NotationSystem` facet when a script doubles as a notation (Braille,
featural scripts).

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
| **TeX / LaTeX** | `Language` (`lang:formalLanguageKind`) | Grammar-defined with parseable syntax and semantics |
| **MathML** | `Language` (`lang:formalLanguageKind`) | XML grammar with defined semantics |
| **MusicXML / MEI** | `Language` (`lang:formalLanguageKind`) **or** `NotationSystem` | Grammar-defined encoding; also musical notation — **co-modelable via standpoint** |
| **MIDI** | `Language` (`lang:formalLanguageKind`) **or** `NotationSystem` | Protocol with defined structure; also encoding — **co-modelable via standpoint** |
| **ABC notation** | `Language` (`lang:formalLanguageKind`) **or** `NotationSystem` | Text-based music notation with grammar — **co-modelable via standpoint** |
| **LilyPond** | `Language` (`lang:programmingLanguageKind`) **or** `NotationSystem` | Programming language for music engraving; also music notation — **co-modelable via standpoint** |

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
   be `Language` instances of `lang:formalLanguageKind` (or, for LilyPond,
   `lang:programmingLanguageKind`) when treated as grammar-defined encodings.**
   They have parseable syntax and defined semantics. They may ALSO be modeled as
   `NotationSystem` via co-modeling — a single entity can carry both
   classifications from different standpoints (Principle 9).

5. **Emoji, gesture, meme, and platform conventions are notation or
   communication conventions unless modeled as full languages by a standpointed
   claim.** They lack generative syntax and are convention-based.

## Usage pattern: NotationSystemUsage

The reified relator `NotationSystemUsage` binds an entity to a notation system
with a role and interval, mirroring names' `NameUsage`:

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

A system may be classified as both a formal-language sign system
(`lang:formalLanguageKind`) and a `NotationSystem` from different standpoints.
GMEOW handles this through co-equal, standpoint-indexed claims (Principle 9),
not subclass overlap:

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

## Projection framework: NotationProjectionProfile

A **`gmeow:NotationProjectionProfile`** is a `gmeow:Profile` (from the core
profiles slice) that describes how a `NotationSystem` projects canonical,
frame-relative content. It is deliberately not the canonical content itself;
it is a machine-readable, honest declaration of what survives the projection
and what is lost (Principles 4, 11, 12).

Every profile states:

* **`gmeow:notationSystemOf`** — exactly one `NotationSystem` being described.
* **`gmeow:representableParameter`** — parameters the notation can carry
  without loss (range is open in core; domain slices constrain it, e.g. to
  `MusicalParameter` in the music extension).
* **`gmeow:declaredLoss`** — `ProjectionLoss` individuals that explain what
  the notation drops or approximates.
* **`gmeow:projectionFunction`** — an FnO function reference that performs the
  render.

A **`gmeow:ProjectionLoss`** is an abstract individual type (value vocabulary;
never subclassed). Each loss may **`gmeow:accountsForParameter`** one or more
parameters so that completeness gates can prove every parameter is either
represented or explicitly accounted for.

The core framework intentionally stays domain-agnostic. The music extension
provides the concrete `MusicalParameter` vocabulary, music-domain
`NotationSystem` individuals, and per-system projection profiles.

### The rendering graft: a projection profile *is* a rendering convention

Under the lang: graft (Principle 19), a concrete render of canonical content
through a notation is a **`lang:Rendering`** — the shared reified crossing
theory, reused rather than forked (mirroring `math:ExpressionRendering`). A
projection profile is the **rendering convention**: a rendering points at it
through `lang:renderingConvention`, fixes `lang:renderingKind lang:renderingNotation`,
names the produced surface through `lang:renderingForm`, and carries a
`logic:PreservationKind` judgment through `lang:renderingPreservation`. The
profile's `declaredLoss` ledger is the fine-grained, per-parameter account
underneath that single coarse preservation grade. No notation-local preservation
enum or display-kind ladder is minted.

The rendering-family terms are dispositioned against `lang:Rendering` /
`lang:renderingPreservation` as follows — each is **retained** because none is
*strictly* superseded, and each retention is recorded as a negative-supersession
cell in `mappings/equivalences.ttl`:

| Term | Disposition | Why not superseded |
|---|---|---|
| `NotationProjectionProfile` | **Retained** | The parameter/loss ledger and FnO binding a `lang:Rendering` has no term for; it *is* the rendering convention a rendering references. |
| `ProjectionFunction` (`⊑ fno:Function`) | **Retained** | FnO executable-transform semantics; `lang:Rendering` is a declarative crossing, not a transform. |
| `ProjectionLoss` | **Retained** | Fine-grained per-parameter loss; finer than the coarse `logic:PreservationKind` a `lang:renderingPreservation` records. |
| `NotationSystemUsage` | **Retained** | The observation-spine usage relator (who/when/role) — orthogonal to a content→form crossing. |
| `NotationUsageRole` | **Retained** | Usage-function value vocabulary — a different axis from `lang:RenderingKind`. |
| `SymbolicSystemKind` | **Retained** | The domain axis of a symbol system — orthogonal to the `lang:SignSystemKind` axis. |

See `examples/notation-systems.ttl` for a worked profile realized as a
`lang:Rendering` with a `logic:SoundUnderApproximation` preservation judgment.

## Terms

### gmeow:SymbolicSystem · gmeow:NotationSystem · gmeow:SymbolicSystemKind · gmeow:symbolicSystemKind · gmeow:notationSystemKind

A `SymbolicSystem` is a first-class `InformationObject` and a `lang:SignSystem`
(the lang: graft) — a convention-based system of symbols, a sign system
distinguished by its `lang:signSystemKind` and `lang:modality`. A
`NotationSystem` is a structured `SubKind` of it (of `lang:notationalKind`) with
defined representation rules in a specific domain. `SymbolicSystemKind` values
classify the orthogonal domain axis via `symbolicSystemKind` /
`notationSystemKind` (transcription, encoding, musical, mathematical, …).

### gmeow:hasNotationSystem · gmeow:notationSystemFor · gmeow:writingSystemAsNotation

The explicit bridging properties that do the cross-family linking the grounding
kinds keep out of the class hierarchy: relating an entity to a notation system it
uses, its inverse, and the bridge that views a `lang:Script` as a
`NotationSystem`.

### gmeow:NotationSystemUsage · gmeow:NotationUsageRole · gmeow:notationUsageTarget · gmeow:notationUsageNotationSystem · gmeow:notationUsageRole · gmeow:notationUsageInterval

The reified relator binding an entity to a notation system with a role and an
interval, mirroring names' `NameUsage`: `notationUsageTarget` the entity,
`notationUsageNotationSystem` the system, `notationUsageRole` a `NotationUsageRole`
value (transcription, representation, …), and `notationUsageInterval` the span it
held.

### gmeow:NotationProjectionProfile · gmeow:hasNotationProjectionProfile · gmeow:notationSystemOf · gmeow:representableParameter · gmeow:projectableExpression

A `Profile` (from the core profiles slice) declaring how a `NotationSystem`
projects canonical, frame-relative content — honestly, not the canonical content
itself. `notationSystemOf` names the one system described,
`representableParameter` the parameters it carries without loss, and
`projectableExpression` the expressions it can render; `hasNotationProjectionProfile`
attaches it.

### gmeow:ProjectionLoss · gmeow:declaredLoss · gmeow:accountsForParameter · gmeow:ProjectionFunction · gmeow:projectionFunction

A `ProjectionLoss` is an abstract value individual (never subclassed) explaining
what a notation drops or approximates; `declaredLoss` lists them on a profile and
`accountsForParameter` ties each to the parameters it covers so completeness gates
can prove every parameter is represented or accounted for. A `ProjectionFunction`
referenced by `projectionFunction` is the FnO function that performs the render.

### gmeow:smuflCodepoint

A Unicode codepoint reference in the Standard Music Font Layout (SMuFL)
specification identifying a glyph used by a `NotationSystem`; multiple codepoints
may be asserted as multiple triples.
