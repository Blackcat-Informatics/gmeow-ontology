# Native term panels: production phase correction

Hosted run 38981058280, generation (b), job 117247724307 failed on
bc230b3f6: missing stage-snapshot product for term entailments. Exact failure:
native-panels-generation-b-failure.log. Builds and lint passed; this is not a
successful generation. Other live jobs were left running. The b reasoning phase
recorded 2,790,123ms; this is not a controlled speed comparison.

The previous lifetime assessment was wrong. The production caller is
docs_render::render_docs_graph, before SnapshotStage consumes its output.
stage-gts-compose and stage-snapshot are both real, distinct stages. Renaming
snapshot, adding a dependency on a downstream stage, or retaining another
carrier would be incorrect. The sink label in the failure came from the reused
error constructor, not the actual execution phase.

Term panels now read only stage-reason, which DocsRenderStage already declares
as a carrier input and already uses for its reasoning verdict. Native owner,
graph pin, payload identity and result admission remain mandatory. Missing-input
and missing-firing diagnostics now identify stage-docs-render. The documentation
implementation identity advances to v13. No new carrier edge, parser fallback,
alias, second reasoning pass or weaker check is introduced.

The old fixture fabricated a downstream snapshot and released reason, so its
nineteen passing checks did not establish production wiring. That fixture is
removed. Its replacement uses the live reason product. A new regression checks
actual DocsRenderStage, ReasonStage and SnapshotStage dependency declarations,
renders panels with the declared reason input alone, and refuses a released
carrier. Existing native modal/world/quoted/directional/refusal cases remain.
All inputs are tiny explicit synthetic products, never corpus production.

Focused qualification: native-panels-phase-tests.log records all 15 selected
term-panel and shared native-reader cases passing in 0.046s, after a 5m58 build.
This includes the real-stage dependency/lifetime regression. Strict
producer-independent workspace/all-target Clippy passed in 3m14:
native-panels-phase-clippy.log. Full hosted generation is not established by
these synthetic checks.

Published as ab1a26067. Normal signed commit hooks and push passed; exact logs
are native-panels-phase-commit.log and native-panels-phase-push.log. No hook was
bypassed. Current-head hosted generation and independent review remain pending.

The obsolete explanation parser, RawDerivation and three parser-only formatters
remain deleted. Full hosted generation, semantic completion, prepared canon and
reasoning-performance acceptance remain unproven.
