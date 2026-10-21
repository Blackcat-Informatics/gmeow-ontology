# Primer observation JSON source unit

The existing primitive-reader plan and validation remain unchanged. This sequential
extension owns only the five primer/teachability record codecs, their direct
observation writer, two authenticated readers, external pure tests and wire docs.

Preserve declaration-order compact bytes, three Pascal external variant tags,
vector order, sorted/deduplicated string sets, explicit nullable ast_error,
strict usize counts, ignored unknown record fields and known-duplicate refusal.
Use native Record/FromJson/ToJson; no Serde bridge or generic compatibility layer.
Native object records intentionally refuse incidental positional Serde forms.
Archive/authentication, selector binding, artifact names and corpus assertions
remain untouched. Rendering and graph construction remain untouched.

New tests construct private plain rows/tasks directly and call only codecs and
pure accessors. Filter: package(gmeow-docs-model) and test(json_gmn_primer_).
No producer, ontology, dictionary, corpus or fixture construction occurs in tests.
Root owns formatting, builds, checks, lock resolution and publication. All
execution acceptance is unrun by this lane, including actual authenticated
producer/docs/bundle consumers. No manifest removal is justified by this family.
