# Issue #1816: Identity, vantage and policy: address identity, policy findings, vantage bound to a key, reader-relative disclosure (consumers: katamari, memetics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: question

## Body

## Summary

Four questions about who a claim is from, who may read it, and what
identifies a node:
1. address identity;
2. policy findings;
3. a vantage bound to a consumer key, with signer kept apart from
   observer;
4. reader-relative disclosure.

Checked against `main` at `a631f73` (2026-10-10).

## Consumers

- **katamari**: the governed RDF 1.2 store. Writes are consumer-signed.
  An admission policy judges each write, and a reader sees a view
  shaped by what it may know.
- **memetics**: the memory adapter for a companion agent.
  - One rule is absolute: only she speaks as herself. Code must never
    write a claim in her first person.
  - Its admission never refuses a write. It records findings about the
    writing code, which go to the operator.

## 1. What identifies an address node (katamari)

**Today.** `gmeow:addressValue` (`core/contacts`) is functional but not
a key, so two address nodes with equal normalized values never collapse.

**Proposal.** Declare it a key (`owl:hasKey`) on `gmeow:EmailAddress`.
Or document that identity derives from the normalized value. katamari's
starting default is the latter.

## 2. Policy findings (both)

**Today.** Nothing specific.

**Use case.** A store's admission policy judges a write and finds
something:
- a serialized record inside a literal;
- an untagged string written by code;
- code naming the companion as the speaker.

The finding is itself a claim, from the policy's vantage, about another
claim.
- memetics records such findings as conditions on the writing code.
- katamari records policy findings at the store.

**Proposed terms.**
- `gmeow:PolicyFinding`, a sub-kind of `gmeow:Observation`: its vantage
  the policy (named and versioned), its observed feature the judged
  claim, and its result a finding kind.
- `gmeow:violatesPolicy` / `gmeow:findingKind` with a value vocabulary
  the policy defines.

**Open question.** Is this a special case of `logic:` constraint
violation reports (SHACL validation results as RDF)? If so, the consumers
would map onto it and only need the vantage binding.

## 3. Vantage bound to a consumer key; signer apart from observer (both)

**Today.** `gmeow:vantage` (`core/observations`) names who observed. It
has no binding to the key that signed the write, and no distinction
between who signed and who observed.

**Use case.**
- The store must check that a claim asserting a vantage was signed by a
  key bound to that vantage.
- That check is how "only she speaks as herself" is enforced at the
  store, not just by convention.
- A migration or a refiner may sign a claim whose observer is someone
  else (re-expressing what she wrote, with a derivation link), so the
  signer and the observer must be separable.

**Proposed terms.**
- `gmeow:boundToKey`: a vantage or agent to a key identifier (per
  #1807 item 2's identifier typing).
- `gmeow:signedBy`: on the claim, distinct from `gmeow:vantage`.

## 4. Reader-relative disclosure (katamari)

**Today.** `hasSensitivity`, `hasDisclosurePolicy`, `coarsenTo` and
`displayable` (kernel slice) exist. Two forms of withholding are not
told apart:
- **visible redaction**: "there is something here you may not see";
- **non-acknowledgement**: nothing indicates anything was withheld.

**Ask.** Is that distinction wanted in the disclosure vocabulary? A
reader-relative view must know which form a policy calls for. The ADR
on katamari's side is not yet written; this is raised early so GMEOW's
model can inform it.


## Comments (0)

