Both applicable incorporated-PR performance findings now have local runtime fixes:

- List-cell admission discovers the union reachable from all first-cell heads once, then uses the existing PurRDF SCC primitive and shared immutable reachable-cell sets. It avoids repeated per-head walks and excludes incoming/unreachable rest-only components from association analysis. Original facts remain intact; the existing inclusive association bound and all-or-nothing publication are preserved. No new traversal cutoff was added.
- Standalone chase and joint execution precompute whether the selected rules/stratum can mint, including property witness frontiers. A quiet Datalog pass enters a generative pass only with that work present. Real invention, blocked reads, truncation, retained-proof draining, world scope and proof/budget accounting retain their existing paths.

Final-source validation:

- 135/135 affected synthetic runtime controls pass, nextest `c129000f-b2b2-46f9-9680-d1bbfb3bbebb`.
- 45/45 source, minimum-cardinality and per-world certificate consumers pass, nextest `dff7ae54-1c8b-4b71-bcf5-5fc081315a3c`, including rejection of copied/wrong demonstrator certificates.
- `make fmt`, `make lint`, strict `make clippy-prebuild`, `make helpers-hygiene` and `git diff --check` pass. Clippy's ambiguous inline shared-Arc initialization was fixed explicitly, without suppression. No helper ceiling, dependency, compiler profile or selected world changed.

The shared branching-cycle test observed 0.462s before and 0.027s after. The exhaustive small-graph test was 0.037s before and 0.067s after; the final selection adds one case. These are single focused observations, not a controlled benchmark or an aggregate/full-pipeline speedup. No test reads or constructs the repository corpus.

Production qualification remains open. The preceding authenticated bootstrap completed native reasoning in 28m55.830s, then failed at `stage-docs-render` because the retained result was unsupported (16,575 constructs). That failure and the refreshed upstream ownership/release evidence are recorded in #1803. The docs verdict guard remains intact. The optimized producer predates these runtime repairs and needs current-source qualification after the production capability refusal is addressed; no third full gate was started with this known failure.

These repairs remain uncommitted and unpushed in the combined candidate. The original review threads remain open pending publication/current-head review and actual production acceptance. No issue, superseded PR or merge is declared complete. The entire logic issue/PR goal remains in scope.

Principles 4, 7, 17 and 18.
