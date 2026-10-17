# Current artifact regression qualification

The fixture repair is already integrated on main as 13a44f7a9, with later artifact
corrections in b797a315e and eca80ae00. The old standalone 784e499e7 commit is not
an ancestor because the repair was integrated under another commit identity.

A fresh selection on the current combined proof branch passed all 16 synthetic
artifact cases in 0.278s, including closure_emits_triple_and_reifier_with_provenance
and rejection of missing or mismatched native execution evidence. Native typed,
directional and recursively quoted RDF 1.2 artifacts also passed. Command:
make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & test(reason::artifacts::tests::) & !test(corpus_)'.
Log: issue-1782-current-artifact-tests.log.

The two authenticated corpus-verifier cases and full gate remain unqualified.
Current pipeline prerequisite qualification is ongoing; no test produced corpus
fixtures, no invariant was weakened, and the issue remains open. Cleanup dry-run
refused the old remote branch as not ancestry-merged. No force or deletion was used.
