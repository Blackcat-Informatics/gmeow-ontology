# Diagnostic source-seal repair

The current source-only repository guard found two defects while qualifying the
already-integrated diagnostic migration: route_facet_value returned Result<(),
String>, and logic.dl-service had no canonical failure class. The original run
executed 29 cases: 28 passed and live_repo_static_passes failed on those two
findings. See diag-current-qualification.log.

Contract facet routing now appends directly to the source compiler's existing
Diagnostic observations. The obsolete string-return path and caller conversion
are removed. Conflicting singleton values retain INVALID_CONTRACT_FACET, Error
severity, the exact contract owner and both conflicting values; source admission
continues to reject them. Repeated values and set-valued facets retain their
previous semantics. This is the compiler observation channel, not a new runtime
error abstraction or a suppressed refusal.

DlService now names DirectSemanticsServiceRefusal as its canonical failure class.
Its existing code, grade, typed no-model/malformed/execution disposition and
original upstream error remain intact. The owning logic slice declares that
operational class and its enforcing boundary, with all seven English prose
fields and paired French/Chinese translations. No authored shape or external
semantic authority was introduced. Principles 4, 6, 7, 17 and 18 apply.

Commit 12380e2df is pushed in PR 1835 and the source tree is clean. All 45 focused
cases passed in 17.824s after 7m34s compilation, including the live repository
source seal. Strict producer-independent workspace/all-target Clippy passed with
warnings denied in 4m11s. Every normal signed commit hook passed. Logs:
diag-repair-qualification.log, diag-repair-clippy.log, diag-repair-commit.log and
diag-repair-push.log. The new head has live producer/lint/Rust-prebuild CI.

Hosted ontology/translation and full consumer qualification remain pending.
No local producer, corpus production, full gate or machine probe ran.
The prior helper repair's hosted optimized producer, lint and Rust prebuild all
passed on c3123db1a. Its downstream generation failed with the known recovery-leg
error; generation-current.log retains that evidence and PR 1830 owns the repair.

No full gate or translation/ontology corpus validation is inferred from the
source seal. This repairs two current diagnostic source-seal defects without
claiming full issue 1762 qualification or full native semantic completeness.
