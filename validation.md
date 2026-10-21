# Validation state

The implementation lane ran no checks. Root qualification executed all31 package tests successfully; strict producer-independent prebuild Clippy passed. No corpus production, host measurement or full gate was run.

## Source observations

- No serde_json references remain in crates/perf-evidence, including its manifest. Make/Cargo resolved Cargo.lock, removing only this package's serde_json dependency.
- All free-form reads use pinned record::read_document; publication uses sorted_last_wins + write_pretty, followed by newline, synchronization and atomic persist. Publication errors retain the destination context and prior destination.
- Object/Value/ToJson own all report construction. Native u128 fields use the owner's direct primitive conversion (the pinned ToJson trait does not implement u128).
- Pair report paths explicitly refuse non-UTF-8 values. Existing display-only path fields retain their existing behavior.
- New byte-fed sample/metadata decoding and inventory payload helpers are wired through actual production callers. Tests invoke these pure helpers, resource delta, existing cache/proof checks, or temporary-file operations. Source inspection found no test calling run_cli, host_identity, child_usage, run_text, dependency_resolution_digest or a subprocess. The getrusage text in one expected JSON string is a report label, not a syscall.
- Intentional differences are documented: embedded number lexemes survive output; paired identity equality uses exact decimal values. Strict integer selectors and finite/nonnegative observation admission remain enforced.

## Exact source inventory

Modified: crates/perf-evidence/Cargo.toml; src/lib.rs, src/sample.rs, src/junit.rs, src/bin/perf-accept.rs; docs/rust-test-performance.md.

New external test modules: src/lib.tests.rs, src/sample.tests.rs, src/junit.tests.rs, src/bin/perf-accept.tests.rs. These retain all previous inline tests and add literal publication/resource/pair/inventory byte expectations, nested sorted last-wins behavior, Unicode/control escapes/null/u64 boundary/native timestamp conversion, number exponent/signed-zero/lexeme behavior, malformed input, pure metadata digest controls, strict sample selectors, finite overflow refusal, escaped identity pointers, numeric equality versus changed string/array refusal, and non-UTF-8 path controls.

## Root qualification

make fmt passed. make nextest-synthetic NEXTEST_FILTER='package(gmeow-perf-evidence)' passed:31 executed,31 passed,0 skipped; native-qualification-3.log retains the complete command/output. The first two compile attempts failed on redundant test imports already inherited through super; both removed without assertion changes. Corpus producer scanner and purity checks passed as prerequisites. Strict make clippy-prebuild passed with warnings denied; its consumer exclusions do not establish consumer-inclusive full Clippy. Normal signed commit and push hooks passed. Complete current hosted feedback is captured; no formal, inline, conversation or thread entries exist at this snapshot. Hosted checks remain pending. No actual host measurement is necessary for this JSON migration. Full #1776 remains open for other crates and workspace-wide acceptance. Local corpus/full gates remain explicitly unrun under the campaign exception.
