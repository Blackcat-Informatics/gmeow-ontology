# Issue #1776: deps: replace serde/serde_json with purrdf_lex::json (PurRDF owns JSON)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, dependencies, rust

## Body

## Problem

PurRDF 3.0 removed serde from every public type and made `purrdf_lex::json` (with `ToJson`/`FromJson` records and `json_record!`) the substrate's one RFC 8259 JSON home. GMEOW still carries its own JSON stack on `serde`/`serde_json`: ~1,370 `serde_json::` uses across ~340 files and ~210 files deriving `Serialize`/`Deserialize`. Under the "where GMEOW and PurRDF conflict in functionality, PurRDF wins" doctrine adopted with the 3.0 upgrade, this is a second JSON implementation.

The 3.0 upgrade crossed the boundary only where PurRDF types meet GMEOW serde (e.g. `docs-model`'s `purrdf_json` serde adapters for `ArtifactRole`/`SliceTier`, YAML instances crossing to `serde_json` through text in `validate::instance`, the research-object `OfflineJsonLdContext`).

## Scope

- Replace `serde_json::Value` and serde derives with `purrdf_lex::json::Value` and `ToJson`/`FromJson` (`json_record!`) crate by crate, in dependency order.
- Retire the boundary shims added for 3.0 (`docs-model` `purrdf_json`, JSON-text crossings in `validate::instance` and the YAML tests).
- Remove `serde`/`serde_json` from the workspace graph when the last use goes.

## Acceptance

- No `serde`/`serde_json` in any GMEOW manifest; `cargo tree -i serde_json` is empty for workspace members.
- Every committed JSON artifact and golden is byte-identical, or each difference is traced to a documented `purrdf_lex::json` rule (lexeme-preserving numbers, member order).
- `make check` passes.


## Comments (0)

