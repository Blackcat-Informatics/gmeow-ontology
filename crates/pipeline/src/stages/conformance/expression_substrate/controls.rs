// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny controls whose required authored math TBox is admitted by the producer.

pub(super) const TWINS: &str = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <https://example.org/twin/> .
ex:symL a math:MathematicalSymbol .
ex:symR a math:MathematicalSymbol .
ex:appA a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sA0 , ex:sA1 .
ex:sA0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refA0 .
ex:sA1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refA1 .
ex:refA0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refA1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
ex:appB a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sB0 , ex:sB1 .
ex:sB0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refB0 .
ex:sB1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refB1 .
ex:refB0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refB1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
"#;

pub(super) const OPERATOR_LESS: &str = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/refuted/> .

ex:noOperator a math:ApplicationExpression ;
    math:argumentSlot ex:slot0 .

ex:slot0 a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression ex:leaf .

ex:leaf a math:NumberLiteral ;
    math:literalValue 1 .
"#;
