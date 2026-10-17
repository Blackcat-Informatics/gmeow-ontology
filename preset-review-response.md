Addressed the remaining probability-model paragraph in 3aba8f7a8. It now requires the query-selected model, explains that a model merely present elsewhere in the graph cannot authorize the query, and states the public result as `unknown` with no marginals.

The native result also really does carry `evaluation = unsupported` and `information = not-evaluated`: `prob_result(ProbStatus::Unknown)` in `crates/logic/src/probabilistic.rs` sets both fields, and `prob_unknown_is_unsupported_not_evaluated` asserts them. That control and the actual no-model query controls are included in the 287 passing tests. Those typed fields remain in the paragraph with the public/typed distinction made explicit; removing them would misdescribe the native output.

This last change is prose only. Normal signed commit hooks passed and the commit is pushed. The reviewed implementation at 8ba5c655d is unchanged. Full hosted production/consumer qualification remains pending; the full canon issues are not closed.
