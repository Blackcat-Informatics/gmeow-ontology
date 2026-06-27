% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Self-authored TPTP-style problem (#753 szs-mini). A disjoint-class clash: an
% individual x is typed A, A is subsumed by both B and C, and B is disjoint from
% C. The axiom set has no model.
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(a_sub_c, axiom, ![X] : (a(X) => c(X))).
fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).
fof(x_is_a, axiom, a(x)).
% SZS status Unsatisfiable for unsat-clash
