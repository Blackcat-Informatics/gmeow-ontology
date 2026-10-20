# Issue #1818: Documentation: question wording, identifier and extent outside creative works, memory as claims vs remembering (consumer: memetics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, question

## Body

## Summary

Three documentation questions. Each is cheap to answer and changes how a
consumer writes data.

Checked against `main` at `a631f73` (2026-10-10).

## Consumer

**memetics**, the memory adapter for a companion agent, modelling her
records (questions she holds, media she was sent, memories) in GMEOW.

## 1. Where a question's wording goes

**Today.** `gmeow:Question` (`core/inquiry`) has no text property, so
its wording rides `rdfs:label`.

**Ask.** Is that the intent? A question she holds ("what do I want this
house to feel like?") is content, not a label. A `gmeow:questionText`,
language-tagged per #1807 item 1, would keep labels for naming.

## 2. Identifier, extent and body value outside creative works

**Today.**
- `gmeow:identifier` and `gmeow:extent` (`core/documents`) have domain
  `gmeow:CreativeWork`.
- A chat attachment is a `gmeow:MediaObject`.
- A chat message part might be a `gmeow:BodyValue`
  (`extensions/email`).

**Ask.**
- Does `identifier` apply to a media object (a manifestation)?
- Is `extent` meant for a byte length?
- Is `BodyValue` the intended home for a chat part's content?

With the domains as written, using them on a media object infers it to
be a creative work.

## 3. Memory as claims, memory as remembering

**Today.**
- `core/ai` describes memory as claims, with `gmeow:MemoryItem`.
- `core/cognition` has `gmeow:remembers` as objectual memory.
- The bridge between them is prose only.

**Ask.** Document how they relate. Is a `MemoryItem` the content of a
`remembers` relation, or are they separate models? The consumer stores
her memories as claims and also needs "she remembers X" to be queryable.


## Comments (0)

