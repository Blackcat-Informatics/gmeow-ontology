## Observed failure

Pages on merged main `07f4c6c59db06962c6a17d104f320d36359be826` failed at
https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/38927612888/job/117063071664.
The optimized producer finished in 29m40s and authentication rejected the selected
docs-print source. Instead of terminating there, docs check-sync continued to its
`make build` prerequisite, entered cli-build/producer-build and compiled again
for 18m05s before reporting the same error. Deployment was skipped.

The docs-only conditional recipe separates `make check-sync` and `make build`
with semicolons and does not enable shell error propagation. The failed first
prerequisite therefore does not prevent the second command.

## Required repair

Make the docs prerequisite chain fail immediately when either prerequisite
fails; never run the next prerequisite or final sync after failure. Preserve
generated -> build -> docs ordering on success. Add an inert regression that
uses the actual recipe with fake producer commands; no test may run a corpus
producer or authenticate/fingerprint the host.

Related: #1825 repairs the authentication failure that exposed this second
defect. Both failures require fixes; correcting package identity alone does not
make the docs prerequisite chain safe. Principles 4 and 7 apply.
