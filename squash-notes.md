docs(math): trace estimate provenance through the model the run used (Principles 4, 7, 17)

An inference run now records each model it applied with `gmeow:used`, and the fitted-to-data link stays on
the `math:FittedModel`; a dataset the run itself read is also recorded with `gmeow:used`. The definition and
usage notes of `math:InferenceRun`, the design note, the p-value example with its conformance and
counter-example twins, and the result-lineage fixture follow that one pattern.

The estimate-provenance competency query reaches a model only through the run that used it, so data shared
with another model no longer manufactures a link. Its header states what the query reads and when a model
yields no row. A second competency question covers the adversarial case: one dataset shared by three fitted
models and used directly by both runs, a single-model run, a two-model run, a model no run used, and a
parameter-only estimate.

Verified: independent review PASS at c75d1c6e4, with the expected rows of both competency questions worked
by hand; the whole merging diff read; `make lint` and the commit hooks pass. Pull request checks: helpers,
lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on main.

Not shown: no run has executed the competency questions, the conformance fixtures or the reasoning gate at
this head, because nothing downstream of the producer runs on the hosted runner. The Rust producer still
writes the fitted-to-data link on the run and is tracked separately.

Closes #2674.
