<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: AGPL-3.0-only
-->

# gmeow-math

The reusable exact-rational geometry layer. It owns the domain-neutral numeric
core that any grounding layer computes THROUGH rather than re-deriving:

- `Rational` — an `i128`-backed, gcd-normalized exact rational that hard-fails on
  overflow (no `f64`, no silent wrap).
- `InnerProductSpace` — a finite-dimensional inner-product space presented by a
  symmetric positive-definite Gram matrix `G`, with its `⟨x,y⟩ = xᵀGy` inner
  product, norm `√(xᵀGx)`, distance, cosine, projection, and the exact-rational
  LDLᵀ positive-definiteness certificate. The only approximation is the final
  square root, emitted as a fixed-precision decimal via an integer floor-sqrt.
- `load_gram` / `load_vector` — pure `math:`-vocabulary loaders that read exact
  rational Gram-matrix and coordinate-vector cells out of an RDF graph.

Consumers: `gmeow-affect` (the affect-intensity metric-tensor norm) and the
native `math:` conformance gates in `gmeow-validate`.
