% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% The disjointness clash authored in CNF (exercises the clause path):
% ¬a∨b, ¬a∨c, ¬b∨¬c, a(x). SZS Unsatisfiable.
cnf(a_sub_b, axiom, ( ~a(X) | b(X) )).
cnf(a_sub_c, axiom, ( ~a(X) | c(X) )).
cnf(b_disj_c, axiom, ( ~b(X) | ~c(X) )).
cnf(x_is_a, axiom, a(x)).
% SZS status Unsatisfiable for cnf-disjoint-clash
