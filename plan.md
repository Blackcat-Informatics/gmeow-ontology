# Admit complete compiler publications within the shared cache budget

Under pipeline issue #1757, hosted run 39384821769 failed in both cold
generations and fixture-prefix with the same 796,711,985-byte compile-logic
publication rejected by the 536,870,912-byte entry ceiling. Producer and Rust
prebuild jobs passed; downstream corpus acceptance did not run.

The source bases that independent entry ceiling on an obsolete 369,597,157-byte
publication. Remove the historical-size ratchet. Define the existing 8 GiB total
store budget once in action-cache and let one complete action use it. Pipeline
writers and read-only consumers select the same shared limits. Preserve full
typed products, authentication, corruption failures and explicit store quotas.
Read and digest verification use the authenticated receipt's declared length as
their bound; a grown file must fail before reading rather than use the broader
store allowance. A small synthetic corruption control covers both paths.
No source selection or corpus output may be trimmed to fit a cache threshold.

Validate with producer-independent strict Clippy and synthetic action-store and
pipeline-cache tests, including corruption, quota eviction, read-only consumption,
publication races and typed hydration. Normal hooks remain mandatory. No local
corpus producer or full gate will run. Merge through ghprsq under the explicit CI
waiver, then remove this completed worktree/branch. The wider issue remains open:
this repair does not establish whole-corpus acceptance or finish performance work.

Principles 4, 7 and 18 govern one complete authenticated product and shared native
storage contracts. No host inspection or process limits are involved.

Execution: all 64 selected action-store/pipeline-cache tests passed, including
the new authenticated-size growth refusal. The test build took 6m50s; execution
took less than one second. Normal signed commit/push hooks passed. The final
strict producer-independent Clippy rerun passed on the final source. Full corpus
production and full-gate acceptance remain unverified, so #1757 stays open.
There is no concurrent base delta at the fetched integration base.

PR #1866 review read found no inline findings or submitted reviews. Automatic
CodeRabbit review was still processing. No independent execution audit is
claimed. Hosted CI is waived by the maintainer's explicit merge instruction.
