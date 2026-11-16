test(quality): witness multi-result reports, categorical verdicts and the aggregate law (Principles 4, 7, 17, 19)

The terms for quantitative quality assessments are on main: a result carries its quantity with its own unit and
frame, quantity and dimension semantics stay with `math:`, and a report may hold several results over several
dimensions. This adds worked data and controls in the `quality` core slice. No module term, catalog or
manifest changes.

A worked example, `examples/multi-result-multi-dimension.ttl`: one report with two quantities, a length with
its metre unit and SI frame and a dimensionless ratio, under two quality dimensions that are not paired with
the results, beside a categorical assessment of the same dataset with two verdicts.

Four competency questions over the quantitative, categorical, multi-dimension and multi-result data, each
declaring exact rows; the existing aggregate question now declares exactness too.

Two structural assertions. One pins the aggregate result law's formula tree: its five per-result conjuncts,
each existence and target relation tied to the result variable, and each typing obligation bound to the
relation it types and to the variable that relation binds, so removing a conjunct or moving a typing
obligation fails it. The other bans a fixed physical dimension on a quality dimension, with a fail-witness
fixture.

Verified: independent review PASS at 480a62e42; the whole diff read. The aggregate pattern was worked by hand
against the module: true as authored, false under each single mutation tried. At that head the changed Turtle
parses and `make lint` exits 0. Pull request checks: helpers, lint, producer-build and rust-prebuild green; the
three producer jobs and the aggregate red as on main, with no error before the producer is killed.

Not shown: nothing has executed the new material. No producer pass, harness, gate or reasoning run; no query
engine has run the patterns, so the questions and assertions are worked by hand. The pattern does not pin the
assessment guards of the aggregate law, and its rationale does not claim to.
