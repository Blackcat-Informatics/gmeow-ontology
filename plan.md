# Preserve supported integrity guards

Principles 4, 7, and 17 govern this correction. The canonical frontend already emits existential co-occurrence guards, and ConstraintIr explicitly admits positive existential outer guards. The violation compiler must preserve that capability with lexical scope intact. The two new diagnostic kinds must also appear in the existing complete catalog and eager registration path.

The preceding PR is merged and its local worktree is cleaned. Its complete pre-merge evidence is retained in the ghprsq audit ref. The two findings arrived after integration and are owned here immediately. The processor-count suggestion is declined under the user's explicit prohibition on machine probing and resource caps; that scanner is unchanged by this correction.

## Task 1: Reproduce both defects

Add an authored co-occurrence fixture through the real frontend and enactment gate, checking both missing-side cases, both-present success and both-absent behavior. Add an independent catalog and eager-registration check naming the two diagnostic kinds. Run those focused producer-independent Make tests against the current implementation and retain their actual failures.

## Task 2: Repair scope and registration

Lower only positive atoms, conjunctions and existential guards. Give each existential binder a fresh name, reserving all authored variables and preserving lexical shadowing and sibling independence. Reuse the existing term-scope and variable-inventory logic rather than keeping a duplicate walker. Apply the same guard lowering to outer and nested implication guards. Keep universal and other unsupported guard forms explicit residue, and preserve focus, universal-operand and intrinsic-operand safety and all-or-nothing law enforcement.

Add both diagnostic kinds to the existing catalog and eager registration in matching order. Preserve their typed original causes and all production callers. No ontology edits, alternative checker, runtime dependency, compatibility layer, or weaker equality implementation is needed.

## Task 3: Qualify and integrate

Run focused scope and production-gate regressions, existing positive-head scope controls, diagnostic controls and strict affected Clippy. Refresh only the explicitly authorized tiny witnesses after final semantic changes, inspect the diffs, and run their read-only consumers. Normal hooks remain mandatory. No local full producer, corpus factory, full gate, machine probe, resource cap or subagent is authorized. Reviews are self-reviews unless actual external feedback is available.

Publish the correction, respond to the two source review threads with actual evidence, resolve the legitimately declined scanner thread, inspect complete current reviews and hosted checks, and integrate only through ghprsq. The existing user instruction permits pending hosted checks; missing production acceptance stays visible on the original open issue. Archive evidence before owned cleanup and keep the full logic goal active.
