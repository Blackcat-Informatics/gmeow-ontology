# Incorporated PR review accounting

This is a read-only consolidation audit. No PR or thread is closed by this
capture, and ancestry alone does not qualify an issue's acceptance.

## Coverage

The exact six PR heads still match pr-consolidation.md. Stagectl metadata and
issue comments are retained as raw/prN-consolidation-*. Complete native review
submissions and inline threads are in raw/incorporated-pr-review-surface.json;
all review, thread and nested-comment connections report hasNextPage=false.
The PR bodies are in raw/incorporated-pr-bodies.json because stagectl metadata
does not include them.

| PR | Issue comments | Review submissions | Inline threads | Coverage limit |
| --- | --- | --- | --- | --- |
| 1793 | 2 | 1 | 1 unresolved | One actionable performance finding |
| 1798 | 1 | 1 | 1 unresolved | One actionable performance finding |
| 1799 | 2 | 0 | 0 | Auto review skipped for a non-default base; includes an integration note |
| 1800 | 1 | 0 | 0 | Auto review skipped for a non-default base |
| 1801 | 1 | 0 | 0 | Bot summary records a review with no actionable comments, not a separate review submission |
| 1804 | 1 | 0 | 0 | Summary covers the earlier message-literal change; the later datatype-range change hit a review limit |

The first 1801 comment fetch failed with TLS handshake timeout. Its retry returned
zero comments even though the native connection counted one. The complete
paginated native issue-comment capture in
raw/pr1801-consolidation-comments-native.json recovered and preserved that
comment. An empty wrapper response was not accepted as complete evidence.

## Findings and current dispositions

Both runtime findings are repaired locally. Final source passes all 135 affected
synthetic controls, nextest c129000f-b2b2-46f9-9680-d1bbfb3bbebb,
raw/review-optimizations-final-controls.log. Strict Clippy, lint and helper checks
pass; 45 affected certificate consumers also pass. Publication, current-head
review and actual production admission remain open;
neither public thread is resolved by this local result.

### List traversal work across heads

Thread PRRT_kwDOSwdn1s6ovDFf, discussion_r4176227281, PR 1793.
At initial capture, input/admission.rs::bind_list_cells restarted its traversal
for every first-cell head, limiting emitted rows rather than all traversal work.
Shared branching/cyclic rest-only subgraphs can therefore be revisited many
times with few output rows. The performance concern is valid and is not
discharged by the later typed EvidenceGap work.

Repair the repeated-work path or account its total analysis work coherently.
Keep exact head/cell associations, shared-tail and cycle behavior, the existing
output bound, and atomic publication: an incomplete refinement inserts no
partial LIST_CELL rows. Every original fact and required forward operation must
remain available; a refinement cannot silently shrink the selected source or
replace a complete admitted execution with a success claim. No host probes,
resource wrappers, weakened output profile or baseline raise are authorized.
Exercise an adversarial shared subgraph and healthy scoped-list neighbors, and
recheck actual production admission after the coherent repair.

The baseline now has three passing synthetic controls, nextest
8c56b8e0-aa05-4b73-9192-d8685fbc654a, recorded in validation.md. Reuse the existing
purrdf_core::graph::scc_component_index primitive over dense term indices;
its public contract guarantees reverse topological component IDs. Compute
reachable first-cells once per component, sharing immutable successor sets
across linear paths, and union them only at actual branches or member additions.
Check the same complete head/cell row count against MAX_BINDINGS before any
LIST_CELL insertion. No extra work cutoff or new dependency was added. Production
sources were held fixed until bootstrap handle 39900 terminated with the actual
unsupported-verdict refusal. The patch was then applied and validated. Its final
reachable-union refinement excludes incoming rest-only components from association
analysis without removing any original source fact or adding a traversal bound.

The concrete three-file candidate is retained in
raw/review-optimizations-candidate.patch records the prepared patch before
application, formatting and the reachable-union refinement. The baseline passes 134/134
in raw/review-optimizations-broad-baseline.log. This covers source exactness,
inverse families, real witnesses, guard withholding, retained proofs and budget
cuts as well as the original three list controls. Final candidate execution passes
135/135, including the fourth incoming-component control.

### Unnecessary generative pass without minting work

Thread PRRT_kwDOSwdn1s6ovvM2, discussion_r4176505075, PR 1798.
At initial capture, chase_world_into and the joint stratum driver unconditionally entered
a generative pass after a quiet Datalog pass. When no prepared producer can mint
and no selected property has a witness frontier, that repeats completed work.

Gate the transition on actual scheduled minting capability, preserving current
truncation/retained-drain handling, completion order and every required witness
round. Cover standalone and joint Datalog-only work, genuine invention, inverse
back-edge satisfaction, empty-nominal guards, unknown value interpretation and
retained proofs. Preserve semantic output and source evidence; investigate any
changed operational accounting explicitly. This is required before final
qualification, not a waived follow-up.

The local implementation precomputes prepared-rule minting for standalone chase
and selected-stratum minting across joint worlds, including witness-frontier
properties. It gates only the empty Datalog-to-generative transition. Truncation,
retained draining, blocked-read completion and actual witness rounds retain their
existing paths; the 135-case result covers their regressions. Full producer and
published-head qualification remain required.

## Integration note adjudication

PR 1799's note originally instructed RJA to read the bridge-closed tuple program.
That instruction is superseded by the later soundness repair. Current
certify_ladder separates the position graph from RjaProgram's exact source
premises; pair encoding/reverse bridges can splice unrelated statements and are
explicitly excluded from restricted blocking. Restoring them would regress that
under-approximation contract. Preserve the exactness, guard, pair and data-term
identity regressions when qualifying the combined source.

## Remaining review obligations

The bot's private-function documentation percentage messages are not proof of a
public API defect or a substitute for repository CI. Assess actual documentation
contracts on the final combined source; do not pad private helpers to satisfy a
percentage or declare required hosted checks passed from a bot summary.

Skipped, rate-limited and summary-only review coverage remains explicit. A fresh
combined-head review is still required where available after publication. The
two applicable performance threads remain open until their source fixes and
qualifying evidence are published. The distinct 1794 certificate change and its
scope-documentation review are separately recorded in pr-consolidation.md.
