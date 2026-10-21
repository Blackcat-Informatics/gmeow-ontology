# Repair disclosure attribution and local required fields

Address both post-merge findings on PR 1891, comments 4332170702 and 4332170707. The original signature example omits its explicit vantage, and the decision class has no local reader/outcome cardinality authority. Both are confirmed against current source. Principles 4, 7, 9, 12 and 17.

Add the original claim vantage. Give DisclosureDecision class-local allValuesFrom and maxQualifiedCardinality/onClass restrictions for reader and result, plus class-scoped ClosureEntry records for required paths. Do not use a bare qualified existential as closed-world presence, make observationResult globally functional, or constrain different decision nodes to share one reader/outcome.

The existing authenticated validation fixture producer already selects the observations module. Add a read-only consumer of its actual shape with nine independent boundary cases: valid, absent reader/outcome, duplicate reader/outcome, wrong reader/outcome type, ordinary multivalued observation, and separate complete decisions. Test code never compiles corpus or derives shapes. Compile the real consumer through strict producer-independent Clippy and run existing synthetic tests for closed universals, qualified counts and absence of global closure leakage. The new corpus consumer itself remains unrun locally without an admitted selector; no local optimized producer, sync or full gate is authorized.

Commit/push normally, publish one repair PR related to the closed original issue, review fresh findings, integrate with ghprsq under the CI waiver, reply to and resolve the original threads only after landing, and clean the completed checkout. Maintain the whole queue. No independent local dispatcher is available; source assessment is self-review.
