# Card-family native JSON

Migrate the five output-only Card records, their direct JSON/TOON/schema writer,
both docs compact adapters, the complete MCP doc_card envelope, and pipeline card
schema publication to the native JSON owner. No speculative decoder is needed:
the records currently derive only Serialize. Preserve required category/iri,
Some("") versus None, omission of only empty vectors, vector order and tier
projection. Keep minijinja NavItem and all unrelated JSON/receipt owners.

Four ordering profiles are distinct: direct Card compact/pretty declaration order;
TOON recursively sorted keys; MCP sorted envelope/nested Card while cost metadata
uses compact declaration-order payload; schema sorted pretty bytes plus one LF.
Native Object/ToJson/sort_keys/write_compact/write_pretty cover these policies.
The schema remains its existing authored document, rebuilt with native objects;
there is no new schema derive/field bitmap or native-to-Serde compatibility path.

Audit confirms no Card in cache receipts or template contexts. Docs CachedSite,
other docs-model records and other MCP/pipeline JSON remain Serde-dependent.
Examples use unrelated map/diagnostic JSON; their dependencies stay. Add the
actual native MCP dependency under its selected core feature.

Add external pure json_card_ tests for independent record/model bytes, tier and
omission distinctions, TOON ordering, complete MCP envelope/cost metadata and
actual schema adapter validation. Preserve existing authenticated consumer tests
by porting only their writer/schema boundaries. No test may construct a corpus,
invoke fixture/source discovery or run producers. Root owns formatting, builds,
tests, lock resolution and publication. Current source at base32f268c26 confirms
the prior assessment; Graphify remains unavailable, narrow source fallback used.
