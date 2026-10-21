# Issue #1816: Identity, vantage and policy: address identity, policy findings, vantage bound to a key, reader-relative disclosure (consumers: katamari, memetics)

State: CLOSED   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
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


## Comments (2)

### paudley — 2026-10-21T01:26:42Z

# Identity, policy and reader disclosure

Deliver all four questions in issue 1816 using existing owners and Principles 4, 9, 10, 12, 17 and 19. No external consumer repository changes.

1. Document store-relative address identity under an explicitly selected normalization policy, without equating channel holders or adding global equality axioms.
2. Add PolicyFinding as an Observation. Reuse vantage, observedFeature and observationResult for versioned policy, judged claim and policy-owned result kind. Explain native constraint evidence and recording-only admission without imposing rejection.
3. Reuse hasSignature, signedBy, signingKey and Certification. Add the missing boundToKey relation from an agent or standpoint to a CryptographicKey entity. Distinguish authorized binding, verified signature and observer attribution. Show refiner signing with derivation and retained observer.
4. Add a reader-specific DisclosureDecision, reader role and explicit release, visible-redaction and non-acknowledgement outcomes. Keep private decision records out of protected reader views. Absence supplies no default; reader decisions coexist.

Author logic-native terms with complete English coats and paired French/Chinese catalogs. Add examples and exact-row producer-owned competencies. No test constructs corpus. Inspect source/catalog pairs and run normal hooks and focused source checks. Local optimized production, synchronization and full gate remain prohibited; source hygiene does not establish hosted generation acceptance.

Publish one coherent PR, review feedback, assess integration, merge with ghprsq under the CI waiver, verify landing/archive and clean the owned checkout. The portfolio remains open. No independent dispatcher is available; label local review as self-review.


### paudley — 2026-10-21T01:38:47Z

Model policy findings, vantage key bindings and reader disclosure

Closes #1816.

Resolve the four GMEOW model questions through existing owners. Address identity is store-relative under an explicit normalization policy and does not equate channel holders. PolicyFinding specializes Observation and retains the policy vantage, exact judged RDF 1.2 claim and policy-owned outcome. boundToKey supplies the missing vantage/key binding while hasSignature, signedBy, signingKey and Certification remain the signature spine. DisclosureDecision retains its reader and explicit release, visible-redaction or non-acknowledgement result. Principles 4, 9, 10, 12, 17 and 19.

Eight owning-slice terms include full English annotation coats and paired French/Chinese catalogs. The guide covers recording-only admission, native constraint evidence, trusted key bindings, refiner derivation without observer impersonation, and protection of policy-side decision records across reader-visible surfaces. Correct the attestation guide to name its actual CryptographicKey range. Add private worked records, two exact-row competencies and two structural controls through the existing producer-owned slice-specification lane. Judged content remains quoted, not asserted.

Source checks passed for eight declarations, 56 exact English annotation fields, 112 complete catalog pairs, unique contexts, guide links and logic-native semantic authoring. Diff hygiene and every applicable normal signed commit hook passed, including spelling, Markdown, secrets and reference policy. No Rust code changed. No local optimized producer, corpus construction, synchronization or full gate ran. Corpus competencies and complete projection/translation qualification have not run locally. External consumer authorization and disclosure engines are outside this ontology delivery and no execution of them is claimed.

The user instructed: "merge them one by one, ignoring CI." Pending CI does not delay integration and is not called passed. Independent local review is unavailable; requirements and source review are self-review. Capture fresh PR feedback and check status before merge; processing notices are not approvals. The fetched base has no additional source delta and merge-tree produced a clean candidate with no conflicts. Source qualification applies to that integration tree.

The authoritative delivery plan is .stage/identity-vantage-and-policy-address/plan.md and the selected Stage directory is .stage/identity-vantage-and-policy-address. The portfolio index remains at the canonical main checkout under .stage/queues/all-issues/queue.md. No dependency, ratchet, detector, profile or required gate changed. Verify the actual landing, issue closure and selected archive, then remove only the completed owned branch/worktree and fast-forward main. Preserve the unfinished GMN sibling.

Defect-Class: none


