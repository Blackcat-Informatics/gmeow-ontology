# Completed hosted failure

Retrieved completed job logs through the GitHub API. Terminal escapes were removed before reading. No hosted job was restarted or cancelled.

| Workflow run | Job | Observed failure |
| --- | --- | --- |
| 39454734148 | 118742222497, generation (b) | stage-export-pydantic: compiled ontology class Blutdruck-Diastolic-cardinality has no term documentation |
| 39454734148 | 118742222689, generation (a) | Same refusal |
| 39458632748 | 118752733577, generation (b) | Same refusal |

The first two failures occurred after the merged Turtle source correction. The subsequent PR generation repeated the same model-export failure. Prior duplicate Agent documentation ownership is no longer the reported blocker. The new patch has not yet run through a completed authenticated producer; these failures are not qualification of its result.
