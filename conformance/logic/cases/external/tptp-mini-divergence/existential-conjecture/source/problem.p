% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% An honest capability gap: p(c) ⊢ ?[X]: p(X) is a genuine first-order
% theorem, but the native EL/DL fragment cannot refute an EXISTENTIAL
% conjecture (its negation ∀X. ¬p(X) is a universal negative constraint the
% projection does not carry). The lowerer must report a DlGap here, NOT a
% wrong `consistent` and NOT a silent `incomplete`. SZS Theorem.
fof(p_of_c, axiom, p(c)).
fof(goal, conjecture, ?[X] : p(X)).
% SZS status Theorem for existential-conjecture
