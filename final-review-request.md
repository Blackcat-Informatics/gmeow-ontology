@coderabbitai review

Please recheck final head 271463204, especially the changes since your reviewed
7db11af84: zero-count worlds are explicitly marked unselectable, consumer boundary
doc comments are added, and the missed fuzz manifest/lock now use exactly the
same PurRDF source as production. All 31 native package identities match.

The native dependency is now merged through purrdf PR 567. The default shortcut
and retired named-only API are removed, with no alias or fallback. Please verify
the selected-world callers and read-only receipt preflight remain wired through
the intended production assembler/native cache and graph projection.

Actual recorded execution now covers all nine worlds: eight named worlds each
replay one proof (four Decided, four DecidedWithinBoundaries), while corrected
default selection has 179177 assertions and native preparation refuses an
invalid property. No canonical consistency or complete class-engine replacement
is claimed. The full issue 1803 remains open; this PR is the graph/consumer
integration. Full hosted qualification is running and has not passed yet.

Please identify any remaining functional or admission gap in this PR's actual
implementation and evidence rather than treating the open broader engine move
as completed. Principles 4 and 7.
