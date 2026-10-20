# Current qualification

Issue 1815; PR 1886. All five questions are implemented in the published branch.
Authoritative plan: plan.md. Requirement-by-requirement self-review: gap-analysis.md.
No independent review dispatcher is available; this is not an independent audit.

| Check | State | Evidence |
|---|---|---|
| Normal signed commit and hooks | PASS | commit.log; all applicable hooks, including corpus purity, formatting, prose/reference checks and secrets checks passed. |
| Push | PASS | push.log; exact branch published normally. |
| Formatting | PASS | fmt.log and final commit hook. |
| First manual lint | FAIL, superseded by settled commit hooks | lint.log observed files changing during concurrent formatting; no gate was bypassed. |
| First synthetic build | FAIL, source corrected | synthetic.log: two new assertions treated Result<bool> as bool. Both now unwrap explicitly; expected behavior unchanged. |
| First strict prebuild Clippy | FAIL, source corrected | clippy.log: the new synthetic validation called parse_shapes with a dataset instead of Turtle text and a base. Corrected to its actual two-argument API without changing expectations. |
| Corrected strict prebuild Clippy | PASS for pre-intersection source | clippy-corrected.log; not qualification of the later production compiler fix. |
| Corrected four-test synthetic selection | FAIL: three codec tests passed; request test exposed production duplicate node-kind facets | synthetic-corrected.log. Canonical resource-range and explicit IRI obligations emitted an invalid shape with two distinct sh:nodeKind values. |
| Node-kind production fix | Implemented, qualification running | Shared IR intersects node kinds in property, node and qualified shape conjunctions; empty intersections preserve optional absence. Compile-stage revision advanced. Original expectations retained. |
| Seven-test synthetic selection | PASS | synthetic-node-kinds.log; completed normally with exit 0. Seven tests passed, including original four tests, all 36 kind pairs/order controls, nested constraints and native projection acceptance/rejection. Optimized test profile, nextest ci profile. |
| Strict Clippy for production fix | PASS | clippy-node-kinds.log; completed normally with exit 0, warnings denied, dev optimized profile. Later catalog-only cleanup changes no Rust inputs. |
| Shape API correction hooks/push | PASS | commit-shape-api.log and push-shape-api.log; normal signed hooks passed and correction was pushed. |
| Node-kind compiler fix hooks/push | PASS | commit-node-kinds.log and push-node-kinds.log; normal signed hooks passed and production fix was pushed. |
| Catalog cleanup hooks/push | PASS | commit-catalogs.log and push-catalogs.log; five stale aliased entries removed per language, retaining current canonical-source matches. No Rust inputs changed. |
| Extended review cases | PASS | synthetic-review-cases.log: affected test passed, exit 0. clippy-review-cases.log: strict Clippy passed, exit 0. The only Rust delta adds typed-anonymous-requester success, literal-requester rejection and blank-right rejection. Prior six unaffected tests remain qualified. |
| Extended review cases hooks/push | PASS | commit-review-cases.log and push-review-cases.log; normal hooks passed and the final source was pushed. |
| Full corpus/producer gate | NOT RUN locally | Standing user restriction forbids local production/full check. Registered source DSL judgments require hosted qualification. |
| Hosted CI | Ignored for merge by explicit user instruction | pr-checks.txt; not represented as passed. |
| Integration | Clean candidate; no base delta | integration-tree.txt, fetch.log; main has no changes beyond the branch base. Refresh before merge. |
| Reviews | One actionable finding addressed and thread resolved | reviews.json, inline-comments.json, review-threads.json; review-reply.md. Status prose is not approval. No independent execution audit is claimed. |

Local selected qualification is complete. The initial compile errors and the duplicate-node-kind production defect are fixed; all original expectations remain enforced. The catalog-only cleanup changes no Rust inputs, so the completed strict Clippy and seven-test results cover the final Rust source. Generated corpus judgments and full producer/gate acceptance remain unrun locally, under the standing restriction and CI merge waiver. The complete issue's vocabulary/doctrine contract is implemented; this is not a claim that either consumer's store deletion endpoint was changed. Use only ghprsq for final integration, then read back its archive before cleaning this owned worktree and branch. Preserve the foreign GMN worktree.
