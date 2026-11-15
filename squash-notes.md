docs: state the PurRDF version the lockfile selects (Principles 4, 7)

`docs/research-objects.md` named "purrdf 3.0.0" while `Cargo.lock`, `fuzz/Cargo.lock` and every shipped
substrate stamp say 4.0.0. The substrate stage reads the first `purrdf <version>` in that document as a
version claim, and one disagreeing site flags every claim site in its group, which is the cause of the nine
pin-agreement findings in the recorded producer pass. The document now states the version the lockfile
selects. The version stays in the prose because the prose site is one the constraint expects a claim from.

Verified: the one-line diff read; the lockfiles read for the version. Pull request checks: helpers, lint,
producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on main.

Not shown: no producer pass, so that the nine findings clear is unproven.
