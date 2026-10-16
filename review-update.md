Review remediation pushed as 22193fc44 (Principles 4, 7, 17 and 18).

- Restricted blocking now retains original clause exactness before ordinary NAF/builtin filters or property operations are relaxed. Its Datalog closure excludes relaxed clauses and variable-predicate pair projections/reverse bridges. Source specialization preserves the exactness metadata.
- Binary restricted-head searches enforce distinct guards, including bindings of frozen universal variables. Guarded binary producers remain non-blockable after statement lowering discards those guards.
- anyURI length/minLength/maxLength use PurRDF's XML whitespace collapse and Unicode character counts through the existing datatype plan.
- Finite-write products use saturating multiplication. The three source provenance comments and helper-ledger final newline named by hosted lint are repaired.

Executed verification:

- make fmt: passed.
- make nextest-synthetic with the chase, joint and any_uri selection: 124 passed, 2132 skipped; nextest fd9ab4ba-45b1-422a-8c50-6e2ed7f8eb7c. This includes a genuine two-distinct-witness cycle and finite execution-prefix exhaustion, plus valid single-witness inverse examples. Test-profile compilation reported unused manifest dependencies csv and flate2; neither was suppressed.
- make lint: passed.
- Normal git commit and push: succeeded. No hook bypass or compiler/profile weakening.

Current acceptance remains incomplete. No full make check, authenticated fixture production, new production verdict, two cold-generation comparison, or independent completion audit is claimed on this commit. The prior fixture-prefix failure is an unsupported native verdict at docs rendering. The ownership agreement in #1803 assigns the generic DL/chase engines to PurRDF; the recorded released-engine benchmark does not yet decide the two large production worlds within budget. That evidence is not replaced by the focused tests above.

The branch also now contains the previously committed corpus repairs from #1804; that ancestry was already present on the local combined branch and is preserved. Other logic issues and prerequisite PRs remain open pending their individual acceptance and ghprsq integration.

@coderabbitai review

Please reassess the four recorded findings on the current head, particularly the distinction between conservative Skolem-flow over-approximation and the exact evidence used to block restricted triggers. None of the review threads has been resolved merely by posting this update.
