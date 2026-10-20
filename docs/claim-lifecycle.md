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
| `gmeow:correctsClaim` | correction → erroneous predecessor | The predecessor's purported earlier validity is rejected. |
| `gmeow:retractsClaim` | withdrawal activity → withdrawn claim | The standpoint withdraws its endorsement; withdrawal alone does not establish falsity. |

The first two relations specialize `gmeow:supersedes`. They accept claim-bearing
entities, including name usages; they do not require turning every name usage into
a `StandpointClaim`. None selects a global winner or changes another standpoint.
Annotate each relation with `gmeow:accordingTo` and `gmeow:validFrom` for its
standpoint and effective time. Keep the proposition's fact-time interval separate.
For a withdrawal activity, `gmeow:wasAssociatedWith` names the performing agent.

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
