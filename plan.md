# Extract the query identity test helper

Hosted producer recipe admission rejects the inline cfg(test) implementation in
dev_shapes/query_identity.rs. That helper was introduced by the retirement-oracle
repair; the production source census correctly refuses it. This is a concrete
build blocker under #1761's production/test separation contract, not a reason to
relax the census or corpus-producer seal.

Move the unchanged test wrapper into the existing extracted dev_shapes.tests.rs
module. Remove its test-only import from the production parent. Give the existing
query-key implementation parent-module visibility so the extracted test can call
the actual production function. Keep all assertions and production behavior.

Verify live source admission with make producer-recipe: its recipe arm returns
after input resolution and fingerprinting and never builds/runs the optimized
producer or constructs a corpus. Run existing focused identity tests through
nextest-synthetic and normal signed hooks. No new mirror tests, resource probes,
profile changes, suppression list or alternate merge path are needed.

Principles 4, 7 and 18. Integrate through ghprsq under the explicit CI waiver,
then clean up the completed branch/worktree. The parent issue's crate-check and
full hosted acceptance requirements remain outstanding until actually qualified.

The shared reasoning audit also found that the chase doctrine claimed a strict
certificate chain even though meet_certified implements an explicit partial order.
Correct that prose under #1796's doctrine requirement and run its existing lattice
regression. This does not change the algorithms or claim complete termination-
issue acceptance. Preserve JA/SWA and RJA/SWA/MSA incomparability and the warning
that meeting certificates does not certify the union of programs.

Execution: make producer-recipe passed on the live source after extraction,
without entering its build/run arms. The five existing identity controls passed.
Seven chase/census controls passed, including both named live GTS producer-map
and authorship-bypass checks, the partial-order meet, a valid inverse certificate,
an uncertified negative, the music-shaped Thing filler and unbudgeted execution.
Corpus-purity and normal signed hooks passed. The fetched base has no concurrent
source delta. Parent full crate-check/gate and hosted corpus acceptance are not
claimed by these focused results.

Final integration review: the merge-tree prediction is clean against the fetched
main. The four-file delta contains only test-helper relocation, parent visibility
and doctrine correction. No PR comments, inline findings or reviews were present
at final inspection. CI and automatic review are pending; integration proceeds
under the user's explicit instruction to merge sequentially while ignoring CI.
No independent auditor was available or claimed.
