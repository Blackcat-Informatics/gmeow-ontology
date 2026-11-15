docs: correct false statements in the agent guide (Principles 4, 7)

The consolidated agent guide reached main with statements a review found false. This corrects them and takes
the review's smaller findings. Authoring `owl:` in a module fails the source lint; `rdfs:` does not, and the
guide now says which `rdfs:` forms the projection-vocabulary ratchet counts. Dependency versions are declared
once in the root manifest for new dependencies, with older per-crate pins moved when touched. Builds are
serialized by a session on its own lock, and the gate takes a separate host-wide lock. Working files live
under the main checkout's `.stage/`. The layout table, the logging example, the slice sugar forms, the
ratchet's guarded vocabularies and relocation rule, and the test-profile rule are stated as they are.

Three rules are changed, by the maintainer's decision on this pull request: the sentence that work held more
than twelve hours is reported is removed; the limit of three open items applies to a contributor's or
session's own; and the ban on rebasing, amending or force-pushing applies to a pushed branch.

Verified: independent review PASS at 9154edf9d; the whole diff read; only `AGENTS.md` changes. Pull request
checks: helpers, lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red
as on main.
