<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native catalog-family observations

The mappings producer writes `pipeline/catalog-family-observations.json` through
the native JSON record owner. `authenticated_registry` first authenticates that
selected mappings artifact, then decodes those exact bytes through the same
native record model and validates governance and native proof coverage. Missing
or stale fixture selection and corrupt artifact bytes
remain failures; the reader never falls back to the authored registry document.

The compact UTF-8 object has no trailing LF. Its record orders are:

| Record | Members in order |
| --- | --- |
| CatalogFamily | `iri`, `name`, `namespaces`, `owners`, `minimum` |
| ResidueExemption | `iri`, `family_iri`, `rationale`, `row_ceiling` |
| CatalogRegistry | `families`, `exemptions`, `native_kernel_families` |
| RegistryObservations | `registry`, `guarded_namespaces`, `native_kernel_evidence` |
| NativeKernelEvidence | `family`, `correspondence`, `semantic_key`, `source`, `target`, `source_owner`, `source_role`, `source_types`, `source_path`, `source_digest`, `artifact_owner`, `artifact_path`, `artifact_digest` |

All fields are required and remain present, including empty strings, empty
collections and zero counts. Vectors retain their order and repetitions. The
guarded namespace and native-family sets emit in lexical order and deduplicate
repeated input items. Missing, null or wrong-type fields and known duplicate members refuse;
unknown record members remain ignored. Both count fields require native unsigned
integer tokens within the host's usize width: `-0`, negatives, fractions,
exponents and overflow refuse. Records admit named objects; positional struct
arrays are not an alternative transport.

The required native-family and evidence fields deliberately change the wire
contract: older observations without them refuse. Structural decoding retains
the native JSON pointer and typed cause through the existing `RecordedInput`
diagnostic. Authenticated admission additionally requires exactly one governance
regime per family, valid owner-to-declaration and owner-to-mapping paths, complete
SHA-256 identities, consistent semantic proofs and sufficient distinct native
correspondences for every selected family. Empty evidence cannot satisfy a
nonzero native-family minimum. Source loading still performs the actual
per-occurrence native frontend admission before deduplication; the authenticated
reader never replaces it by reopening authored sources. Declaration authority is
explicitly `module` or `example`; examples retain their actual Example-role source
and declared native types without claiming Module ownership. Missing role/type
fields, unknown roles and empty Example types refuse. An invalid Module declaration
cannot fall through to Example authority. Shipped-program acceptance compares
Module owners or Example type ownership and complete non-law correspondence keys
with those proofs. Law discharges remain separately certified.

The artifact name and authentication mechanism are retained. New source and
manifest dependencies participate in the producer's actual receipt and product
commitments. Other pipeline JSON and Serde owners remain separate.
