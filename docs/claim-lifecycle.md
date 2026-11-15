<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Claim lifecycle

Ordinary revision retains history and suppresses superseded or withdrawn records
from display with `gmeow:displayable false` (Constitution Principle 10). Suppression
does not establish whether the earlier claim was valid. Lifecycle relations carry
that distinction, indexed to the standpoint making it.

| Relation | Subject → object | Historical interpretation in the stated standpoint |
|---|---|---|
| `gmeow:succeedsClaim` | successor → predecessor | The earlier claim remains valid during its earlier fact-time interval. |
| `gmeow:correctsClaim` | correcting claim → corrected claim | The correcting standpoint states that the corrected claim is wrong, including its purported earlier validity. The corrected claim is retained unchanged. |
| `gmeow:retractsClaim` | withdrawal activity → withdrawn claim | The standpoint withdraws its endorsement; withdrawal alone does not establish falsity. |

Only `gmeow:succeedsClaim` specializes `gmeow:supersedes`. The relations accept
claim-bearing entities, including name usages; they do not require turning every
name usage into a `StandpointClaim`. None selects a global winner or changes
another standpoint. Annotate each relation with `gmeow:accordingTo` and
`gmeow:validFrom` for its standpoint and effective time. Keep the proposition's
fact-time interval separate. For a withdrawal activity, `gmeow:wasAssociatedWith`
names the performing agent.

## Correction is not suppression

Correction and suppression are two different acts. "It is blue" answered by "No, it
is red, you are wrong, I checked" does not erase the evidence of blue.

- A correction derives no `gmeow:supersedes` and no `gmeow:supersededBy`, and it is
  no reason to set `gmeow:displayable false` on the corrected claim. The corrected
  claim and its correction are both retrievable.
- No data sets `gmeow:displayable false` because a claim is disputed, corrected or
  evaluated. What a particular reader sees of a disputed claim is decided at
  projection. Principle 10 names the cases that do prompt suppression.
- A succession or a retraction is a standpoint changing its own position. Neither
  is a dispute, and neither is weakened by the rule above.

Choose the pattern by what happened:

| What happened | Record |
|---|---|
| Two parties simply disagree; nothing is corrected. | A `gmeow:ClaimEvaluation` that `gmeow:evaluates` the claim, with the evaluator's standpoint in `gmeow:accordingTo` and its verdict in `gmeow:expresses`. Both claims stand. |
| A standpoint states, on a codified basis such as a checked record or a rule that decides between the two claims, that another claim is wrong. | `gmeow:correctsClaim` from the correcting claim to the corrected claim, annotated with the correcting standpoint and the effective time. |
| The claimed state changed. | `gmeow:succeedsClaim`. |
| A standpoint withdraws its own claim. | A `gmeow:Activity` that `gmeow:retractsClaim` the claim. |

The standpoint slice carries one worked example of the first two patterns in
`slices/core/standpoint/examples/correction-and-disagreement.ttl`.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:janetUsage gmeow:succeedsClaim ex:bobUsage
    {| gmeow:accordingTo ex:person ;
       gmeow:validFrom "2025-01-01T00:00:00Z"^^xsd:dateTime |} .
ex:correctUsage gmeow:correctsClaim ex:mistakenUsage
    {| gmeow:accordingTo ex:person ;
       gmeow:validFrom "2025-02-01T00:00:00Z"^^xsd:dateTime |} .
ex:withdrawal a gmeow:Activity ; gmeow:wasAssociatedWith ex:person ;
    gmeow:retractsClaim ex:claim
    {| gmeow:accordingTo ex:person ;
       gmeow:validFrom "2025-03-01T00:00:00Z"^^xsd:dateTime |} .
```

`gmeow:assertoricRetract` remains a description of a withdrawal **speech act**.
Assigning it to a claim never withdraws that claim or changes lifecycle state.
The activity and `retractsClaim` relation are the single canonical encoding of an
actual withdrawal. Consumers apply their governed state and display policies;
these relations do not perform destructive graph updates.

An absent `gmeow:claimModalForce` or `gmeow:standpointModalForce` is **unspecified**,
neither necessary nor actual. State necessity explicitly. Explicit legacy
`unequivocal` and `refuted` retain their declared necessary-force decompositions;
absence of a legacy modality supplies no such decomposition.

## Governed erasure

An in-flight `gmeow:ForgetRequest` has one or more values for each of:

- `gmeow:requestingAgent`: the agent making the request, not necessarily the data
  subject or authorizing authority;
- `gmeow:requestedErasureTarget`: each exact named object requested for removal;
- `gmeow:requestUnderRight`: the named right or policy being invoked.

The last two are IRI references, not literal selectors or transitive queries.
The requester may be a named or anonymous agent. A literal is not an agent;
the consuming policy may separately require an identified requester.
The three paths are required by class-scoped canonical closed-world constraints.
A request does not grant permission. Pending and denied requests do not delete
anything. Authorization and exact-object deletion are the consuming store's duties.

On authorized fulfillment, explicitly select the request record itself together
with its authorized targets. Remove exactly those objects. Do not cascade into
unselected referring or referenced objects. Unselected references can dangle and
answer “gone”; do not preserve a tombstone to implement that answer. Retain no
deletion receipt, removed-identifier list or derived purge history in the store.
This exception applies equally to names, gender facets and software identities;
ordinary renaming, correction, retraction or expiry continues to retain history.

GMEOW does not adopt `ExpunctionReceipt` or `ClaimRewrite` as deletion-receipt
vocabulary. Receipts of repairs made before migration stay as source evidence
**outside** the migrated governed stores. Do not launder them into generic
`Activity` records inside those stores. The learned-change `LifecycleReceipt`
vocabulary concerns promotion, rejection, evaluation and rollback, not deletion.
