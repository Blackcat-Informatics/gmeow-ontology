fix(lang): give the two spellings of a plain string one GMN classification, order and reference key (Principles 4, 7)

The GMN-1 codec recognised a plain string only when its datatype was spelled out as `xsd:string`, and the RDF
library hands a plain string back from a dataset with the datatype implied. No literal read from a dataset
could be classified `LiteralIdentifier`, so the construct-coverage audit reported the category unexercised
and failed the producer pass, and the by-reference key of such a string differed from the committed golden
vector.

One accessor now gives the expanded datatype of a literal and is the only place the codec reads it. The
classifier, the record order and the by-reference key and payload all use it. An identifier-shaped plain
string read from a dataset is written inline again. The by-reference key of a plain string read from a
dataset returns to the key derived from the explicit `xsd:string` datatype, the value the committed vector
pins; the key of a language-tagged string changes the same way, to the key derived from `rdf:langString`.
Emitted GMN-1 bytes change for these literals.

Verified: independent review PASS at 395ffe5e8, with both pinned golden keys recomputed; the whole diff read.
At the committed head on a clean tree the lang-bridge tests pass (251), clippy is clean and the source-input
scanner passes. The two tests for record order and the reference key fail on the previous codec. Pull
request checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the
aggregate red as on main.

Not shown: no producer pass, so that the coverage gap closes and the frozen-vector test passes are unproven;
that test and the wasm crates' native tests were not run. The prebuilt wasm images still embed the old codec
and are not refreshed here; the committed GMN image already disagrees with the committed witness on main.
Base direction of a by-reference literal is still lost on decode.
