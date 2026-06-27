% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Self-authored TPTP-style problem (#753 szs-mini). x is typed A and A is subsumed
% by B; no disjointness. The axiom set has a model.
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(x_is_a, axiom, a(x)).
% SZS status Satisfiable for satisfiable-open
