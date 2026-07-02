% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Axioms alone are contradictory: a⊑b, a⊑c, b⊥c, a(x) forces x into
% disjoint b and c. SZS ContradictoryAxioms (the axiom set has no model).
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(a_sub_c, axiom, ![X] : (a(X) => c(X))).
fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).
fof(x_is_a, axiom, a(x)).
% SZS status ContradictoryAxioms for contradictory-axioms
