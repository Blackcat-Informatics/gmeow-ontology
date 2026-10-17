# Recorded world consumer: execution evidence

The consumer is committed and pushed as `0327fb993`. Strict affected-example
Clippy passed with warnings denied; normal commit and push hooks passed.
The first bench-profile compilation completed in 25m 39s. This is compilation,
not a native reasoning measurement.

Three existing pipeline-profile contributions sharing one recorded build were
hydrated through the strict native read-only cache. Exact action, receipt,
product and blob identities are retained in the logs and input locator. The
first list operation spent 933ms, 2ms and 3637ms hydrating source-load, statements
and compile-logic respectively, then 2774ms assembling the object-level carrier.
It produced no corpus and wrote nothing to the selected input store.

The actual assembled assertion census is:

| World | Assertions |
| --- | ---: |
| default | 179177 |
| demo/jointly-acyclic | 22 |
| demo/model-summarizing-acyclic | 18 |
| demo/super-weakly-acyclic | 22 |
| examples | 13436 |
| imports | 8099 |
| logic | 52366 |
| relational-core | 377397 |
| statements | 114 |

Named-world IRIs use the `https://blackcatinformatics.ca/gmeow/graph/`
prefix. Named projection used the native graph operation, including its RDF 1.2
statement-layer selection rules.

| Selected world | Prepare | Decision | Proof replay | Native answer | Retained boundaries |
| --- | ---: | ---: | ---: | --- | --- |
| imports | 15ms | 13ms | 11ms | True / DecidedWithinBoundaries | DataRange |
| logic | 259ms | 180ms | 209ms | True / DecidedWithinBoundaries | DataRange, UnrecognizedTerm |

Each decision retained its original native certificate and one proof run, which
replayed successfully against the same dataset and Consistency question.
Boundary-bearing answers are not unrestricted consistency claims and do not
establish GMEOW complete-canon admission. There is no measured speedup ratio
against the old class engine on these exact inputs.

All six actual refusal controls terminated with a failure exit: empty CLI
arguments, missing receipt, wrong producer profile, wrong producer stage,
altered receipt payload and duplicate source selector. The missing receipt
retained its original I/O cause. No control invoked a producer or repaired the
cache. Logs are `control-*.log`; edited selectors are separate Stage files,
never mutations of the input store.

The default operation exposed a defect in this new benchmark: its authored-default
shortcut selected 180062 assertions before complete object-level filtering,
whereas the actual assembled default has 179177. The broader input refused with
the original native role-hierarchy InvalidProperty failure at term 6745. That
refusal is not a result for the actual assembled default world. The shortcut
must be deleted, not retained as a fallback.

Active correction: [PurRDF 566](https://github.com/Blackcat-Informatics/purrdf/issues/566)
replaces the named-only projection with one native default-or-named graph
projection, migrating existing callers without a legacy alias. Its isolated
implementation and graph-isolation controls are in progress. After integration,
the GMEOW benchmark will project every selected world from the full production
assembler and rerun the actual default world. No class-engine deletion or full
engine migration is claimed yet.
