<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# GMN token-cost-matrix tokenizer provenance

The `maint-gmn-cost-matrix` lane runs five tokenizer families over the emitted GMN /
Turtle / JSON-LD serializations of the grounding corpus. Each family's vocabulary is
obtained by one of three license-appropriate mechanisms. Every vocabulary is
**digest-pinned** (blake3) so a corrupted or swapped asset is a HARD FAIL, never a
silently-different tokenization.

## 1. Embedded (compiled into the binary) — no vendoring

| Family | Source | License | Digest |
|---|---|---|---|
| `o200k_base` | `tiktoken-rs` `assets/o200k_base.tiktoken` (`include_str!`) | MIT | compiled-in (tiktoken-rs) |
| `cl100k_base` | `tiktoken-rs` `assets/cl100k_base.tiktoken` (`include_str!`) | MIT | compiled-in (tiktoken-rs) |

These are the two OpenAI vocabularies already used by the on-gate glyph-cost primitive in
`crates/lang-bridge`. No network, no filesystem read — deterministic and reproducible.

## 2. Vendored in-repo (blake3-pinned) — permissive license only

| Family | Source repo | Commit source | License | Size (bytes) | blake3 |
|---|---|---|---|---|---|
| Qwen | `Qwen/Qwen2.5-0.5B` | `resolve/main/tokenizer.json` | **Apache-2.0** (ungated) | 7,031,645 | `b91332ca0c7a5f8e173effc53337026f64f17d4f25ca09205d4c1d5ecae4d621` |

Vendored at `qwen/tokenizer.json`; verified against the pin on load (`load_qwen`). Apache-2.0
is redistributable and AGPL-compatible, so this asset is committed in-repo. See
`qwen/PROVENANCE.md`.

## 3. Fetched at maint-time (NEVER vendored) — restricted, AGPL-incompatible licenses

| Family | Original model | Default ungated fetch source | License | Size (bytes) | blake3 |
|---|---|---|---|---|---|
| Llama | `meta-llama/Meta-Llama-3-8B` | `NousResearch/Meta-Llama-3-8B` `resolve/main/tokenizer.json` | **Meta Llama 3 Community License** | 9,085,698 | `174e70b51765e4514178cbae91eb5e54975cfdf3946427a75b8cba4954de898e` |
| Gemma | `google/gemma-2-2b` | `unsloth/gemma-2-2b` `resolve/main/tokenizer.json` | **Gemma Terms of Use** | 17,525,357 | `7e8d9bfc505e187f92921e574a36991eef018ded6744adc658d2d343d6de1010` |

### License rationale — why these are fetched, not vendored

This repository is `AGPL-3.0-only`. The **Meta Llama 3 Community License** and the **Gemma
Terms of Use** are restricted, non-free licenses with field-of-use / acceptable-use
restrictions (and, for Llama, a >700M-MAU clause). Committing those tokenizer assets into
an AGPL-3.0-only tree would add redistribution restrictions AGPL forbids — a real license
conflict. So, exactly like the repo's other restricted-license Lane-B corpora
(`maint-tptp-corpus` "per-problem licensed, NEVER vendored"; `maint-ontouml-corpus`
"CC BY-SA — NEVER vendored"), the `maint-gmn-cost-matrix` lane **fetches** each asset over
the network into the git-ignored `.tmp/` at maint-time, blake3-verifies it against the pin
above, uses it in-process, and never commits it.

### Fetch sources and gating

* **Llama**: the canonical `meta-llama/Meta-Llama-3-8B` repo is HuggingFace-gated
  (`gated: manual`). The default fetch source is the ungated `NousResearch/Meta-Llama-3-8B`,
  a faithful re-host of Meta's exact Llama-3 tokenizer (identical bytes ⇒ identical pin).
* **Gemma**: the canonical `google/gemma-2-2b` repo is HuggingFace-gated (`gated: manual`,
  HTTP 401 without an accepted-license token). The default fetch source is the ungated
  `unsloth/gemma-2-2b`, a faithful re-host of Google's exact Gemma-2 tokenizer. To fetch
  from the canonical gated Google repo instead, accept the Gemma license on HuggingFace,
  export `HF_TOKEN`, and override `GEMMA_TOKENIZER_URL` (the fetched bytes must still match
  the pinned blake3, or the lane HARD-FAILS).

The fetch URLs and the `HF_TOKEN` handling are Make variables on the `maint-gmn-cost-matrix`
target; overriding them to any authorized source is supported as long as the fetched bytes
match the committed pin.
