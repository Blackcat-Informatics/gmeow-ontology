## Observed failure

PR #1830's run [38948027790](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38948027790) gets past the recovery-leg and stable fixture-admission defects. Both independent generation jobs and fixture-prefix then fail in `stage-docs-render` because the native result is valid but unsupported, with 16,575 unsupported entries. The docs projection accepts only a boolean consistency verdict.

`docs/GATE-AND-PIPELINE.md` P3 requires producers to record results and gates to grade them. The docs model also currently treats every class absent from the proved-empty set as satisfiable, even when its global verdict is inconsistent. An unsupported result must never become a positive consistency or satisfiability claim, but its explicit undecided state is legitimate required documentation data.

## Required repair

- Carry an explicit consistent/inconsistent/undetermined documentation assessment, the exact native input/evaluation/completeness/information axes, and unsupported observations.
- Keep native payload validation, exact published handle identity, required Inferred payload, source-owner checks and missing/corrupt-input failures.
- Render an undecided result honestly in health output, badges and the documentation RDF. Non-class not-evaluated and class undetermined must remain distinct. Retain proved class emptiness without inventing satisfiability for other classes.
- Preserve all native reasoning admission and reason/verify gate checks. Demonstrate the same unsupported result remains non-decided for the gate while its documentation observation can be produced.
- Add focused synthetic native-result, RDF, badge and renderer controls. No test may produce the repository corpus.

This repairs the producer/documentation projection. Completing the canonical native semantics remains required by #1757/#1748 and the broader logic goal; an undecided report does not satisfy that acceptance. Principles 4, 7, 17 and 18 apply.
