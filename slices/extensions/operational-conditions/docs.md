<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Operational conditions

The Memetics Body importer uses this slice to retain the observations
and acknowledgements in the Decodes condition ledger. Each observation
records where a machinery organ reported a condition. An acknowledgement
identifies the condition by its stored kind and key across reporting
sites and retains the author field supplied by the ledger. Actual
remediation and its evidence belong to separate records.

Both records carry one source timestamp as `gmeow:recordedAt` with an
`xsd:dateTime` value, one kind code as `xsd:token`, and one exact key.
They can also carry an occurrence identifier. Encode the UTF-8 bytes of
the key, occurrence, organ, site, tool call identifier, custody kind,
custody handle and acknowledgement author as `xsd:hexBinary`. This
preserves whitespace and separators in source identifiers. Historical
kind codes remain valid as tokens.

An observation carries one reporting organ, one site and one
`gmeow:bodyConditionEvidenceDigest`. The digest contains the thirty-two
original BLAKE3 bytes as sixty-four hexadecimal digits. A supplied
packet identifier uses `gmeow:originatingPacket` as an `xsd:token`.
The tool call identifier is optional. Custody carries both the kind
and handle or neither.

Load the generated class and procedural shapes together. The class
shapes check field types and cardinality. The procedural shapes admit
only the declared fields and `rdf:type`, check both custody fields,
and enforce the evidence digest width. Record facts can carry source
attribution on separate RDF 1.2 statement reifiers.

In the worked ledger, intake and outlet reports share a stored kind
and the key whose decoded value has a leading space, the text
`a:b`, and a trailing space. The acknowledgement matches both reports.
[Worked ledger](examples/condition-ledger.ttl)
