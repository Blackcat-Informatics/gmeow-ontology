# Independent Card native JSON source review

Reviewed the complete thirteen-file inventory in validation.md, including new
external tests and CARD-JSON.md, against the original source diff. Reviewed the
final refrozen schema expressed as named Object locals and corrected docs ordering
prose. This reviewer did not implement the unit and ran no formatter, build, test,
corpus/producer/model operation, host probe, commit or post.

No blocking source finding identified.

- All five serializers preserve original member order and omission: category/iri
  always appear; present empty optional strings and nonempty vectors remain;
  nested panel records emit every field, including empty premise arrays.
- The four existing ordering policies remain distinct. Direct Card JSON retains
  declaration order; TOON receives recursively sorted objects before its original
  encoder; MCP sorts the full envelope and nested Card after measuring the original
  compact declaration-order payload; schema publication sorts recursively and adds
  exactly one newline. Native Value::sort_keys and Display implementations support
  the selected paths without a Serde bridge. Vector/required-array order is retained.
- Final schema source retains the original fields, required arrays, nested panel
  definitions, closed objects, descriptions and reference identities. The complete
  independent publication literal protects exact sorted pretty bytes plus LF;
  synthetic positive/negative instance controls exercise the actual schema owner.
- Actual callers are wired: both docs compact writers, shared public Card renderers,
  complete MCP core envelope and pipeline schema publisher. Focused reference search
  found no additional Card/schema Serde embedding left in this closure. MCP's
  direct native dependency and helper/tests are selected under core, preserving
  unrelated reasoning-only ownership and remaining Serde users.
- TOON's scalar quoting, terminal newline, array/table encoding and column selection
  remain unchanged; the native input shape contains only resolved strings and
  containers. Numeric-looking strings remain strings and use the existing quoting
  rule. No generic decoder or new numeric semantics were added.
- Thirteen authored controls use plain Card/DocTerm values, literal expectations,
  private actual adapters and local schema validation. They do not load a bundle,
  corpus, fixture cache or SourceCatalog, or invoke source compilation/production.
  Existing authenticated MCP/schema and rendered-site assertions remain separate.

Limits: this is source review, not executed qualification. Root owns the active
focused run, dependency resolution and strict checks. Authenticated bundled MCP,
full rendered artifacts, installed JSON/TOON, actual producer publication, excluded
consumer/wasm feature images and full-gate acceptance were not run here. Pure
adapter tests do not establish these broader outcomes. This source review grants
no additional waiver and does not establish broader acceptance.
