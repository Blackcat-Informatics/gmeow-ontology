# External corpus quality profiles

Implement all of issue 1752 as an installed, checkout-free consumer operation. Principles 4, 6, 7, 15 and 17-19 govern the work. Explicit profile selection precedes capability admission; no missing input may silently select a weaker profile, become a passing score, or disappear from the report.

Existing engine: the consumer already reads its rubric and GMN dictionary from the embedded bundle; DocMaturity already builds a foreign-slice model. Profiles and explicit per-axis applicability/results are absent. The shared floor-free measurement standard and lattice remain the scoring authority. Reuse them; do not fork the scorer or weaken repository floors.

## Task 1: Canonical profile admission

Author profiles and applicability semantics in the quality rubric. Read them through strict typed admission: unique profile key, known target tier and axes, explicit status and non-empty reasons for exclusions/unsupported axes. Select axes before loading their required capabilities. Missing selected dictionary/catalog inputs fail. Preserve measurable documentation, module/manifest, competency and box-role checks.

## Task 2: Consumer scoring and complete reports

Wire the public quality command and bundle/file APIs through selected profiles. Keep default full-profile behavior explicit. Report passed/failed/unsupported/not-applicable for every rubric axis, reasons, measured/excluded/unsupported counts and the transparent aggregate denominator. Bind exact corpus, selected-profile and rubric identities. Use only external files, declared imports and the bundle; no checkout dependency or guessed inputs.

## Task 3: Meaningful qualification

Use independent small GMEOW fixtures for profile contrasts, exclusion reasons, missing selected capabilities, invalid/ambiguous profile data, unchanged scores on shared axes, stable identities/roll-ups, declared imports and honest unsupported results. Add installed consumer execution from outside the checkout to hosted qualification. No test may produce corpus; no local corpus producer, full gate, host probing, resource caps or hook bypass. Run focused Make checks and strict producer-independent lint locally; hosted producer/consumer checks qualify the installed bundle.

## Task 4: Review, merge and cleanup

Commit through normal hooks, push and open one issue-linked PR only after the implementation is complete. Request CodeRabbit review; no independent delegation API is available and no independent verdict is fabricated. Resolve findings and applicable gates, merge sequentially with ghprsq, then remove the completed checkout/branch and disposable drafts. Keep only this active plan and issue setup; do not archive copied evidence.

## Remaining work

PR https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1852 remains open. The helper and three valid review repairs are pushed and pass all 23 focused profile/lint controls, strict Clippy, full helper hygiene and normal hooks. CodeRabbit accepted those fixes and withdrew the platform exemption; all four original review threads are resolved. The branch also carries the identical shared cache correction qualified by the 24 action-cache controls on the projection-purity branch.

Obtain fresh hosted producer/installed-consumer/full qualification. The original hosted run failed and is not acceptance. Resolve further findings, assess integration with projection purity, merge sequentially through ghprsq and remove the completed checkout/branch. No local corpus producer or full gate is authorized. Keep no copied check logs or evidence archive.

The latest hosted attempt is attempt 3 and is terminal failure. Generation and fixture jobs ended with runner shutdown, cancellation or lost communication; no semantic acceptance or source root cause is established. Do not repeat unchanged production while the shared-producer qualification is live. Full hosted and installed-consumer acceptance remain unmet.

Drain acceptance recheck: the installed-consumer control copies the built gmeow artifact outside the checkout, selects external-sacred explicitly and compares shared measurements and corpus/rubric identities against full-profile scoring on identical foreign files. Profile admission precedes dictionary admission; import assembly requires declared owners and fails missing imports. The rubric identity binds the complete bundle bytes, including bundled import content. This read-only inspection found no new defect and does not replace execution of the hosted consumer control.

## Authorized integration

User authorization: "merge them one by one, ignoring CI." Hosted CI is waived for integration and remains unverified. All four original review threads are resolved. Integration follows the canonical purity and packet authoring changes; the same cache repair agrees.
