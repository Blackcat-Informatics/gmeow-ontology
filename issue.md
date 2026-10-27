# Issue #1985: Use native JSON for complete Common Logic round-trip observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

The Common Logic round-trip observation channel still uses Serde JSON for its complete stage-owned record and authenticated reader. Move this channel to PurRDF native JSON under Principles 4 and 7.

Acceptance:

- Migrate the complete Observation and DialectObservation records, the actual CompileLogicStage publisher, the authenticated corpus reader and authored component readers. Remove obsolete Serde ownership after checking consumers.
- Preserve source identity, the exact three-entry dialect array, entry order, open dialect strings, unit successes, both levels of recorded refusal and complete causal diagnostics.
- Require named records and declared fields; refuse duplicate known fields, positional records, malformed outcomes and arrays with the wrong width. Ignore unknown fields consistently with the current transport.
- Preserve the projection, canonical reference, parsing and complete IR comparison algorithms and all authenticated assertions.
- Reuse the existing three authored behavior controls and add only focused transport admission coverage. Compile the actual pipeline and developer CLI callers and run normal hooks.

Remaining contracts: #1776, #1336 and #1962. The full compiler IR migration, authenticated corpus execution, shipped assets, browser parity and installed acceptance remain with their existing owners. Current user instructions prohibit local optimized producer, corpus production, synchronization and full gate execution; report those limits explicitly.


## Comments (0)

