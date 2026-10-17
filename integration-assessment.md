# Integration candidate

The actual origin/main contains the declaration-versus-execution fix merged as
9cb2cb1fc. Its delta from this branch's base affects fifteen logic-compile,
probability test, grounding-contract and translation files. It does not alter
the graph-projection callers or the recorded consumer changed by this PR.
The clean merge-tree prediction is 1fd78d1ef168f143635cc4b39be129686252fdc0.
No base merge is required merely because this branch is behind.

The current local prebuild Clippy and normal hooks qualify 7db11af84, including
the final native pin af8936e6f and the corrected test argument types. The main
declaration fix has its separate attributable evidence in PR 1836's archived
Stage. The current hosted PR checks must still establish the combined candidate;
a clean textual merge alone does not establish that acceptance.

CodeRabbit's current review covers the comparison from 9cb2cb1fc to 7db11af84.
Its lockfile path filter excludes Cargo.lock. The lock delta was inspected:
only the thirty-one PurRDF git source references move with the five direct
manifest pins; no unrelated package version is updated.

The recorded-input benchmark uses historical authenticated products and does not
claim current complete-canon admission. Its corrected default-world operation
is still live. The paired native PR must land before this dependent PR. Review,
full hosted acceptance and final evidence capture remain outstanding.
