<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Pinned textual regex datatype

`gmeow:linearRegex` is a lang-owned literal datatype for a declared external
textual syntax, reusable by downstream applications without moving grounding
ownership. The version-one contract fixes Rust regex 1.13.1 with regex-syntax
0.8.11, string-regex defaults except explicit inline flags, Unicode 16.0.0
property/simple-fold tables, and leftmost-first non-overlapping matches against
original Unicode text. A pattern permitting any zero-width match is malformed.

The locally inspected regex-syntax general-category and simple-fold files
declare Unicode 16.0.0. The fold relation uses C/S pairs, not full F expansions;
comparison by simple folding is independent of locale. A caller must not
substitute a host Unicode release, byte-regex dialect or SPARQL/SHACL pattern
semantics while retaining this datatype.

Regex's individual-search bound is O(m*n), linear in input length n for fixed
pattern size m. Its leftmost-first iteration has an O(m*n*n) worst case. The
datatype name promises no stronger total-enumeration bound. Checking only an
empty input does not prove a pattern cannot match zero-width in a nonempty
context.

This declaration adds neither lexical validation machinery nor a matcher.
Canonical constraints can require the datatype of supplied values, but that is
not proof of lexical membership, nonempty-match capability, runtime complexity
or actual search. A selected operation requiring such checks must provide them
and fail explicitly when they are unavailable. No placeholder formula using
`logic:termRegex` claims to implement this dialect.

The cue application's phrase comparison and sentence scope are separate
contracts: its word categories/simple-fold pairs use Unicode 16.0.0; its default
UAX 29 sentence segmentation uses Unicode 17.0.0, matching the inspected
unicode-segmentation 1.13.3 table constant. Phrase folding never silently alters
regex input. Both preserve original literal identity and recover code-point
offsets against the original text.

External authorities and inspected software sources are recorded in the canonical
[reference ledger](../../../../metadata/references.ttl). They are by-reference
syntax/algorithm sources, not equivalent ontology classes or evidence that a
runtime operation was executed. The downstream [cue design](../../../core/refinement-cues/design/REFINEMENT-CUES.md)
consumes this contract without authoring a second external correspondence.
