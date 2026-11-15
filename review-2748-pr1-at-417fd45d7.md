PASS at 417fd45d7

# Recheck: 2748 PR 1 at 417fd45d7

- **Head:** `417fd45d7b715272026f015d9561075b7b935fbd` (`git rev-parse HEAD`), parent 378795a38. The
  worktree is clean.
- **Delta reviewed:** `git diff 378795a38 417fd45d7`.
- **Whole branch:** 21 files, +2213 −70 against the merge base. `git merge-tree --write-tree origin/main HEAD`
  against `origin/main` `862638ad8` exits 0.
- **Written:** 2026-11-15 03:49:06 CST (`date`).
- **Mode:** read-only. No make, cargo, producer, gate or reasoning run.
- **Previous report:** `review-2748-pr1-at-378795a38.md` (PASS).

## Scope of the delta

`git diff 378795a38 417fd45d7 --stat` shows exactly two files, as claimed:

- `crates/logic/src/reason/claims.rs`, +3 −3;
- `crates/logic/src/reason/claims.tests.rs`, +4 −2.

No other path changed, so nothing under `crates/logic/src/physical`, `crates/logic/build.rs`,
`crates/logic-compile` or `crates/build-inputs` changed. The witness files are untouched.

## O1 from 378795a38: closed

The row `"ex:r2 ex:r1 ex:Wrong ."` was added to the table in
`a_claim_another_carried_statement_mentions_is_named_and_adds_no_fact` (claims.tests.rs:561). It
expects the theory to stay consistent and the unread reasons to be exactly `[mentioned(R1, R2, 1)]`.

Without the predicate check (`parts.push(quad.p)`, frontend/claims.rs:191), nothing else could keep
`ex:r1` out:

- `ex:r1` is not the subject of any statement beyond its own evidence;
- no statement has `ex:r1` as its object (here the object is `ex:Wrong`);
- `ex:Wrong` is not a triple term, so no quoted triple mentions `ex:r1`;
- `ex:r1` names no graph.

`ex:r1` would then be read. Its claim `ex:a logic:instanceOf ex:Right` would contradict the
`CONTESTED` assertion `ex:a logic:instanceOf ex:Left` (claims.tests.rs:539-542). The `consistent`
assertion would fail, and so would the reason list. So the row protects that check.

This is established by reading the code. I did not run the test with the line removed, because this
review is read-only.

## O2 from 378795a38: closed

The new drop text is "its carrier {carrier} is mentioned by {n} statement(s), first that of {by}[ in
graph G]" (reason/claims.rs:108). `by` is always the subject of the mentioning quad (`node_name(store,
quad.s)`, frontend/claims.rs:183, :197). The text is true for every way a reifier can be mentioned:

| Mention | `by` |
|---|---|
| Inside a triple term, at any depth | The reifier or other subject whose statement holds the triple term |
| Classic statement node (`rdf:subject`, `rdf:predicate` or `rdf:object` pointing at the carrier) | The statement node |
| Predicate of a plain statement | The subject of that statement |

The doc comment (reason/claims.rs:57-60) now lists all three forms.

`git grep` finds no remaining "carried statement(s)" and no other occurrence of "is mentioned by" in
the repository. The one test that matches drop text uses only the unchanged prefix
`its carrier https://example.org/prove/r1 is mentioned` (prove_cli.rs:757). README, PROVE_SEMANTICS,
LOGIC-IR and LOGIC-SEMANTICS do not quote the drop wording.

## Logs at 417fd45d7 (n3-pre-* ignored)

`n3-batch.sh` runs, in order:

1. `make fmt` (n3-pre-fmt) and a pre-commit test run (n3-pre-logic);
2. `git add` of the two files, then the commit;
3. the checks below, each of which must exit 0;
4. the push.

Each log below begins with the worktree path and the head `417fd45d7b71…`, and has an empty
`git status --porcelain` block. The exception is `n3-commit.log`: it starts at 378795a38 with only the
two files staged, shows every hook passing, and ends `[… 417fd45d7]`.

| Log | Result |
|---|---|
| n3-commit.log | hooks passed, `EXIT=0` |
| n3-nextest-logic.log: `make nextest-synthetic NEXTEST_FILTER=package(gmeow-logic) & test(/reason::claims\|reason::program::\|program_analysis\|lower::tests\|witness_tests\|native_semantics\|retention_tests/)` | 239 run, 239 passed, `EXIT=0` |
| n3-nextest-compile.log: `package(gmeow-logic-compile) & test(/native_statement_tests\|frontend::tests\|ir::tests/)` | 318 run, 318 passed, `EXIT=0` |
| n3-cli.log: `cli-real.sh`, the prove CLI tests | 26 run, 26 passed, `EXIT=0` |
| n3-clippy.log: `cargo clippy --locked … -p gmeow-logic-compile -p gmeow-logic --all-targets -- -D warnings` | recompiled gmeow-logic, `EXIT=0` |
| n3-hygiene.log: `make helpers-hygiene` | `EXIT=0` |
| n3-recipe.log: `make producer-recipe` | `EXIT=0` |
| n3-push.log: `git push`, 378795a38..417fd45d7 | `EXIT=0` |

The logic run includes the extended mention-table test (:117) and the witness pin test
`world_local_admission_keeps_its_witness_contract_and_witness_iri` (:43). The CLI run includes
`a_claim_refuted_by_another_claim_is_not_read` (:46).

## Findings

**REQUIRED:** none.

**OPTIONAL:** O3 from the previous report still stands. `cli-real.sh` embeds the bundle from
`.stage/deliver-2695/bundle-pass12` rather than one produced at this head. Producing a bundle at the
final head remains the gate's job.

## What this verdict relies on

Everything else on the branch is unchanged since 378795a38. This verdict therefore carries forward:

- `review-2748-pr1-at-378795a38.md`: every scan covers every graph, named-graph handling, drop
  accounting, consistency of the texts, the tests fail if the scans are narrowed back, the witness
  contract;
- `review-2748-pr1-at-74e396902.md`: the mention checks;
- `review-2748-pr1-at-13514df01.md`: the `Debug` pin and the allow-list;
- `review-2748-pr1-at-dfc7c6414.md`: admission profile identity, batching, pipeline call sites and
  source admission.

## Not verified

- `make check` on the tree merged with `origin/main` `862638ad8`. This includes whether the pipeline
  closure is unchanged apart from the content-addressed engine identity.
- Execution of the new row with the predicate check removed. That it would fail is established by
  reading only.
- The behaviour on a large corpus.
