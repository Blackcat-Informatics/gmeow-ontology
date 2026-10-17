# Producer sharing candidate

Read .github/workflows/ci.yml producer-build job and pages.yml build job, plus
Makefile maint-mdbook-smoke. CI uploads gmeow-dev-producer-${github.sha} containing
the optimized binary and its receipt. Pages independently invokes
maint-mdbook-smoke -> check-sync, which establishes a producer. Pages currently
restores Cargo dependencies but does not consume that exact CI artifact.

This is evidence of distinct producer paths, not a measured duplication cost or
a speedup claim. A repair must preserve exact source/recipe authentication,
trusted workflow provenance, branch/ref authorization, docs prerequisites and
publication behavior. Consider artifact handoff or shared producer caching only
after comparing those contracts. No workflow changes made in this worktree.
