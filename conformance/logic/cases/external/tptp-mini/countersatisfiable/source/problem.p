% SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
% SPDX-License-Identifier: CC-BY-4.0
%
% Non-entailment: a(x) does NOT entail b(x). The refutation stays
% satisfiable (a countermodel exists). SZS CounterSatisfiable.
fof(x_is_a, axiom, a(x)).
fof(goal, conjecture, b(x)).
% SZS status CounterSatisfiable for countersatisfiable
