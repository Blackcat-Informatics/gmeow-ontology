% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Subclass entailment: a⊑b, b⊑c ⊢ a⊑c. Refuted by a fresh witness
% w∈a ⇒ w∈b ⇒ w∈c, clashing with w∈c̄. SZS Theorem.
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(b_sub_c, axiom, ![X] : (b(X) => c(X))).
fof(goal, conjecture, ![X] : (a(X) => c(X))).
% SZS status Theorem for theorem-subclass
