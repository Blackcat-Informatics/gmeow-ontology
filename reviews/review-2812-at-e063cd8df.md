PASS at e063cd8df

# Delivery review: issue 2812 at e063cd8df

Reviewed 2026-11-15 16:33 -0600. Worktree
`/home/paudley/Active/gmeow-ontology/.worktrees/2812-native-engine-refuses-a-rule-head`.

## Scope and state

- `git rev-parse HEAD` = e063cd8df74a2422fde505037d5042ae805bae0a. `git status --porcelain` is empty.
- origin/main = 96f9fd6cd. `git merge-tree --write-tree origin/main HEAD` exits 0 (tree 912db092a), so
  the merge is clean. origin/main beyond b4804439f changes nothing under `crates/logic`, `prove.rs`,
  `prove_cli.rs` or `README.md`.
- First-parent history since the earlier PASS (2218abafa):
  - 6f85e5a3e merges origin/main at b4804439f.
  - e063cd8df rewrites one test.
- Whole branch diff (`origin/main...HEAD`): 11 files, +955/-275. They are README.md, `prove.rs`,
  `prove.tests.rs`, `prove_cli.rs`, `error.rs`, `chase.rs`, `chase.tests.rs`, `physical/mod.rs`,
  `source_grammar.tests.rs`, `relational_core.rs` and `relational_core/tests.rs`.

## 1. The delta 2218abafa..HEAD

**The main merge (6f85e5a3e).** `git merge-tree --write-tree 2218abafa b4804439f` produces tree
a5ee1df80. That is exactly `6f85e5a3e^{tree}`, so the merge is a pure automatic merge with no manual
edits. Main's changes from the old base 417fd45d7 to b4804439f touch none of the branch's own files.
I checked those files plus `physical/`, `rule_ir`, `reason/dl*` and `operator_rules`, and
`git diff --stat` is empty for all of them. The merge therefore changes no line the branch wrote or
calls into on the chase or prove path.

**The branch's own change (e063cd8df).** It changes only
`crates/logic/src/physical/seminaive/joint/source_grammar.tests.rs`, +25/-2:
- It adds a PROTECTS doc comment.
- It replaces `assert!(PreparedChaseRule::new(rule.clone()).is_err())` with the assertions examined
  below.
- It factors `certificate_with_external` into `certificate(rule, external)`. The external-writer test
  keeps its exact behaviour because it still passes `rule()`.

No production line changed. All other branch files are byte-identical to 2218abafa: a
`git diff --stat 2218abafa HEAD` over them is empty.

## 2. The rewritten test `constructor_grammar_checks_the_prepared_allocation_route`

Mixed head (tests.rs:7-25 plus 108-120):
- `FIRST(?cell, ?value)`
- `REST(?cell, nil)`
- `ONE_OF(urn:class, ?cell)`
- `instanceOf(?cell, urn:relation)`
- `naryArg0(?cell, ?value)`

The body is `seed(?x, ?value)`, so `?cell` is the only existential.

| Line | Assertion | Production line it protects |
|---|---|---|
| 103-107 | the base rule's `uses_only_frontier_skolems()` is true | chase.rs:965-971 (no `naryArg` gives empty groups) and chase.rs:302 |
| 121 | the mixed rule now prepares (`unwrap`) | chase.rs:977-980 skips non-`naryArg` atoms and chase.rs:1009-1018 takes `instanceOf` only on a reifier. Before the change this rule was refused by the removed "mixes a non-reified predicate" branch. The coder's `confirm-source-grammar-6f85e5a3e.log` shows the old assertion failing at tests.rs:116 against the merged code, which matches. |
| 122 | `witness_policy == FrontierSkolem` | None in production: `new` stores the rule unchanged. This is a precondition that makes line 123's `false` attributable to the reifier groups only (see O2). |
| 123 | `!mixed.uses_only_frontier_skolems()` | chase.rs:302 (`&& self.reifier_groups.is_empty()`) together with chase.rs:316 (groups are populated for a mixed head) |
| 124 | `certificate(rule())` is `Some` | Positive control: the harness can certify. Without it, line 125 could pass through a harness fault. |
| 125-128 | `certificate(mixed)` is `None` | source_grammar.rs:188, the `!producer.uses_only_frontier_skolems()` arm. This assertion can fail. |
| 129-133 | without the typing atom, preparation is `Err` | chase.rs:1042-1049 (a group with no `instanceOf` is refused) |
| 134-137 | a pure tuple head prepares and is not frontier-only | chase.rs:302 and 316. Unchanged from before. |

Why line 125 can fail: I traced `closed_head` (source_grammar.rs:89-162) on the mixed head.
- The policy is FrontierSkolem.
- `?cell` is fresh.
- The `FIRST` object `?value` is not fresh, and `witness_frontier` is `None`, so line 126-134 does not
  reject.
- `REST` is nil.
- The `ONE_OF` subject is a constant.
- `instanceOf` and `naryArg0` are not grammar predicates, so they are skipped.
- The walk `cell → nil` closes.

So `closed_head` returns **true**. With `producer_effects`, `ValueFlow` and empty facts, possible
facts and external writers, the only way `certify` returns `None` for this rule is the route check on
line 188. Delete that arm, or the `reifier_groups.is_empty()` conjunct at chase.rs:302, and line 125
goes red.

**Is "a cell minted by tuple identity is not admitted to the closed-list proof" true of the code?**
Yes.
- chase.rs:469-490: when `reifier_groups` is non-empty, a reifier variable is skipped by the
  value-null loop. It is bound to `mint_nary_reifier(rel, arg_values)`.
- provenance.rs:509-519: that identity is `NARY_REIFIER_PREFIX` plus the SHA-1 of relation and
  argument terms. It contains no rule, scope, world or ordinal.
- The ordinary `FIRST`, `REST` and `ONE_OF` atoms are then grounded on that same node
  (chase.rs:505-510).
- source_grammar.rs:188 refuses such a producer.

**Is the refusal sound?** Yes. Withholding the certificate removes only the `Completed`→`Positive`
schedule relaxation (source_grammar.rs:290-301) and the `ConstructorClosedLists` basis
(dl.rs:1359-1365). Admission then waits on completed grammar reads (dl.rs:1362-1365), which is the
ordinary completion route. No fact is dropped and no weaker semantics is substituted.

**Is the refusal necessary?** Yes, given the proof's design.
- The module's invariant (source_grammar.rs:8-9) is that each cell's first and rest are functions of
  its witness address.
- `closed_head` checks one producer at a time, and `certify` has no check across producers for a
  shared cell address.
- A frontier Skolem address includes `rule_iri`, scope and ordinal (chase.rs:859-864), so two
  producers cannot share a cell.
- A tuple address is shared by every rule that derives the same relation and arguments. Two producers
  that each pass `closed_head` could write different `rest` values onto one node.

Admitting tuple-minted cells would let the proof certify an ambiguous list.

The PROTECTS comment (tests.rs:95-99) and the commit message describe this accurately. `prose-check`
on both: EXIT=0.

## 3. Other tests that pin the old refusal or the old head-shape behaviour

I searched all of `crates/**/*.rs`, not only `crates/logic`, excluding chase.rs itself, for:
- the removed message texts: `mixes a non-reified`, `must be instanceOf or naryArg`,
  `carries no naryArg`, `reified-tuple atoms`;
- the removed function `reified_nary_head`;
- the surviving refusal texts: `bound by the body`, `not existential`, `fresh existential`,
  `relation is unknown`;
- the new surfaces: `validate_head_shape`, `RefusedRuleHead`, `head relations [`;
- every `PreparedChaseRule::(new|from_source|from_domain)(` in test code;
- every test file in `crates/logic` that mentions `naryArg`, `nary_arg` or `instance_of_iri`:
  `nary/tests.rs`, `chase.tests.rs`, `reason/mod.tests.rs`, `relational_core/tests.rs`,
  `source_grammar.tests.rs`, and `crates/logic/tests/` (none there).

Result: no test still asserts the old refusal of a mixed head, or the removed refusal of an empty
argument list.
- The hits for `bound by the body` and `not bound by the body` are about something else: the surviving
  frontier-reifier refusal (chase.tests.rs:249, relational_core/tests.rs:500), or range-restriction
  residue in the compiler (`logic-compile` relational_core tests, reason/mod.tests.rs:1602,
  shacl_af.rs).
- `nary/tests.rs:103,124` refuse non-range-restricted and empty heads in `lower_nary_rules`, which is a
  different recognizer.
- The `reason/mod.tests.rs` n-ary hits are pre-reified EDB facts and derived argument triples. They
  do not depend on head-shape acceptance.

The only stale reference is a comment (O1).

## 4. Does the earlier PASS still hold for the whole branch diff at this head?

Yes.
- The production code on the chase, lowering and prove paths is unchanged from 2218abafa.
- The merge is a pure automatic merge that changed none of the branch's files.
- Current main still merges cleanly.
- The rewritten test now states the delivered behaviour, where before it asserted the refusal the
  issue removes.
- Acceptance boxes 1 to 5 map as tabled in `review-2812-at-2218abafa.md`, and that mapping still
  holds.

## The four failures in `nextest-grammar-run.log` (207 run, 203 passed, 4 failed)

That log records HEAD 6f85e5a3e. The rewritten test PASSes in it (line 82/207), while the committed
version at 6f85e5a3e fails (`confirm-source-grammar-6f85e5a3e.log`). So the run was on a dirty tree
carrying the uncommitted rewrite. The commit 3 minutes later (`commit-sg.log`, COMMIT_EXIT=0)
committed only that one path. I did not establish the four failures. From reading the code:

- `operator_rules::…hydration_retains_native_source…` (test_support.rs:16) and `enactment_gate …`
  (prepared_verify.rs:26) panic with `NotPresent` for the producer-selected fixture. This is the
  expected refusal when no fixture manifest is present. No branch code is on that path.
- `reason::dl::tests::one_of_on_ordinary_class_stays_decided` (dl.tests.rs:2644). The refusal is a
  `NativeSourceAdmission` with `IncompleteList` on the flat `logic:oneOf` fixture in graph/logic,
  emitted at reason/program.rs:666. That is source admission of a source construct. It involves no
  rule head, `naryArg`, or chase preparation. The constructor certificate is also not the cause: it
  only relaxes admission and cannot produce an `IncompleteList` issue. Not on this branch's path.
- `rule_ir::reduction::tests::reduction_and_existential_heads_share_completion…` (reduction/tests.rs:588,
  `witness_derivations.len()` is 2 where 1 is expected).
  - The head is `copy(?group, ?copy)`, binary with no `naryArg`.
  - The branch's code is reached only nominally. `validate_head_shape`, called from
    relational_core.rs:164 if this lowering goes through it, returns at chase.rs:965-971, and the chase
    takes the unchanged `reifier_groups.is_empty()` arm (chase.rs:451-458).
  - The branch changed only comments in that arm.
  - Not caused by this branch as far as reading shows.

`branch-two-tests.log` (e063cd8df) and `baseline-main-two-tests.log` contain only headers and no
result, so they show nothing yet.

## What I read at e063cd8df

- `crates/logic/src/physical/chase.rs`:
  - 260-359 (`PreparedChaseRule`, `uses_only_frontier_skolems`, `new`, `from_source`, `from_domain`)
  - 425-523 (firing and minting)
  - 852-869 (`mint_witness`)
  - 900-1122 (`refused_head`, `validate_head_shape`, `reified_nary_head_groups`, `mint_order`,
    `eval_term_value`)
  - the full branch diff of the file
- `crates/logic/src/physical/seminaive/joint/source_grammar.rs`: 1-317, all of it.
- `crates/logic/src/physical/seminaive/joint/source_grammar.tests.rs`: 1-227, all of it, plus
  `git show e063cd8df`.
- Branch diffs of `crates/logic/src/error.rs`, `physical/mod.rs` and `relational_core.rs`.
- `crates/logic/src/provenance.rs`: 509-519.
- `crates/logic/src/physical/seminaive/joint/input.rs`: 755-795.
- `crates/logic/src/reason/dl.rs`: 1345-1385.
- `crates/logic/src/rule_ir/reduction/tests.rs`: 530-591.
- `crates/logic/src/reason/dl.tests.rs`: 2600-2650.
- `crates/logic/src/nary/tests.rs`: 15-30.
- The search results listed in section 3.
- Coder logs:
  - `nextest-grammar-run.log` (header, command, results and the four failure blocks)
  - `confirm-source-grammar-6f85e5a3e.log`
  - `commit-sg.log` and `commit-sg.sh`
  - `run-nextest.sh` and `run-two.sh`
  - headers of `branch-two-tests.log` and `baseline-main-two-tests.log`
- Issue 2812 (`stagectl issue 2812`, body; it has no comments).
- `review-2812-at-2218abafa.md`.

## What I did not read or run

- I did not re-read `prove.rs`, `prove.tests.rs`, `prove_cli.rs`, README.md, `chase.tests.rs` or the
  `relational_core/tests.rs` bodies at this head. They are byte-identical to 2218abafa, where the
  earlier review read them, and I rely on that review for them.
- I did not read the rest of `chase.rs` (the certifier and below).
- I ran no build, test, clippy, make, producer or gate. Every test result above is the coder's
  claim, read from their logs.
- I have no clippy evidence for the test target at e063cd8df, and no clean-tree test run of the
  rewritten test at e063cd8df (see O3).
- `make check` has not run on this head. It is still required before merge, and it is not part of
  this review.
- One stray write: a discovery command redirected `git diff --name-only` into
  `/tmp/main_files.txt`. It is outside the repository and the worktree. Nothing else was written
  except this file.

## Findings

REQUIRED: none.

OPTIONAL:
- **O1. A stale reference to a removed function.** `crates/logic/src/nary/tests.rs:24` still says the
  test exercises "the multi-reifier generalization of `reified_nary_head`". This branch deleted
  `reified_nary_head` (chase.rs diff), and the surviving recognizer is `reified_nary_head_groups`. The
  comment now names a function that does not exist. Smallest correction: say "exercises
  `reified_nary_head_groups` with two reifier groups", or drop the clause.
- **O2. Line 122 echoes an input.** `source_grammar.tests.rs:122` asserts the stored `witness_policy`,
  which `PreparedChaseRule::new` never changes. A more useful assertion in its place is
  `assert!(closed_head(&rule, SemanticVocabulary::Exact))`. It pins that the mixed head passes the
  grammar, so the `None` on line 125 is proven to come from the allocation route
  (source_grammar.rs:188) and not from `closed_head`. Today that holds only by the trace in section 2.
- **O3. The evidence for the rewritten test was taken on a dirty tree at 6f85e5a3e.** It was not
  taken at e063cd8df, and `run-nextest.sh` records no `git status`. The commit committed only that
  file 3 minutes later, so the tested bytes are very probably the committed ones, but this is not
  proven. The pending `branch-two-tests.log` run, or the final `make check`, at e063cd8df closes this.
- Carried forward from the earlier review, not reopened: O1 (a repeated n-ary atom in one head is
  refused), O6, O7, O8, O9 and O10, and the separately tracked exit 5 for a head subject grounded to a
  literal.

VERDICT: PASS
