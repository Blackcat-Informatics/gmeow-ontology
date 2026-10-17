# Native class label-map investigation

Read-only sites found: current_conflict, add's opposite-label lookup, saturation's
disjunction check and the first pick_branch pass. The outer saturation iteration
needs a stable snapshot because it mutates labels. Nominal branch inspection calls
mutable find, which may create member roots; preserve those insertions and exact
choice order, cloning only the nominal entries if that path is changed. Do not
replace nominal find with a read-only approximation or cache labels across merges.
The complete pre-optimization native execution selection has passed all eight cases.

This unit advances the full-canon/performance work without claiming native semantic
completion. Current producer evidence retains unsupported constructors and recorded
4,707,532ms for reasoning; the exact internal cost owner is not established by that
enclosing duration. No obstruction may be removed just to produce a positive verdict.

Source inspection found State::add cloning the entire concept/support map merely
to look up an opposite label. current_conflict also clones that map before a read.
These are avoidable ownership operations whose cost grows with memberships on one
individual. Keep branch snapshots where mutation actually requires them; preserve
union support, local conflict subjects, source leaves, budgets and deterministic
choice ordering. Do not cache mutable labels across nominal merges.

First add a Criterion class_memberships group to the existing reason bench and use
the existing Make benchmark lane on 250/500/1000 synthetic memberships. Assert the
selected family executes completely and retains every asserted membership proof;
an unengaged or unsupported family must fail the benchmark. Capture baseline before
editing production code, then compare the same harness/profile/lock after edits.
No corpus is produced and no machine/resource or memory probe is authorized.

Replace only proven redundant whole-map copies with borrowed lookup/iteration and
owned copies of the actual matched conflict support. Test wide labels, opposed
membership, nominal equality and disconnected subjects through production native
class execution; run focused nextest and strict Clippy. Measure the changed native
boundary, not a standalone imitation. Do not extrapolate an end-to-end speedup or
claim unsupported class constructors now complete.

Work remains on the existing 1770 branch/PR 1832. The broader source, prepared-base,
installed proof and full-gate criteria in plan.md remain unchanged. Independent
review APIs are unavailable; hosted CodeRabbit review remains required after push.
