fix(build): preserve producer identity and fail docs prerequisites closed

Normalize Cargo compiler evidence owners through exact metadata identities so compiler-message IDs bind to selected source owners, generated inputs and external roots. Keep unknown/contradictory identity failures and all compiler-read, executable and recipe authentication mandatory. Chain docs prerequisites so a failure stops immediately instead of repeating an optimized build. Require the inert actual-recipe failure/order regression in both lint entry points. Principles 4, 7 and 18 apply.

Closes #1825. Closes #1826.

Three focused producer identity tests passed. Strict producer-independent Clippy passed on the identical Rust repair. The actual docs recipe regression passed and its old-source negative control failed as expected. Normal hooks passed for all three source commits. Hosted optimized producer, lint and rust-prebuild passed for ee21d2c32. CodeRabbit reported no actionable comments; documentation coverage passed at 90.91%.

Full pipeline qualification FAILED: generation recovery evidence lacks an executable resolved leg pair (#1828); fixture-prefix rejects goal-directed stable-prefix/recompute-aggregate admission (#1829). Dependent validation skipped and aggregate quality failed. No full local gate or independent completion audit is claimed.

The user expressly directs: "Please ghprsq this branch now, we'll create issues for anything that comes up in CI, but we need to get this code pushed and merged." Integration proceeds under that explicit exception to waiting for CI/audit completion. The failures remain owned by the linked issues; they are not passing acceptance.

No conflicts. The fetched base is an ancestor of the source and the clean integration prediction is 7ae630f24a243f0c53b7da1ca142d36bc3715112. No additional base delta invalidates source checks. Review dispositions are in review-debt.md. Authoritative plan: .stage/reconcile-producer-input-selection-with/plan.md; selected evidence: .stage/reconcile-producer-input-selection-with. No governing source/authentication/profile contract was weakened.

Defect-Class: compiler-input-identity-mismatch
