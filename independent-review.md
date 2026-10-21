# Independent source review: i18n diagnostic focus JSON

Disposition: no actionable finding in the two-file migration. Source-only review; no checks, builds, corpus reads, producers, or host probes were run by this reviewer.

## Requirement and actual callers

`I18nLintReport::observe` now uses the pinned native `ToJson` tuple codec and compact writer directly. Its selected input is only `(rule: &str, subjects: &[&str])`: the required output remains `[rule,[subjects...]]`, retaining empty strings, empty lists, Unicode, subject order, and boundaries. No numeric, object-key, or parser admission behavior is involved. No fallback, replacement error, or speculative compatibility layer was introduced.

The surrounding `CatalogInconsistent` diagnostic, warning/error grade selection, message, `at_source` location, stage attachment, and ledger projection are unchanged. `lint_po_files` continues to feed this observation path; the actual `gmeow-dev i18n lint` caller still emits the projected ledger and checks its existing verdict. Other Serde owners and manifests remain untouched.

## Owner evidence

Pinned native revision `017f552`, `crates/lex/src/json/record.rs`: tuple `ToJson` at 1417–1430 constructs an array in tuple order; slice codec at 1363–1366 maps items in iteration order; reference and string codecs preserve text. `json/write.rs` routes `write_compact` through `Format::COMPACT`, whose string policy is `ShortForms`. `json_escape.rs::try_escape_body` preserves ordinary UTF-8 and escapes JSON controls/quotes/backslashes. This matches the selected original compact Serde string/array representation; no sorting or extra newline is introduced.

## Coverage reviewed

- `lint_native_json_focus_preserves_tuple_bytes`: independent literal focus bytes and retained source path.
- `lint_native_json_focus_preserves_string_escapes_and_empty_subjects`: empty tuple components, quotes, backslash, short/control escapes, Unicode and literal U+2028/U+2029.
- `lint_native_json_focus_keeps_rule_and_subject_boundaries_distinct`: distinct rule/list delimiters, list ordering, and trailing empty subject survive ledger identity construction.
- Unchanged `lint_identity_excludes_wording_but_keeps_context_and_warning_grade`: wording differences coalesce without losing messages; differing subject context remains distinct and warnings stay collected.

These controls construct report observations directly. They do not invoke catalog discovery, authored-source loading, corpus construction, or the CLI. Runtime success, required source-policy checks and package qualification belong to the root's separate serial checks; none is inferred from this review. End-to-end repository i18n lint and produced/installed acceptance remain unrun here.
