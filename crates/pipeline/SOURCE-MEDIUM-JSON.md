<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Source-medium registry JSON

The source-medium observation uses native JSON records at its real publisher and
authenticated reader. The reader still requires the selected stage-conformance
artifact and the exact original `slices/core/gts/module.ttl` source path. Its
decoder restores typed produced state; it does not parse RDF, retrain dictionaries,
revalidate semantic registry constraints, or replace the registry's lookup rules.

Compact UTF-8 has no final newline. Declaration order remains:

* Source observation: `source_path`, `source_digest`, `registry`, `producer_media`.
* Registry: `dictionaries`, `dictionary_by_id`, `corpora`, `split`, `schemas`,
  `schema_by_rep`, `media`, `assignment`.
* Dictionary: `iri`, `id`, `version`, `strategy`, `target_length`, `corpus`.
* Corpus: `iri`, `selectors`.
* Split: `iri`, `stride`, `offset`.
* Schema: `iri`, `rep`.
* Medium: `iri`, `codec`, `zstd_level`, `source_kind`, `dictionaries`,
  `reader_capabilities`.
* Assignment: `schema`, `medium`, `dictionary`.

Maps and sets use their existing sorted order; selector vectors preserve order.
Every duplicate map value is admitted before its key's last value wins. Repeated
set elements collapse. Required record fields and collections do not acquire
defaults. The optional split writes explicit null; missing or null restores
absence. Known duplicate fields refuse and unknown record fields remain ignored.
Named records require objects; incidental positional arrays are not part of this
producer-owned transport. Malformed and trailing JSON refuse.

Dictionary strategy units are `Trained`, `RawContent`, `TermTable`; source-kind
units are `PerRep`, `HeaderDict`, `WholeArtifact`. Dictionary selection writes
`"Baseline"` or `{"Named":"dictionary IRI"}`. All unit variants also admit their
exact singleton-null-object form. Corpus selectors are exact singleton objects:
`BlobRep`, `Graph`, `PathPrefix`, `StageProduct`, each with a string payload.
Empty strings remain strings; the codec does not invent IRI or selector policies.
Unknown names, multiple or duplicate variant members and wrong payloads refuse.

Target length uses unsigned integer tokens checked against the host `usize`
width; split stride/offset use the u64 range. Signed zero, negative, fraction,
exponent and overflow tokens refuse on these unsigned fields. The signed i32
zstd level also refuses `-0`, fractions, exponents and overflow, retaining the
ordinary signed transport's token contract without imposing a compression grid.
Native record/map decoders retain field and nested-key error pointers.

The strict integer and unit-enum admission functions have shared native homes;
existing diagnostic-node unit enums and medium-sweep levels use those same
functions without changing their error wording. `MediumSelection` remains a
separate transport. RDF registry admission, source digests, builders, training,
emission and declared-medium selection remain with their existing owners.
