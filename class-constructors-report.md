# Partition and local self laws

Extended the existing source-bound class-law path, without a second parser or
runtime dependency. Disjoint unions retain the membership cover plus every
pairwise overlap exclusion. Duplicate source members are a set, so they do not
create self-disjointness. All original list statements remain selected support,
and dependencies use the existing refusal propagation.

Local self restrictions now pass through the shared native restriction compiler
and define membership by p(x,x). Native XSD boolean interpretation admits true/1;
false, untyped, invalid and scoped flags remain refused. This is local class
membership, not a global reflexivity assertion.

The first focused run passed 68/69. Its one failure was an incorrect new positive
test expectation for whitespace-padded true. PurRDF's Boolean lexical parser and
the existing native reasoner both reject that lexical. RDF 1.2 Concepts defines
the Boolean lexical space as true/false/1/0:
https://www.w3.org/TR/rdf12-concepts/#section-Datatypes
Moved the whitespace case to the negative product checks and added uppercase
refusal; no parser, assertion, gate or admitted lexical space was weakened.

The corrected source passed 69/69 compiler TPTP/list/restriction tests; nextest
85045e6f-74cf-4848-bae9-69b4e3017cda, 38.09s build, 0.032s execution. The tests
inspect actual source admission, native compiler retention, exact class bindings,
cover membership and every pairwise constraint, source singleton/set semantics,
and existing malformed/context/dependency controls. Corpus purity passed and no
corpus was produced. Strict producer-independent workspace/all-target Clippy
also passed with -D warnings and its existing four consumer exclusions.

Evidence: class-constructors-tests.log (historical failure),
class-constructors-corrected-tests.log, class-constructors-clippy.log and normal
class-constructors-commit.log. Full authenticated canon, qualified cardinality,
datatypes, contextual owners, prepared bundle carriage and installed consumer
composition remain required by the original issue; no complete blocker count or
full issue acceptance is claimed.
