<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native catalog-family observations

The mappings producer writes `pipeline/catalog-family-observations.json` through
the native JSON record owner. `authenticated_registry` first authenticates that
selected mappings artifact, then decodes those exact bytes through the same
native record model. Missing/stale fixture selection and corrupt artifact bytes
remain failures; the reader never falls back to the authored registry document.

The compact UTF-8 object has no trailing LF. Its four record orders are:

| Record | Members in order |
| --- | --- |
| CatalogFamily | `iri`, `name`, `namespaces`, `owners`, `minimum` |
| ResidueExemption | `iri`, `family_iri`, `rationale`, `row_ceiling` |
| CatalogRegistry | `families`, `exemptions` |
| RegistryObservations | `registry`, `guarded_namespaces` |

All fields are required and remain present, including empty strings, empty
collections and zero counts. Vectors retain their order and repetitions. The
guarded namespace set emits in lexical order and deduplicates repeated input
items. Missing/null/wrong-type fields and known duplicate members refuse;
unknown record members remain ignored. Both count fields require native unsigned
integer tokens within the host's usize width: `-0`, negatives, fractions,
exponents and overflow refuse. Records admit named objects; positional struct
arrays are not an alternative transport.

These codecs preserve the existing transport admission boundary rather than
inventing registry policy. Source loading and the mappings gate continue to
enforce family identity, namespace/owner requirements, floors, overlap and
bounded exemption rules before publication. JSON decoding does not normalize
vectors or add those source-policy checks. Authenticated decode refusals retain
the native JSON pointer and typed cause through the existing `RecordedInput`
diagnostic, with the operation and refusal in its primary message.

The artifact name, authentication, source selection, receipt and product
commitments are unchanged. Other pipeline JSON/Serde owners remain separate.
