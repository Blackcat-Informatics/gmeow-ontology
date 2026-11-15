fix(standpoint): a correction does not supersede or suppress the corrected claim (Principles 6, 9, 10)

Correction and suppression are two different acts. `gmeow:correctsClaim` is no longer a sub-property of
`gmeow:supersedes`: a correction is a statement by the correcting standpoint that another claim is wrong, and
the corrected claim is retained unchanged, is not superseded and takes no display flag. Plain disagreement,
where nothing is corrected, is a `gmeow:ClaimEvaluation`. Which claim a reader is shown is decided at
projection.

The per-ranker closure constraint counted any correction by anyone as ending a claim's liveness, so one
party's correction changed what another ranker was held to rank. It now counts a correction only when the
annotation on the `gmeow:correctsClaim` statement names the same ranker. Succession and retraction, a
standpoint's own acts, are unchanged, as is the order constraint.

The lifecycle example no longer hides the corrected claim. A new example shows a disagreement and a
correction with both claims retrievable; a competency question and structural assertions pin the decision,
including that the sub-property edge must not return. The slice guide and the claim-lifecycle guide say which
pattern applies when. French and Chinese entries cover every changed literal.

Verified: independent review PASS at 22d0a0ffb, with the two cases of the revised constraint and every new
query worked by hand; the module, examples and tests read in full. `make lint`, the catalog checks and the
commit hooks pass. Pull request checks: helpers, lint, producer-build and rust-prebuild green; the three
producer jobs and the aggregate red as on main.

Not shown: no producer pass, so that the revised constraint lowers and runs, and that the structural
assertions, the competency question and the new example pass in the harness, are unproven. No fixture
exercises the closure rule: the lowered statement edges it reads are not supplied to validation of raw
fixtures, which is tracked separately. The same older reading remains in other slices and is tracked with the
display-policy work.

Closes #2725.
