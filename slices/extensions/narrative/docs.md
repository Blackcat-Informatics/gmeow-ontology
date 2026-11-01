<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Narrative — canon as a reference frame, the text as a projection of the story

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/narrative` · **tier: extension**
> In-universe facts hold *accordingTo* a frame; the chapter sequence is the syuzhet projection of frame-relative story content.

A narrative is two things GMEOW keeps rigorously apart. A **canon** is a coordinate system:
locations, dates, and events inside the Harry Potter world, Earth-616, or the Star Wars
Legends split are positioned relative to a `gmeow:NarrativeReferenceFrame`, which is a
`gmeow:ReferenceFrame` (the Location-as-reference-frame design) living in the
`gmeow:frameRealmNarrative` defined by the places slice. The frame *functionally serves* as
the standpoint under which in-universe claims hold true, without subclassing
`gmeow:Standpoint` — gUFO's MixIden enforces exactly-one-Kind inheritance, so the frame
*is* the canon and `gmeow:accordingTo` does the standpoint work. Competing continuities are
separate frames that may sharpen one another; **no single canon wins** (Principle 9), variants
are linked by `gmeow:counterpartOf` and never merged (Principle 5), and a retcon is a
frame-scoped claim revision preserved by `gmeow:supersedes` + `gmeow:displayable false`
(Principle 10), never a deletion.

The second thing is the **text**, and the load-bearing doctrine of this slice is that **the
text is not the story**. A `gmeow:CreativeWork` (BookRelease, SerialInstallment) is an
out-of-universe, rights-bearing artifact that *sources, witnesses, or revises* a frame — it
is never the canon itself. Story content is frame-relative; the chapter sequence is a
*syuzhet projection* of it; the narration seam relates the two without confusing them; and
every interpretive reading — what the protagonist is, what a character felt in chapter 31,
whether a string is a motif — is a vantage-indexed claim coexisting with its rivals
(Principle 9). The sections below build that stance from frame to seam to interior.

## Frames, sourcing, and myth

### gmeow:NarrativeReferenceFrame

The canon, continuity, or narrative realm — a `gmeow:ReferenceFrame` SubKind that
coordinatizes in-universe locations, dates, and events and serves as the standpoint for
in-universe claims. Ordinary GMEOW entities (Person, Place, Event) carry claims
`gmeow:accordingTo` it; cross-continuity variants link by `gmeow:counterpartOf`.

### gmeow:sourceFor · gmeow:NarrativeFrameLink

`gmeow:sourceFor` (⊑ `gmeow:contributesToFrame`) relates a creative work to the frame it
sources, witnesses, or revises — the out-of-universe → in-universe edge. Frame-to-frame
relations (canon, alternate continuity, expanded universe, fanon, crossover, adaptation —
the open `gmeow:NarrativeFrameRelation` vocabulary) ride the flat shortcuts
`gmeow:hasNarrativeFrameRelation` / `gmeow:relatesToFrame`; promote to the reified
`gmeow:NarrativeFrameLink` (source × target × relation) when provenance, confidence, or scope
must be a node.

### gmeow:Myth

A socially-sustained narrative whose currency is independent of its truth-value — a founding
myth, an urban legend, a debunked claim that persists (the design, Deception design context).
GMEOW asserts no truth verdict (Principle 1); in-myth claims are scoped `gmeow:accordingTo`
the `gmeow:mythFrame`. Tellings attach via `gmeow:hasMythTelling`, spread along
`gmeow:propagatesFrom` (⊑ `prov:wasDerivedFrom`, reusing the lineage spine — Principle 4),
and the propagating agent bears `gmeow:roleDupe` or `gmeow:roleDeceiver` (the deception
bridge).

## Narrative time (narrative-position axis, design context)

**The text is not the story: the chapter sequence is the syuzhet projection
of frame-relative story content.** Two kinds of `NarrativeTimeFrame` (each a
`ReferenceFrame` under `frameRealmNarrative`) coordinatize narrative
position:

- **discourse time** (`axisDiscourseTime`, syuzhet) — the order of telling;
  owned by the telling work via `discourseTimeOf` (a re-segmented edition is
  a different frame);
- **story time** (`axisStoryTime`, fabula) — the order of happening
  in-universe; owned by a `NarrativeReferenceFrame` via `storyTimeOf`
  (a continuity can reorder it — a retcon is a frame-scoped remapping,
  preserved by suppression).

### gmeow:NarrativePosition

`NarrativePosition` is the narrative analogue of `SpatialCoordinates`:
frame (mandatory — no bare chapter indices), ordinal, label, or both.
`atNarrativePosition` is the single domain-free anchor that the depiction
seam (narration seam), arc samples (arc samples), and motif occurrences (motifs) all reuse —
deliberately non-functional, because one diegetic event holding positions in
*both* frames whose orders disagree **is** the flashback, queryable instead
of contradictory (Principle 9).

In-universe Allen relations are claims `accordingTo` the narrative frame via
the statement layer, never global facts. Position comparison, ordering, and
discourse↔story reconciliation are solver-layer (Principle 12). A reified
discourse↔story mapping construct is deliberately deferred to coordinate
with the music extension's `TimeMapping` (music design) — one frame-mapping idiom in
the repo, not two.

## The narration seam (narration seam, design context)

NOnt's reference function between text and story — neither mereology nor
participation: "chapter 31 *narrates* event E; character C is *narrated in*
segment S."

### gmeow:narrates · gmeow:narratedIn

- **Flat by default**: `narrates` (segment → diegetic content) and
  `narratedIn` (content → segment; Wikidata **P1441**'s edge — SSSOM row
  deferred to the alignment window), both `⊑ narrationLink` (domain- and
  range-free ancestor for media-specific seams: panel, shot, verse).
  **No `owl:inverseOf`** between the orientations — EL-clean; query both
  (the `connectsTo` convention).

### Retrieval chunks that also narrate

`narratedIn` continues to range over `ContentSegment`. A retrieval `Chunk` is not
automatically a structural content segment, so neither a universal subclass nor
a wider range is warranted. If a retrieved node is also a paragraph, scene or
other structural part of the source, type the same node as both `Chunk` and
`ContentSegment`, using `chunkOf`/spans for retrieval and `segmentOf`/`segmentType`
for document structure. This deliberate dual typing keeps one identity and
both roles. An arbitrary retrieval window is only a Chunk until that structural
role is actually asserted. See the [worked dual-role segment](../../core/observations/examples/refinement-readings.ttl).

### gmeow:NarrationUsage · gmeow:NarrationMode

- **Reify with a reason**: `NarrationUsage` (the NameUsage/DepictionUsage
  idiom) = segment × subject × mode(s), SHACL-required mode — the modeless
  case is the flat shortcut. It pairs with the flat `narrates` via
  `gmeow:pairsWith`. `NarrationMode` is open: direct, mentioned,
  flashback (the narrative-position axis two-axes disagreement as a mode), dream, hypothetical,
  unreliable (the boundary — narrator-level held ≠ projected;
  documented bridge, no axiom coupling).
- **Naming note**: `gmeow:depicts` belongs to the image spine
  (MediaObject → Entity, `⊑ isAbout`, documents module) and `isAbout`'s
  Entity range cannot carry diegetic *events* (occurrents) — hence the
  separate, range-free narration family.
- **The efficiency doctrine, codified**: the foundation corpus carries
  38,413 character-segment links + 23,962 narrated events + 12,354
  appearances. Budget arithmetic: flat ≈ 1 quad/link; reified ≈ 6–8
  statements/link; full reification ≈ 1.5–2M statements for one corpus.
  Flat is the default; **silent full reification is a defect, not
  thoroughness**. The consumer child (narrative consumer child) gates the flat/reified split
  against a declared budget.

Diegetic events stay ordinary `gmeow:Event`s claims-scoped `accordingTo`
their frame; the events module's Participation machinery is reused untouched
for who-did-what *in* the story.

## The narrative interior (arc samples / scoped roles / motifs, design context)

### gmeow:ArcSample

- **Arc samples (arc samples)** — the music `PitchTrajectory` move: an arc is
  sampled control points in a frame, not prose. `ArcSample ⊑ Observation`
  reads {subject × `NarrativePosition` (narrative-position axis) × state-by-IRI × vantage};
  the state is a *soft* cross-slice reference (affect `EmotionType` when
  loaded, EmotionML category otherwise — IRI, never dependency, P16).
  The existing `CharacterArc` is the integrating whole
  (`arcSample ⊑ hasPart`, purely additive); two analyzers disagreeing at
  one position are two coexisting cells, surfaced — never resolved — by
  `narrative-arc-trajectory.rq` (the syuzhet-CSV primitive).

### gmeow:RoleInNarrative · gmeow:hasNarrativeRole

- **Roles (scoped roles)** — a narratological role is a *function relative to a
  scope*, never a property of the character: `RoleInNarrative` =
  bearer × `NarrativeScope` (named umbrella grafted over CreativeWork /
  ContentSegment / NarrativeReferenceFrame, extension-side — the
  `Rule ⊑ Norm` direction) × open `NarrativeRole` value. The flat shortcut
  `hasNarrativeRole` pairs with the relator via `gmeow:pairsWith`.
  Protagonist of the trilogy, of one book, of one chapter are different
  claims; ensemble works carry coexisting protagonist relators and **no
  `primaryProtagonist` exists or ever will** (P9). Character-as-exemplar
  composes from the rubrics facility's `exemplarSubject` with zero new
  machinery.

### gmeow:Motif · gmeow:motifOccursIn

- **Motifs (motifs)** — thematic tags with *identity*: `Motif ⊑ SocialObject`
  with aliases (names module), an open `MotifKind`, and occurrences riding
  the narration seam (`motifOccursIn ⊑ narratedIn`, pairing with
  `NarrationUsage` via `gmeow:pairsWith` and inheriting the narration seam
  flat-by-default efficiency discipline). Cross-work identity goes through
  `counterpartOf` or a registry anchor (Thompson Motif Index / ATU /
  DBTropes — alignment window), never string equality. **Tag vs Motif
  boundary**: tags are uncurated labels (tags module); promote to Motif
  when recurrence and identity emerge — the promotion is an Activity with
  provenance, and the corpus heuristic (concepts → always; thematic_tags →
  on recurrence + curator confirmation) gates in the narrative consumer child.

## Solver layer & deferred alignment

Position comparison, discourse↔story reconciliation, arc-trajectory CSVs, and motif
recurrence analysis are all solver-layer (Principle 12); the slice carries the frames,
positions, and seams those queries read. Alignment is by reference — Wikidata P1441 for the
narration seam, the Thompson Motif Index / ATU / DBTropes for motifs — with SSSOM rows and
the reified discourse↔story `TimeMapping` (shared with the music extension music design) both
deferred to the alignment window so the repo grows one frame-mapping idiom, not two. No axiom
here references a deception-slice or norms-slice IRI: the unreliable-narration boundary and
the `NarrativeScope` graft are documented bridges and extension-side grafts (P16), never
core edits.

## Beat functions and memorial/homecoming arcs

`BeatFunction` is an open vocabulary for the structural work a beat does.
`beatFunction` links a `NarrativePosition` to one or more attributed values:

| Value | Structural reading |
| --- | --- |
| `beatFunctionIncitingIncident` | Disturbs the established situation and initiates the focal development. |
| `beatFunctionThreshold` | Crosses into a changed situation or commitment. |
| `beatFunctionMidpoint` | Reorients the development at an interpreted structural middle. |
| `beatFunctionCrisis` | Presents an acute impasse or pressured choice. |
| `beatFunctionClimax` | Culminates in a decisive confrontation, action or turn. |
| `beatFunctionResolution` | Settles consequences or releases the focal tension. |
| `beatFunctionOpenThread` | Carries an unresolved possibility or question onward. |

The generic set does not impose three-act structure, another tradition, a fixed
order or a length fraction. A midpoint is not computed by dividing an ordinal,
and a climax is not inferred from affect intensity. One position can serve
several functions, such as threshold plus inciting incident or resolution plus
open thread. Keep `positionFrame` and the locator; preserve disputed readings
with statement-level attribution rather than choosing a preferred function.

`arcTypeMemorial` describes a trajectory shaped by remembrance, including a
creature named in memory of another. It does not infer a death or naming event.
`arcTypeHomecoming` covers return or the establishment of belonging, including a
house becoming a home; it does not assert travel, ownership or residence.
`arcType` keeps its existing functional contract: distinct attributed arc
interpretations use distinct `CharacterArc` records with subject and frame.
See [the worked beats and arcs](examples/beat-functions.ttl).

For props inside the story world, use the generic qualitative placement
predicates `besideOf`, `onTopOf` and `insideOf` with an explicit narrative frame
and vantage. The [places pattern](../../core/places/docs.md#qualitative-placement-in-a-frame)
keeps those claims separate from world-independent facts and image regions.
Olfactory story texture can use the existing subjective `SensoryPerception`
pattern and `sensoryModalityOlfactory`; the sensory vocabulary also supplies
`observablePropertyOdor` for instrument readings. See the
[odor examples](../sensory/examples/odor-observations.ttl).

## Dependencies

Depends on `kernel`, `documents` (CreativeWork, ContentSegment, the image spine), `places`
(ReferenceFrame and `gmeow:frameRealmNarrative`), `provenance` (the propagation spine),
`observations` (ArcSample ⊑ Observation), and `events` (diegetic events and Participation).
Consumed by the deception slice's worked examples and by the lbox foundation-docs importer
(design context: 16,445 arc samples, 779 role links, 16,002 tags / 669 concepts) ahead of the narrative consumer child
consumer child.

## Literal language and direction

The natural-language fields `gmeow:positionLabel` admit
plain text with unspecified language, language-tagged text, and RDF 1.2 directional
text. Supplied public or private-use language tags remain part of the literal.
The canonical value restrictions derive the validation view; they do not add a
closed-world requirement for an explicitly asserted subject type. A machine key,
source expression, or byte payload uses its own declared property and datatype.
See [literal policy](../../../docs/LITERALS.md) for the shared contract.

## Impasse, progress and a shared thread

`arcTypeBreakthrough` is a teller’s reading from an impasse to a next step that
let the subject continue. It differs from recovery after loss. Its ordering
claims what came next, never what caused the impasse to end.

`sampleProgress` records a `ProgressState` beside `sampleState`. The latter
remains the existing functional affect reading, so a sample can carry both an
emotion and `progressStateImpasse` or `progressStateMoving`. Progress is a
vantage-attributed interpretation, not a diagnosis or the subject’s assertion.
The vocabulary is open; movement does not imply success or completion.

`impasseSignal` distinguishes a reading from the subject’s own words
(`impasseSignalOwnWords`) from one based on a parked inquiry
(`impasseSignalInquirySuspended`). Preserve the original records through the
existing provenance relations and retain the reviewed reading method when the
signal comes from a rule applied to wording. The category itself supplies no
rule, source text or proof; inquiry suspension does not entail an impasse.
The sample’s `vantage` identifies the teller, while the arc’s `accordingTo`
retains its interpretive scope (Principles 3 and 9).

`arcThread` names the exact resource that the teller asserts both ends share.
`arcThreadKind` classifies it with the open `ThreadKind` vocabulary: a working
loop, question, intention or record, including the first record itself. The
resource need not be a document; an intention is an intentional mode. The link
records the teller’s common-thread assertion without independently proving
endpoint links or inferring a causal connection.

Explicit `threadKindNone` says the teller found no shared thread and excludes
any supplied `arcThread`. Each of the four initial positive kinds requires an
explicit thread resource. Those two canonical integrity laws prevent internally
contradictory thread reports without closing the vocabulary. An absent
`arcThreadKind` remains unknown and creates no default. These additions do not
make progress, signal or thread fields mandatory for existing arcs or samples.

The ordinary [breakthrough scene](examples/breakthrough.ttl) keeps affect and
progress together, preserves the teller and source evidence, and distinguishes
a shared question from a separate arc explicitly told without a shared thread.

## Fictional characters and source narrative wording

`Character` is a fictional Entity whose identity belongs to one
`NarrativeReferenceFrame`, supplied through the existing `hasReferenceFrame`.
It is disjoint from `Person`. A fictional portrayal of a historical person
and that person remain distinct; an explicitly supported `counterpartOf`
link records correspondence without identity merging. Variants in different
continuities likewise use separate Character nodes.

Existing `arcSubject` and `narrativeRoleBearer` already accept Entity, so
they accept Character without wider ranges. An arc’s frame must match its
Character subject’s identity frame. A role explicitly scoped to a narrative
frame must match that frame too; roles scoped to works or segments retain
their existing meanings.

Four source axes must stay distinct:

| Source meaning | Representation |
|---|---|
| A named location in the narrative, such as a chapter title | `positionLabel` on a complete, frame-bound NarrativePosition |
| A structural function, such as crisis or threshold | `beatFunction` with an open BeatFunction value |
| An attributed reading of pursuit movement or impasse | `sampleProgress` on an actual ArcSample with its required position, state and vantage |
| The source’s original arc-phase wording, such as “rising” | `arcPositionLabel` on the actual Character or source record, with its narrative `hasReferenceFrame` |

No phase taxonomy or other axis follows from “rising” alone.
`arcPositionLabel` has no domain that would turn a source record into a
Character, CreativeWork or ArcSample. It requires an explicit narrative frame
on the bearer, and preserves the exact label rather than filling missing
sample data.

`motivation` and `relationshipText` retain a source’s prose on Character.
All three text properties are nonfunctional and accept plain,
language-tagged and directional language-tagged literals. Keep each supplied
language and direction. Rival readings can coexist; none implies an actual
Desire, a resolved contact relationship or another character’s endorsement.

Attach the original source and interpreting viewpoint to the exact RDF 1.2
statement. Keep recorder attribution separate from viewpoint. Do not assert
that the fictional character was derived from a document merely to record the
document as evidence for one descriptive sentence.

[The ordinary source scene](examples/characters-and-source-wording.ttl)
shows separate continuities, a historical counterpart, attributed prose and
a source record carrying “rising” without an invented ArcSample.
[The design](design/FICTIONAL-CHARACTERS.md) explains the frame and language
boundaries. These are source semantics and intrinsic consistency laws; they
do not claim that contextual reasoning or interpretation has been executed.

## Qualified breakthrough readings

`arcTypeInsight`, `arcTypeDecision` and `arcTypeSettledQuestion` name more
specific breakthroughs supported only by the exact next-step record. Open,
wording-only or multiply classified steps retain `arcTypeBreakthrough`.
`threadKindConversation` retains a bound conversation root and every message of
every structurally continuing conversation, without requiring internal reply
links or inferring cause.

`resolutionSignal` on an `ArcSample` distinguishes an attributed act from
the subject's own words read under a reviewed cue rule. Configure numeric degrees
on per-rule-version `SignalConfidenceSetting` records; annotate actual signal
statements with existing `confidence`, teller attribution, the selected setting,
source evidence and a reason. Shared signal vocabulary individuals acquire no
global numeric value.

`arcBetweenCount` and `arcUndatedCount` retain counts over an explicit finite
thread cohort, exact endpoint records and a selected source clock. Keep undated
records, original time precision and unavailable counts visible rather than
inventing dates or zeros. These declarations do not execute aggregate counting.

[The source contract](design/QUALIFIED-BREAKTHROUGHS.md) explains all boundaries
and the [ordinary qualified scene](examples/qualified-breakthrough.ttl), including
its exact retained-source and reviewed-confidence payloads.

## Build a story from retained records

`acceptedAsStory` links an information-object candidate to the story a person
accepted it as. The acceptance is the person's claim: retain its exact RDF 1.2
statement reifier with `wasAttributedTo` naming that `Person`. Independent
acceptances can name different stories without choosing a global winner.
A quoted acceptance alone does not assert its content. Canonical attribution
laws follow the completed statement-metadata components of the exact statement;
they check binding evidence, not the person's identity or authenticity.

A person can instead give a `storyFocus` without any candidate. Keep the wording
verbatim, including supplied language and direction. Focus does not imply
acceptance. Story candidates and stories reuse existing information-object and
work types; these properties introduce no universal Story class.

For a curated story collection, actual selected record resources ride `hasPart`.
Do not use organization membership's `hasMember`. A refiner's `readingScope`
and its `scopeRecord` entries retain the records offered to the refiner, which
need not equal the selected members. Keep those two enumerations distinct.

`storyGathered` names the activity reported to have finished gathering, written
after every selected member. `storyStructured` names the completed structuring
activity, written after the supplied events, seams and citations. The canonical
laws require explicit `logic:Closed` occurrent boundaries and require a gathered
marker on the same story before its structured marker. The declarations do not
certify transaction order or global member completeness. Missing markers imply
neither success nor failure, and an intermediate gathering stage is not an
`artifactComplete` verdict for the entire final story.

Retain the actual structure: `hasSegment`/`segmentOf`, frame-bound
`NarrativePosition` records and `beatFunction` values, flat `narrates` seams or
`NarrationUsage` when mode and role players matter, and actual `cites` links or
citation acts. A marker never manufactures these facts. Completion is not a
subproperty of `wasGeneratedBy`: when a stage really creates an immutable story
version, record that version's generation independently.

`recollectionCue` preserves every supplied short recall phrase on an event.
Several phrases and short names coexist without a preferred language or name.
They do not assert that a refinement matcher ran or that recall succeeded.
The [ordinary story-building scene](examples/story-building.ttl) includes actual
members, structural positions, citations, attributed acceptance and separate
completion claims. [The design](design/STORY-BUILDING.md) explains the boundaries.
