PASS at 48f9ac5729f3ab4dcb60d7e669f8021d8ca81c05. This covers the whole branch: the delta is checked here, and the rest relies on the reviews at aa4ea2381 (PASS) and 3d41394c4.

# Delta review: issue 1897, branch paudley/1897-ground-model-serving-tensors-and, 3d41394c4..48f9ac572

## Scope and head

- `git rev-parse HEAD` and `origin/paudley/1897-ground-model-serving-tensors-and` are both
  48f9ac5729f3ab4dcb60d7e669f8021d8ca81c05. The worktree is clean.
- `git log 3d41394c4..HEAD` lists one commit, 48f9ac572. `git diff --stat` shows one file,
  slices/extensions/model-serving/tests/example-conformance.ttl, with 3 insertions and 2 deletions.
- The delta matches its commit message:
  - The only non-comment line in the diff is the removed line 62
    (`or an additional incompatible owner, ...`), which had no `#`.
  - No non-comment line is added. The three added lines all start with `#`.
- How the delta bears on the rest of the branch:
  - The 3d41394c4 review blocked only for the parse error. It found everything else in its delta
    correct.
  - This delta changes comment text and removes the one malformed line. It cannot change any
    triple, query, module axiom or document.
  - So the 3d41394c4 review's other conclusions still hold, and so does the PASS at aa4ea2381 for
    the earlier branch.

## Findings

### REQUIRED

None. The earlier REQUIRED 1 (example-conformance.ttl was not valid Turtle) is resolved:

- `rapper -q -i turtle -c` on the file at this head exits 0.
- The cause of the CI failure on 3d41394c4 is now established from the CI log:
  - Run 43813627179 failed in `generation (a)`, `generation (b)` and `fixture-prefix`, each with
    `RDF parse error ... example-conformance.ttl: ... unexpected token Some(Word("or")) ... at <unknown>:62:1`.
  - That is exactly the line this commit fixes.

### OPTIONAL 1: the unchanged first sentence of the comment omits the "only" on the parameter space

- **Location:** example-conformance.ttl, lines 58 to 61.
- **What the comment says:** the constraint requires "every weight's parameter space to belong to
  that model or that graph".
- **Why that is incomplete:**
  - In tensor-weight-extra-owner-space.ttl, the space does belong to ex:model.
  - That fixture violates only clause 5: every owner must be m or g.
  - The constraint's skos:definition states this ("names only that model or graph"), and so does
    the cell rationale.
- **Effect:** the comment's next sentence correctly says an extra owner on the parameter space
  breaks the law. Its own summary of the requirements does not show why. The summary is not false,
  only incomplete.
- **Possible fix:** "...to belong to that model or that graph and to no other owner."
- This text predates the delta. The delta only rewrapped the line it shares. This does not block.

### Earlier optional findings

- **OPTIONAL 1 at 3d41394c4 ("each break one part" overstates):** resolved. The new sentence no
  longer counts clauses. The commit message says the two wrong-owner fixtures break two, which is
  true (see the clause table).
- **OPTIONAL 2 at 3d41394c4 (round-4 evidence incomplete):** superseded in effect.
  - Round-4 problem 1: r4-prose.log was empty. Round 5 has no prose log at all. I ran `prose-check`
    on the changed comment (lines 57 to 63 via `--stdin`), the head commit message (`--stdin`) and
    docs.md. Each exited 0.
  - Round-4 problem 2: the new fixtures were untracked during the lint run. Moot, because both
    fixtures are committed and parse at this head.
  - Round-5 evidence has its own limit:
    - r5-rapper.log names head 3d41394c4 with example-conformance.ttl modified, so it is a
      pre-commit run on a dirty tree.
    - It covers four files, each with EXIT=0.
    - My run below covers every slice Turtle test and example file at the committed head.
  - r5-commit.log is complete. It shows the hook results, commit 48f9ac572, EXIT=0, the push
    `3d41394c4..48f9ac572` and PUSH_EXIT=0.

## Evidence

### Parse check: `rapper -q -i turtle -c <file>` at 48f9ac572

All 21 Turtle files under the slice's tests/ and examples/ were checked. Every one exits 0:

- examples/mathematical-model.ttl
- examples/provenance-chain.ttl
- examples/serving.ttl
- examples/two-sessions.ttl
- tests/competency.ttl
- tests/conformance-fixtures/artifact-training-objective-loss.ttl
- tests/conformance-fixtures/metric-observation-dimensioned.ttl
- tests/conformance-fixtures/tensor-artifact-coherent.ttl
- tests/counter-examples/deployment-two-artifacts.ttl
- tests/counter-examples/metric-result-dimension-mismatch.ttl
- tests/counter-examples/metric-result-no-magnitude.ttl
- tests/counter-examples/scalar-metric-path.ttl
- tests/counter-examples/tensor-graph-extra-owner.ttl
- tests/counter-examples/tensor-graph-other-model.ttl
- tests/counter-examples/tensor-weight-extra-owner-space.ttl
- tests/counter-examples/tensor-weight-other-model-space.ttl
- tests/counter-examples/training-objective-not-loss.ttl
- tests/counter-examples/training-problem-no-objective.ttl
- tests/example-conformance.ttl
- tests/perf-recall.ttl
- tests/structural.ttl

### SPARQL queries, checked by reading and by counting braces

- **queries/competency/artifact-weight-tensor-shapes.rq:** braces 1/1. Declares gmeow: and math:,
  and uses only those.
- **queries/competency/audit.rq:** braces 1/1. Declares gmeow: and uses only that.
- **queries/competency/metric-observations.rq:** braces 2/2, including OPTIONAL. Declares gmeow:
  and math:.
- **queries/competency/provenance-chain.rq:** braces 1/1. Declares gmeow:.
- **queries/competency/two-sessions.rq:** braces 1/1. Declares gmeow:.
- **queries/project/upper-projection.rq:** braces 3/3, covering CONSTRUCT, WHERE and OPTIONAL.
  Declares gmeow:.
- Every prefixed name in every query uses a declared prefix.

### Clause check of gmeow:TensorArtifactCoherenceConstraint (module.ttl line 558)

The constraint applies to an artifact a with learned model m and computation graph g. Its clauses
are:

1. architectureOf(g, m)
2. architectureOf(g, owner) → owner = m
3. computationNode(g, node) → node is an ApplicationExpression
4. inParameterSpace(w, p) → parameterSpaceOf(p, m) ∨ parameterSpaceOf(p, g)
5. parameterSpaceOf(p, owner) → owner = m ∨ owner = g

Method:

- I diffed each counter-example against conformance-fixtures/tensor-artifact-coherent.ttl, with
  comment lines removed.
- Each one adds `ex:otherModel a math:LearnedModel .` and changes exactly one object list.

| Fixture | Change | Broken clauses |
|---|---|---|
| tensor-graph-other-model | `math:architectureOf ex:otherModel` | 1 (no architectureOf to m), 2 (owner otherModel ≠ m) |
| tensor-weight-other-model-space | `math:parameterSpaceOf ex:otherModel` | 4 (no space of m or g), 5 (owner otherModel ∉ {m, g}) |
| tensor-graph-extra-owner | `math:architectureOf ex:model, ex:otherModel` | 2 only |
| tensor-weight-extra-owner-space | `math:parameterSpaceOf ex:model, ex:otherModel` | 5 only |

In all four, clause 3 holds because ex:forward is an ApplicationExpression.

Comparison with the new comment ("four counter-examples break it: two with a wrong owner and two
with an additional incompatible owner, each once on the graph and once on the parameter space"):

- The comment is true. Two fixtures have a wrong owner and two have an extra owner. Each pair has
  one graph fixture and one parameter-space fixture.
- The comment no longer claims a clause count.

Comparison with docs.md, lines 156 to 160: the four descriptions match the table. "So each
counter-example breaks its one law only" is a claim at the level of laws. It was checked at
3d41394c4 (other shapes and laws) and is not affected by this delta.

The cell rationales (example-conformance.ttl lines 76, 84, 92 and 100) were not changed:

- The two extra-owner rationales say "only the clause ... is broken". That is true: clauses 2 and 5
  respectively.
- The two other-model rationales name a requirement and say no other part of the law is broken.
  "No other part" refers to the node and parameter-space parts, or the graph and node parts, and it
  is true.

### Process wording

- I searched the head commit message and the added lines for `#<digits>`, PR, pull, CodeRabbit,
  review, issue and follow. There were no matches (grep rc=1).
- "(Principles 4, 7)" is the repository's citation convention.

### CI at this head

`gh pr checks 2832` shows lint and helpers passed. producer-build and rust-prebuild are pending.
The generation jobs had not run yet.

## Read

- The full review at 3d41394c4, and the first lines of the review at aa4ea2381 (PASS at
  aa4ea2381).
- `git log` and `git diff 3d41394c4 HEAD`, plus a non-comment-line filter of the diff.
- model-serving:
  - module.ttl lines 550 to 600, covering all three procedural constraints
  - docs.md lines 150 to 175
  - example-conformance.ttl lines 50 to 110
  - every tensor fixture and counter-example, as diffs against the conforming fixture
  - all six .rq files in full
- Evidence: r5-rapper.log, r5-commit.log and the first lines of r5-msg.txt.
- The failed-job log of CI run 43813627179 (the 3d41394c4 run).

## Not read

- The rdf-work SKILL.md and the worktree AGENTS.md in full. I relied on the copy of AGENTS.md
  loaded in this session.
- The math slice. Nothing in it changed, and the 3d41394c4 review checked it for these fixtures.
- The native lowering code, and the slicetest harness beyond what the earlier review recorded.
- `gh issue view 1897`, because the scope is unchanged by a comment-only fix.

## What nobody has shown

- **Native parse at this head:** nobody has parsed the file with the native codec. The CI failure
  stopped at line 62, so the two extra-owner cells added in 3d41394c4 (lines 86 to 100) have never
  been read by the native parser. rapper accepts them, and they are ordinary Turtle like the cells
  around them, but native acceptance is inferred, not observed.
- **CI at 48f9ac572:** still pending. No generation, fixture or test job has completed.
- **Harness runs:** none at this head, for either changed competency question or for any tensor
  conformance cell. That includes exactly one SPARQLConstraintComponent finding for each
  counter-example.
- **`make check`:** not run at this head.
