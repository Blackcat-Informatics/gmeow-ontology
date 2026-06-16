<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

## Summary

- what changed
- why it changed

## Validation

- [ ] `make lint` (ruff + mypy)
- [ ] `make validate`
- [ ] `make reason` (if the ontology changed)
- [ ] `make mappings` / `make wikidata` (if mappings changed)
- [ ] `uv run pytest`
- [ ] docs updated if behaviour, flags, terms, or outputs changed

## Checklist

- [ ] scope is intentional and limited to this change
- [ ] tests updated when behaviour changed
- [ ] new/changed GMEOW terms have an rdfs:label, skos:definition, and gUFO grounding
- [ ] no reference-only (NC/ND/share-alike/copyleft) axioms copied into the vocabulary
- [ ] no secrets or local-only paths introduced
- [ ] CLA Assistant check completed or not required for this contributor

## Notes

Add any reviewer context, follow-ups, or caveats here.
