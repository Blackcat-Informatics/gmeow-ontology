# Class label copy investigation: rejected

The experimental whole-map clone removal was not retained. The exact proposed
source, benchmark and regression are preserved in class-labels-investigation-final.patch;
no experimental production, benchmark or test edits remain in the source branch.
This does not replace the current production path with an unproven faster version.

Same-harness Criterion central estimates (milliseconds):

| Labels | Original | Candidate | Candidate repeat |
| --- | ---: | ---: | ---: |
| 250 | 25.003 | 29.155 | 30.993 |
| 500 | 53.790 | 64.446 | 111.02 |
| 1000 | 253.65 | 380.84 | 316.40 |

The candidate did not establish an improvement over the original. No machine or
resource state was inspected, no cause was inferred from timing variation, and
no pipeline-wide speed or memory claim follows. Three separate raw Criterion
captures and benchmark logs retain the measurements without overwriting baseline.

The pre-change eight-case native execution selection passed, including the new
wide-label conflict/equality control. The candidate broad nextest filter also
selected a corpus-backed list case, which correctly failed closed because the
synthetic runner supplies no authenticated fixture manifest. That is not a
candidate production failure or a full test pass; no corpus production occurred.
See class-labels-candidate-tests.log for the exact executed inventory and result.
No further build is required to qualify source that has been entirely removed.

The published admission increment and current-head review remain unchanged.
Full-canon semantics, prepared-base packaging, installed proofs and production
performance remain open under plan.md; this experiment claims none of them.
