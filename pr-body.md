Term release-authority evidence now uses native JSON records throughout its actual reader and explicit maintainer writer. Remove the private PriorTerm/ReleaseAuthority Serde derives and this family's Serde JSON calls. Preserve the existing v1 schema, field order, compact output plus newline, omitted empty history and every release/domain/publication guard.

The old term-map decoder silently overwrote repeated term IRIs. Refuse that ambiguous ownership before map decoding using native duplicate detection. Duplicate declared fields also fail; unknown fields remain ignored, absent history defaults empty, and null history is invalid. Positional struct arrays are outside the authored format. Principles 4 and 7 apply.

Independent source review passed. Five focused component tests passed, covering canonical wire/history, actual-reader refusals, release-boundary and atomic no-op behavior, repeated manifest stability, and byte-identical term/changelog documentation. Scoped all-target strict checks for the pipeline and developer CLI passed with warnings treated as errors. Normal commit hooks passed. Inputs are authored bytes or the existing tiny synthetic datasets. No tracked authority refresh, local optimized producer or corpus generation is performed. Hosted producer-build, helpers and lint pass on the final source; prebuild and cold-generation lanes are pending. Automated review found no production code issue; its generic documentation percentage was declined for private trait implementations and test fixtures. Final test-only review is rate-limited; independent review and focused validation provide its evidence.

This is a complete bounded codec family under #1776. Full parent, authenticated corpus and installed acceptance remain open.

Closes #1969.
