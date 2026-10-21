# Current qualification

Plan: plan.md. Changed scope: direct contextual derived-attribution indexing and its synthetic real-entry regressions, two Rust files. No generator, manifest, profile, requirement or guard changes.

- make fmt: passed before compilation; normal hook will verify final formatting.
- git diff --check: passed.
- make clippy-prebuild: passed, warnings denied, after the test lookup was corrected. See clippy.log.
- First contextual test compilation: failed on a test-only Arc method-resolution mismatch, now fixed. See contextual-tests.log; it is not a pass.
- make nextest-synthetic NEXTEST_FILTER='test(modal::contextual::)': second run compiled and ran 44 cases: 42 passed and two fixture scenarios failed because the unrelated rival lost its required status. Fixture replacement is now limited to the selected support claim; the settled run in contextual-tests-qualified.log passed all 44 cases. No corpus fixtures were loaded or produced. Final make clippy-prebuild passed with warnings denied; see clippy-final.log.
- Normal signed commit hook passed (including corpus-purity and formatting checks). Push succeeded. PR 1890 is open. The fetched base has no delta from the branch starting point and merge-tree returned a clean candidate; integration-tree.txt records that candidate. The actual tested source therefore covers the integration behavior. Fresh review capture has no submitted findings; the bot is still processing. Producer, prebuild, helpers, lint and automated review are pending; CLA passed. The user explicitly waived CI as a merge prerequisite. No independent approval is claimed.
- Full producer, corpus construction, synchronization, installed consumer and full gate: not run under standing authority. Whole-production acceptance remains unverified and parent stays open.
- Independent audit unavailable; completion and review records are local assessments. CI does not delay merge under the explicit user instruction and is never called passed without observed completion.
