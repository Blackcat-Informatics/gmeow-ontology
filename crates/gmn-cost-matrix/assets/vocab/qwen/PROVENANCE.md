<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# Vendored Qwen tokenizer

* **File**: `tokenizer.json`
* **Model repo**: `Qwen/Qwen2.5-0.5B`
* **Source URL**: `https://huggingface.co/Qwen/Qwen2.5-0.5B/resolve/main/tokenizer.json`
* **License**: Apache-2.0 (ungated, redistributable, AGPL-compatible)
* **Size**: 7,031,645 bytes
* **blake3**: `b91332ca0c7a5f8e173effc53337026f64f17d4f25ca09205d4c1d5ecae4d621`

This is the only tokenizer vocabulary committed in-repo: its Apache-2.0 license permits
redistribution inside an AGPL-3.0-only tree. It is blake3-verified against the pin above on
load (`gmeow_gmn_cost_matrix::load_qwen`); a mismatch is a HARD FAIL. The two OpenAI
vocabularies are embedded via `tiktoken-rs`, and the restricted-license Llama / Gemma
vocabularies are fetched at maint-time and never committed — see `../PROVENANCE.md`.
