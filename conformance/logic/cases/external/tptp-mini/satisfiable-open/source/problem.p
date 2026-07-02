% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% An open, clash-free ontology: a⊑b, a(x). A model exists.
fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).
fof(x_is_a, axiom, a(x)).
% SZS status Satisfiable for satisfiable-open
