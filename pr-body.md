The merged producer compiled successfully but failed authentication because
compiler-message package IDs were compared with normalized source-selection IDs.
Pages then continued after that failed prerequisite and repeated the producer
build for another 18 minutes.

This repair normalizes compiler artifact, generated-input and external-root
owners through the same exact Cargo metadata identity. The existing source-read,
recipe and executable authentication checks remain mandatory. Docs prerequisites
now stop at the first failure, with an inert actual-recipe regression required by
both lint targets. Principles 4, 7 and 18 apply.

Closes #1825. Closes #1826.

Validation:

- Three focused producer identity tests passed, including actual synthetic
  dep-info matching and the compiler-read seal, generated ownership, unknown and
  unselected inputs, external roots and contradictory metadata.
- Producer-independent workspace/all-target Clippy passed with warnings denied.
  Those checks ran against the identical producer patch in the existing reasoning
  checkout; four corpus-dependent consumer packages were excluded.
- The docs prerequisite regression passes on this source and fails against the
  previous recipe. It never invokes a real producer or builds a corpus.
- Normal commit hooks passed for all commits, including source purity,
  formatting, issue-reference policy and ShellCheck where applicable.

Exact-source optimized producer and downstream CI remain pending. No local full
gate or independent completion audit is claimed. Existing producer fingerprinting
reads host CPU/environment data prohibited by the user; no authentication or
profile setting was weakened to avoid that boundary. CodeRabbit reviewed the
functional changes without actionable code comments. Its documentation-coverage
finding is addressed in the latest commit; updated review and exact-source CI
remain pending.

The least-established behavior is the full optimized producer/receipt path,
which requires the hosted run. The important additional finding is the duplicate
Pages build after a terminal prerequisite failure; its regression is included
here alongside the failure that exposed it.
