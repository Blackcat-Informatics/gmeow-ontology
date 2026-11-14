ci: repair the named steps of the scheduled fuzz, coverage, heavy and benchmark jobs (Principles 4, 7)

The fuzz workspace's lock is brought back in line with the workspace, and a new `fuzz-lock-check` target runs
the locked resolution in the `helpers` job, so a stale fuzz lock is caught on the pull request path. The
coverage and heavy jobs install ripgrep the way the main workflow does, and the corpus scanner now names a
missing tool instead of exiting with a bare status. The benchmark job installs Valgrind and the
instruction-count runner at the version the lock file names.

Verified: independent review PASS at 4d376c7c1; `make fuzz-substrate-check`, `make test-corpus-scanner` and
`make lint` exit 0 locally; dispatched runs of both workflows pass every repaired step. None of the four
scheduled jobs is green yet: each fails at its next step, filed separately. Pull request checks: helpers,
lint, producer-build and rust-prebuild green; the three producer jobs and the aggregate red as on main.

Closes #2783.
