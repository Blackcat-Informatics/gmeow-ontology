# gmeow-ns

The single declaration site for the namespaces GMEOW mints ontology terms into.

GMEOW mints into **four** namespaces — `gmeow:`, `logic:`, `lang:`, and `math:` —
but purrdf's slice framework is namespace-neutral and takes the set from the
consumer. A namespace GMEOW mints into but never declares is *invisible* to
purrdf's ownership analyzer: `rdfs:isDefinedBy` claims and typed vocabulary terms
on such a subject are dropped, every reference to those terms resolves to no
owning slice, and no dependency edge is ever computed. The failure is silent —
the analysis still produces plausible output for every other slice.

This crate therefore owns:

- [`GMEOW_NS`] / [`LOGIC_NS`] / [`LANG_NS`] / [`MATH_NS`] and the
  [`TERM_NAMESPACES`] set they form;
- [`gmeow_profile`] — the one `purrdf::OntologyProfile` every emitter derives its
  vocab view from;
- [`gmeow_slice_vocab`] — the one `purrdf::SliceVocab`, carrying all four
  namespaces as owned term namespaces, that every `SliceCatalog::discover` call
  in the workspace passes;
- [`gmeow_json_schema_namespaces`] — the SHACL→JSON-Schema keying view.

It depends on `purrdf` alone. That is deliberate: `gmeow-validate`,
`gmeow-docs`, `gmeow-slice-brief`, and `gmeow-pipeline` sit at four different
heights in the crate layering, and a shared constructor is only genuinely shared
if it sits below all of them.
