# Crossref XSD fixtures

This directory contains third-party Crossref XML Schema Definition (XSD) files
used by `tests/test_crossref.py` to validate the generated Crossref DOI deposit
XML **offline**, without making network requests during the default test run.

## Files

- `crossref5.4.0.xsd` — main Crossref deposit schema (version 5.4.0)
- `common5.4.0.xsd` — common types included by the main schema
- `languages5.4.0.xsd` — language code enumerations
- `mediatypes5.4.0.xsd` — media-type enumerations
- `AccessIndicators.xsd` — Crossref AccessIndicators program schema
- `relations.xsd` — Crossref relations program schema
- `fundref.xsd` — Crossref FundRef program schema
- `fundingdata5.4.0.xsd` — Crossref funding data schema
- `clinicaltrials.xsd` — Crossref clinical-trials schema
- `JATS-journalpublishing1-3d2-mathml3.xsd` — minimal local stub for the JATS
  namespace, replacing the full JATS publishing schema tree because GMEOW only
  references the optional `jats:abstract` element.
- `mathml-stub.xsd` — minimal local stub for the MathML namespace, mapped over
  the remote MathML 3.0 schema so the test suite stays offline.
- `xml-stub.xsd` — minimal local stub for the `xml:` namespace attributes,
  mapped over the remote `xml.xsd` so the test suite stays offline.

## Source

The Crossref XSD files were downloaded from:

- <https://www.crossref.org/schemas/crossref5.4.0.xsd>
- <https://www.crossref.org/schemas/common5.4.0.xsd>
- <https://www.crossref.org/schemas/languages5.4.0.xsd>
- <https://www.crossref.org/schemas/mediatypes5.4.0.xsd>
- <https://www.crossref.org/schemas/AccessIndicators.xsd>
- <https://www.crossref.org/schemas/relations.xsd>
- <https://www.crossref.org/schemas/fundref.xsd>
- <https://www.crossref.org/schemas/fundingdata5.4.0.xsd>
- <https://www.crossref.org/schemas/clinicaltrials.xsd>

These schemas are copyrighted by Crossref and are used here under their
publicly published terms for validation and testing purposes only.
