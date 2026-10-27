Schema vocabulary discovery silently dropped directory-open and entry errors, allowing required ontology inputs to disappear. Discovery now propagates contextual failures through the three stage input declarations and ontology loader, and rejects selected module paths that are not ordinary files.

Recursive exact-name selection, deterministic traversal, followed symlinks and legitimate empty readable vocabularies are preserved. Vocabulary derivation and output formats are unchanged. Principles 4 and 7 govern the complete input boundary.

Independent review, formatting/diff hygiene, strict pipeline/developer CLI compilation, helpers and static producer-recipe admission passed. Both authored temporary-tree controls passed after a 5 minute 1 second build and 0.071 second execution; 1,207 unrelated cases and one library binary were excluded. These controls cover healthy selection and required-input refusal without corpus production. Normal signed commit and push hooks passed.

Standing exception: CI waived; no local optimized producer, canonical corpus/fixture construction, synchronization or full gate. Actual corpus, installed consumer and release acceptance remain unverified. Remaining contracts: #1776, #1777, #1336 and #1962.

Closes #2005.
