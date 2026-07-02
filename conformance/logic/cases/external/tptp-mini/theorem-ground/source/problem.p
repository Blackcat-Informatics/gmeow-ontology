% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Ground entailment: a⊑b, a(x) ⊢ b(x). The refutation premises ∧ ¬b(x) is
% unsatisfiable. SZS Theorem.
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(x_is_a, axiom, a(x)).
fof(goal, conjecture, b(x)).
% SZS status Theorem for theorem-ground
