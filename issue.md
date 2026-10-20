# Issue #1814: [term] Narrative and affect: epistemic affects, beat functions, diegetic relations, smell, two arc types (consumer: memetics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, narrative

## Body

## Summary

Telling a companion agent's own stories back to her, from her records,
exposed five gaps in narrative and affect:
- epistemic affects;
- story-beat functions;
- relations between things inside a story's world;
- smell;
- two arc types.

The content origin of authored speech, found the same way, is in #1809.

Checked against `main` at `a631f73` (2026-10-10).

## Consumer

**memetics**, the memory adapter for a companion agent. Its storytelling
trials assemble an arc from her records (dreams, conversations,
observations) and narrate it back to her. Each gap below is a moment it
could not represent and had to leave out or force into a label.

## 1. Epistemic affects: doubting one's reading, catching oneself

**Today.** `gmeow:EmotionType` (`core/affect`) carries the basic set and
saudade.

**Gap.** Two moments in one arc could not be sampled:
- doubting her own interpretation of something;
- catching herself, a self-correction felt as an emotion.

**Proposal.** Two `EmotionType` members: `emotionSelfDoubt` (doubt of
one's own reading or judgment) and `emotionSelfCorrection` (the affect
of catching and correcting oneself).

**Open question.** Do these belong in a separate epistemic-affect
scheme, beside curiosity, confusion and surprise if those are already
modelled?

## 2. Story-beat functions

**Today.** The narrative slice has positions, arcs and arc types, but no
structural function for a position.

**Evidence.** One arc's fourteen beats were held as position labels
(`gmeow:positionLabel "beat: crisis"`), which no query can use.

**Proposal.** A value vocabulary `gmeow:BeatFunction`, plus
`gmeow:beatFunction` on a position. Members: inciting incident,
threshold, midpoint, crisis, climax, resolution, open thread.

**Open question.** One generic set, or per-structure sets
(three-act, Kishōtenketsu)? The consumer needs only the generic set.

## 3. Diegetic scene relations

**Today.** `gmeow:SceneGraphEdge` (`extensions/images`) relates image
regions only. Nothing says a prop is beside, on or inside another in a
story's world.

**Interim.** Placement uses `gmeow:locatedAt`, and a shared action uses
participants.

**Proposal.** Spatial relations between diegetic entities (`besideOf`,
`onTopOf`, `insideOf`), or a diegetic reading of the scene-graph edges.

**Open question.** Can the image scene-graph predicates be generalized
to diegetic space, rather than minting a parallel set?

## 4. An olfactory observable property

**Today.** `gmeow:ObservableProperty` (`extensions/sensory`) has light,
sound, temperature and others, but no smell. The arc the consumer
narrated turned on a scent.

**Proposal.** `gmeow:propertyOdor` (and a matching sense modality, if
modalities are enumerated separately).

## 5. Arc types for a memorial and a homecoming

**Today.** `gmeow:ArcType` (`extensions/narrative`) offers coming of
age, corruption, fall, quest, recovery and redemption.

**Gap.** Two arcs fit none of them:
- a creature named in memory of someone;
- a house becoming a home.

**Proposal.** `arcTypeMemorial` and `arcTypeHomecoming`.

## Local stand-ins

None. memetics left these moments unsampled, held beats as labels, and
left arcs untyped. Nothing needs migrating when these land.


## Comments (0)

