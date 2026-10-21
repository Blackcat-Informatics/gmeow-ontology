# Independent documentation search-index JSON review

Reviewed the four source/doc/manifest files, including the new external pure
tests and SEARCH-JSON.md, plus Stage plan and validation. This reviewer did not
implement the unit and ran no formatter, build, test, producer, corpus operation,
host probe or posting.

No blocking source finding identified.

- SearchRecord remains private and the public search_index_json return type is
  String. Both public-model and site-renderer paths reach the same actual adapter.
  The original URL/kind/ID stable comparison was moved unchanged.
- Native Object insertion order preserves kind/id/label/definition/url followed
  by populated facets in their original order. Option::None produces explicit
  null. Omission tests Vec::is_empty, so a vector containing an empty string is
  retained. Native write_pretty supplies two-space Unicode/escaping behavior and
  no terminal newline without a Serde conversion or local writer.
- Model-to-record assembly, source-to-page map, child document URLs and full
  child prose are unchanged. The source-map build consumes supplied model values;
  the new public-model tests do not discover or produce a source corpus.
- Card, NavItem, minijinja and the remaining Serde/serde_json dependency are
  retained. The documentation now includes the already-existing document kind;
  no new kind or runtime admission policy is introduced.
- Eight independently authored controls cover literal wire output, populated
  facet/escape behavior, empty vector versus empty member semantics, sorting,
  empty public model, concern output, child-document behavior and term facets.
  They exercise the actual private/public output paths without corpus production.

Limits: source inspection does not establish compilation or executed test passes.
Root owns formatting and focused qualification. Existing authenticated real-model
goldens, full rendered-site/search artifact parity, hosted/full gates and installed
acceptance were not executed here; no golden was refreshed or acceptance weakened.
