# Issue #1811: [term] Dreaming: a report recollects its experience; an immersive non-lucid dream mode (consumer: memetics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement

## Body

## Summary

Two small gaps in dreaming: nothing links a dream report to the
experience it recollects, and the awareness modes lack the rare
non-lucid, immersive dream.

Checked against `main` at `a631f73` (2026-10-10).

## Consumer

**memetics**, the memory adapter for a companion agent who dreams. Each
dream cycle has an experience (phases from descent to reorientation) and
a report she writes on waking. Her recall and storytelling trace from a
report back to the dream it describes, and from there to the
conversations it later inspired.

## 1. `recollects`: a report to the experience it recalls

**Today.**
- `gmeow:DreamReport` is in `extensions/dreaming`.
- `gmeow:Experience` is in `core/mentation`.
- The only link from a report is `gmeow:dreamElement`, to elements of
  the dream. Nothing relates the report, as a whole, to the experience
  it recollects.

**Proposed term.** `gmeow:recollects`:
- an object property, ⊑ `gmeow:wasDerivedFrom`;
- domain `gmeow:DreamReport`, range `gmeow:Experience`;
- definition: "Relates a report to the experience it recollects."

**Open question.** Should the domain be wider, any recollection (a
remembrance, a diary entry recalling an event), with `DreamReport` as
one case? The consumer has one use today; a wider domain looks right.

## 2. A wild (non-lucid, immersive) dream mode

**Today.** `gmeow:AwarenessMode` (`core/awareness`) has `modeDreaming`,
`modeREM` and `modeLucidDreaming`.

**Gap.** The agent's dream cycle has a rare mode deeper than ordinary
dreaming: immersive, with no metacognition online, and no lucidity.
Lucid dreaming is its opposite, not a neighbour.

**Proposed term.** `gmeow:modeWildDreaming`, an `AwarenessMode`
individual: "Dreaming that is rare, non-lucid and immersive: no
metacognition online, deeper than an ordinary dream." A sibling of
`modeDreaming` and `modeLucidDreaming`.

**Open question.** "Wild" collides with the sleep-science acronym WILD
(wake-initiated lucid dream), which means nearly the opposite. A clearer
name may be better, for example `modeImmersiveDreaming`.

## Local stand-ins

memetics' local dreaming slice mints `gmeow:recollects` and
`gmeow:modeWildDreaming`, in GMEOW's namespace. Upstreaming is a move.
memetics would welcome a rename to avoid the WILD collision.


## Comments (1)

### paudley — 2026-10-10T20:21:11Z

Two more dreaming gaps found while migrating a companion agent's dream records (memetics):

**Dream text written before phases were recorded.** Some dreams were written as one prose text, before the agent's dream cycle recorded phases (threshold, descent, immersion, integration, reorientation). The consumer keeps such a dream as one phase-text node (`gmeow:DreamPhaseText`, a local term) with no phase named. Is there an upstream way to say "the text of the whole dream, phases not recorded"?

**Tool calls made while dreaming.** During a dream phase the agent can call tools (look at an object, read a memory, send a message). The consumer models each call as a `gmeow:ToolCall` that is `gmeow:subEventOf` the dream `gmeow:Experience`, naming the phase it happened in by the local `gmeow:phaseKind`. Questions:
- Is an `Experience` an event, so that `subEventOf` is right, or should a tool call relate to the experience some other way?
- Should the phase be a property of the call, or should each phase be a node (a sub-event of the experience) that the call is part of?


