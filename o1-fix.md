# O1 header fix evidence

- date: Sat 14 Nov 2026 05:30:03 PM CST
- pwd: /home/paudley/Active/gmeow-ontology/.worktrees/2674-repair-estimate-provenance-model
- old head: 5c55e409e
- new head: 750ca179b
- file: slices/grounding/math/queries/competency/statistics-estimate-provenance.rq (header comment only)
- commit hook exit status: 0
- push exit status: 0

## Changed sentences

1. Before: "A dataset the run itself gmeow:used is not reported and links no model."
   After: "The query does not read a dataset the run itself gmeow:used; the data column is the model's math:fittedToData, and a run's dataset links no model."
   (Review finding O1, wording as given.)

2. Before: "One row per (estimate, run, model used by the run, assumption)."
   After: "One row per (estimate, run, model used by the run, assumption), repeated for each estimator and estimand the estimate names and each dataset the model was fitted to."
   (The SELECT also projects ?data, ?estimator and ?estimand; module.ttl caps none of them at one value.)

3. Before: "A run that used several models yields rows for all of them: the query never picks one model and never claims the model is unique."
   After: "A run that used several models yields rows for every one that carries both its data and an assumption: the query never picks one model and never claims the model is unique."
   (The body requires math:fittedToData and math:modelAssumption on the model, with no OPTIONAL.)

4. Before: "An estimate whose run used no math:FittedModel yields no row."
   After: "An estimate whose run used no math:FittedModel yields no row, and neither does a model with no math:fittedToData or no math:modelAssumption."

All other header sentences were checked against the body and left unchanged. The query body is unchanged.

## git diff --stat
```
 .../competency/statistics-estimate-provenance.rq       | 18 +++++++++++-------
 1 file changed, 11 insertions(+), 7 deletions(-)
```

## Push output
```
remote: 
remote: GitHub found 36 vulnerabilities on Blackcat-Informatics/gmeow-ontology's default branch (2 critical, 15 high, 14 moderate, 5 low). To find out more, visit:        
remote:      https://github.com/Blackcat-Informatics/gmeow-ontology/security/dependabot        
remote: 
To ssh://ssh.github.com:443/Blackcat-Informatics/gmeow-ontology.git
   5c55e409e..750ca179b  paudley/2674-repair-estimate-provenance-model -> paudley/2674-repair-estimate-provenance-model
```
