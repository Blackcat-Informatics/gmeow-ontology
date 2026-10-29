The modal RDF receipt now writes and reads its complete evaluation through native PurRDF JSON. All four nested wire types preserve the original NUL-prefixed bytes, field order, operator tags and evidence identities. Import still validates the conclusion and binds the evidence to the actual axiom.

The named-record reader requires explicit false values and empty frontiers, and rejects duplicate, missing, positional or undeclared members. Native parse and record failures retain the existing Reason diagnostic category and their typed cause. Shared Serde derives remain for other transports under parent #1776. Principles 4 and 7.

Validation: the original Serde capture passed before replacement; three complete original wires and actual evidence/receipt IDs are frozen and independently checked. The existing authored RDF round-trip control now replays them and exercises selected-format, required-member, semantic and importer-binding refusals. Independent trust/design and combined source review passed, including correction of the binding refusal to reach the actual importer. Warnings-denied compilation passed for all logic, pipeline, validation and developer CLI targets in 1 minute 46 seconds. The existing modal receipt control passed: one case, 2,187 tests and three binaries excluded; build 5 minutes 58 seconds, execution summary 0.155 seconds. Helpers and the static producer-recipe check passed. Normal signed commit and push hooks passed.

CI is explicitly waived. No local optimized producer, canonical corpus construction, sync or full gate ran. Authenticated corpus, installed, browser, release and remaining parent acceptance stay open. No performance claim.

Closes #2046.
