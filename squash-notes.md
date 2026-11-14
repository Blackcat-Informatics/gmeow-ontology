Provenance: record an approval of a body of work, separate from who holds its claims (Principles 9, 10, 17)

Adds gmeow:Approval, a relator between an approver and a body of work, with gmeow:approvalOf,
gmeow:approvedBy, gmeow:approvedOn (a calendar day), the optional gmeow:approvalScope and gmeow:reportedBy.
Approving a work is not holding its claims: approvedBy is a subproperty of no holder, attribution or
contributor relation. The approver, the work and the day are required paths, authored as logic: restrictions
with closed-world closure entries and qualified maximum cardinality one. A missing scope means the scope is
unstated, never the whole work. A worked example, the slice guide and the French and Chinese catalogs are
included.

Verified: independent review PASS at 74e334917; rapper, msgfmt, the issue-reference lint and the producer
recipe exit 0 on the merged tree. Hosted helpers, lint, producer-build and rust-prebuild green; the three
producer jobs and the aggregate are red as on main. No producer run was made.

Closes #2683.
