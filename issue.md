# Issue #1815: Claim lifecycle doctrine: erasure and forget requests, historical deletion receipts, succession vs correction, modal force default, retraction (consumers: memetics, katamari)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: question, normative

## Body

## Summary

Five questions about how a claim's life ends or changes. They need
GMEOW's judgment more than new terms:
1. erasure, against the "suppression, never erasure" doctrine;
2. a receipt for a deletion made before the store existed, where the
   two consumers currently disagree;
3. succession against correction;
4. the default of an absent modal force;
5. retraction encoded twice.

Checked against `main` at `a631f73` (2026-10-10).

## Consumers

- **memetics**: the memory adapter for a companion agent. Revision is
  suppression; removal is a governed purge, never a silent expiry. It is
  migrating an archive that includes historical repairs which removed
  and rewrote claims before any of this doctrine existed.
- **katamari**: the governed store. A delete removes exactly the object
  named, and references to it dangle (its ADR-0047 47.10, which retired
  an earlier purge/expunge split). No record of a deletion may survive
  the purge (its ADR-0014).

## 1. Erasure and the forget request (both)

**Today.**
- Every slice says "suppression, never erasure". The doctrine comes
  from #234's identity-over-history model: `displayable false` retains
  the bytes and never displays them.
- The nearest terms are `gmeow:hasDataSubject` and `gmeow:actionDelete`
  (`core/rights`).

**Gap.**
- Both consumers must be able to remove data for real. A person can ask
  to be forgotten, and a governed purge must actually delete.
- The vocabulary has no way to state a forget request, and the doctrine
  as written forbids what both stores must do.

**Proposal.**
- Doctrine: suppression remains the default for revision.
  - Erasure is a separate, governed act. It removes exactly the object
    named; references to it dangle, and a dangling reference answers
    "gone".
  - The doctrine text says so, rather than "never erasure".
- A term for the forget request: what it covers, who asked, and under
  which right.
  - In katamari, the request's own record is purged together with what
    it covers. The term is for the request in flight, not a lasting
    record.

**Open question.** How should the slices that state "never erasure"
(names, gender, software identity) word the exception without weakening
#234's model for ordinary revision?

## 2. A receipt for a deletion made before the store (the consumers disagree)

**memetics' position.**
- The archive it is migrating contains historical repairs. Before any
  governed store existed, a maintenance process removed some claims and
  rewrote others, and recorded a receipt: reason, count, which claims.
- That receipt is history of the archive, not a deletion inside the new
  store.
- memetics minted `gmeow:ExpunctionReceipt` (a `gmeow:Activity`), with
  its reason, a removed count and the identifiers removed, plus
  `gmeow:ClaimRewrite` for the rewrites.

**katamari's position.**
- A tombstone, "this was purged", is not a candidate. Its store forbids
  any record of a deletion surviving purge.
- Vocabulary for one would exist to be misused.
- A dangling reference is already its own observation.

**Ask.** Does GMEOW want a term for a historical deletion receipt at all,
given the tombstone risk? If yes, how should it be fenced so it cannot
be used for deletions inside a governed store? For example: it is
documented as describing an act outside the graph's own history, and
carries no identifier of what was removed. If no, memetics will keep its
historical receipts as source evidence outside the migrated stores.

## 3. Succession against correction (katamari)

**Today.** "Bob became Janet" (succession: both were true, at different
times) and "Bob was never Bob" (correction: the earlier claim was wrong)
both reach for `gmeow:displayable false` (`names`, `gender` slices).
They are not told apart.

**Why it matters.** A store answering "what was true then?" must keep
the succession's earlier truth, and must not keep the corrected claim's.

**Proposal.** Distinguish them, for example with a succession relation
between name usages (with a time) beside suppression for correction.
katamari models this as a `succeed` claim operation.

## 4. The default of an absent modal force (katamari)

**Today.** `gmeow:claimModalForce` (`core/standpoint`) has a `howToUse`
reading an absent force as `gmeow:modalForceNecessary`. That is odd for
a plain observation: "this message was sent" is not necessary.

**Proposal.** An absent modal force reads as unspecified, as GMEOW
already does for standpoints. katamari has adopted this reading.

## 5. Retraction encoded twice (katamari)

**Today.** `gmeow:AssertoricForce` (`core/standpoint`) includes
`gmeow:assertoricRetract`, while retraction is also a lifecycle act on
the claim.

**Proposal.** One canonical encoding. katamari encodes retraction only as
the act, and refuses the force value as non-canonical. Should
`assertoricRetract` be deprecated, or documented as the speech-act
reading only?


## Comments (0)

