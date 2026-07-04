# Retention: `tests/test_notation.py`

**Category:** Static repo guard

## What it tests

Retained cross-slice and dynamic guards for the notation and symbolic systems building block.

Retained dynamic tests:

- `test_value_vocabularies_not_subclasses` — No unexpected subclasses of SymbolicSystemKind or NotationUsageRole.
- `test_ambiguous_cases_co_modelable` — Ambiguous systems (MusicXML, MathML, MIDI, ABC) can be co-modeled as both FormalLanguage and NotationSystem through standpoint-indexed claims.

## Why it cannot be deleted or moved to Rust today

Retained cross-slice and dynamic guards for the notation and symbolic systems building block.
