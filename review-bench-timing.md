# Independent Criterion timing-family source review

Reviewed the five files in `validation-bench-timing.md`, their old diff and all
current callers of comparison, baseline emission and leaderboard rendering.
This reviewer did not implement this family or execute any checks. The perf
merger was outside this review.

No concrete defect found in this bounded source review.

- The public comparison API now returns a fallible result, and its actual
  `bench-compare` caller handles it as a graded failure. Missing/unreadable
  baseline and missing live directory no longer manufacture an all-new or
  all-missing successful report. Valid comparisons—including regressions—still
  return success; timing thresholds and the existing positive-denominator
  calculation are unchanged.
- Baseline records use native required-field admission, explicitly unsigned
  `Value::as_u64`, and exact integer output. This avoids the previously observed
  generic unsigned signed-zero pitfall. Native map/record ownership preserves
  outer last-wins names, known-field duplicate refusal and ignored unknown
  fields; incidental positional records are explicitly refused and documented.
  The production leaderboard uses this same decoder, with no Serde bridge.
- Live free-form Criterion input uses recursive last-wins parsing. Native exact
  decimal sign admission rejects negative underflow before binary64 conversion;
  exact zero remains admissible. Finite, nonnegative in-range rounding keeps
  the previous ties-away-from-zero behavior. The 2^64 boundary now refuses
  saturation. Metric diagnostics retain file, benchmark and metric context.
- Directory/path selection retains the prior `new/estimates.json` and minimum
  depth policy. Selected non-UTF-8 identities now fail rather than being merged
  through lossy spelling. Valid empty inventories remain distinct from missing
  required inputs, and an empty measurement set cannot emit a new baseline.
- Zero baseline median produces an unavailable percentage, separate from new,
  missing or measured zero. It contributes neither a false zero-percent claim
  nor a regression tally. Literal output, threshold, empty/missing and invalid
  numeric controls meaningfully exercise the touched behavior.
- The 13 new controls use plain records, authored JSON and temporary trees.
  Their calls collect authored measurement files or render an authored baseline;
  none executes Criterion, engines, corpus discovery/materialization, producer
  stages or authenticated fixture synthesis. The five existing timing controls
  remain, adjusted for the intended hard-failure contract. CostArtifact,
  deterministic engine-cost and soak ownership remain untouched.

Compilation, focused tests, CLI execution, actual benchmark measurements,
producer/authenticated consumers and broader gates were not run by this
reviewer. Source expectations do not establish those outcomes. The recorded
user execution/CI exception remains separate: this review grants no additional
waiver and does not establish broader acceptance or a performance improvement.

## Compiler repair disposition

Re-reviewed the formatted final timing diff and the precise baseline encoder
repair. Passing `Object` directly to `Object::insert` is correct: the pinned
owner accepts `value: impl Into<Value>`. The removed trailing `.into()` left an
unconstrained intermediate target and caused E0283; removing it lets `insert`
select the existing `Object` conversion. It changes no members, ordering,
numbers or wire bytes. No new substantive finding in the formatted diff.

Root reported the combined focused qualification as 27 executed and passed:
nine merger, 13 new timing and five existing timing controls. This reviewer did
not execute those tests; strict prebuild was still running when this disposition
was recorded. Actual measurements and broader acceptance limits above remain.
