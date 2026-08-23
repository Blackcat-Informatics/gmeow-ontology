<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-affect

Rust-owned affect-intensity geometry.

This crate is the authority for computing overall affect intensity as the norm
`√(xᵀGx)` over a **non-orthogonal, positive-definite** metric Gram matrix `G`
(never a raw L² norm), read from an RDF graph and computed **outside** the
reasoned core (Principle 12). It exposes a reusable, exact-rational
inner-product-space over `G` (inner product, norm, distance, cosine, angle,
orthogonality, projection, and an LDLᵀ positive-definiteness certificate) plus
the graph-reading front door consumed by the `gmeow affect` CLI and the
EmotionML emitter.

All arithmetic is exact rational (`i128`, gcd-normalized, hard-fail on
overflow). The single approximation is the final square root, produced as a
deterministic fixed-precision decimal (`k = 6` fractional digits, round-half-up
at the seventh digit) via an integer floor-sqrt — never `f64::sqrt`.
