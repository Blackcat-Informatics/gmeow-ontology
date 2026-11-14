# Issue #2683: vocabulary: record an approval of a body of work, separate from its holders

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, foundational, cross-cutting, module-enhancement

## Body

Request: a general vocabulary for recording an approval of a body of work,
separate from who holds the claims in it.

Need: a reader of a body of work often has to know who approved it, when,
and for what scope, and that is a different fact from who made or holds
its claims. Today a record can say who holds a claim but has no typed way
to say that a named person approved the work on a date, and for which
scope, without making the approver a holder.

Proposed terms (one node per approval; a later approval adds a node, and
its identifier and label never change):
- an approval class;
- approvedBy (the person or agent who approved);
- approvedOn (the date, typed as a date);
- approvalScope (optional: the part of the work approved);
- reportedBy (optional: who recorded the approval).

Note: an approval is never a holder. Holding claims and approving a body of
work are separate facts and both should be expressible.


## Comments (0)

