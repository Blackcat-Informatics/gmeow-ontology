// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The original inference commitment controls, evaluated only by the producer.

pub(super) const PRELUDE: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <http://example.org/inf/> .
ex:methodReason a gmeow:ObservationMethod .
";

pub(super) const WELLFORMED: &str = "\
ex:p1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:concl a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:commit a gmeow:InferenceCommitment ;
    gmeow:premise ex:p1 ;
    gmeow:conclusion ex:concl ;
    gmeow:inferenceModeOf gmeow:modeDeduction .
ex:h1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:h2 .
ex:h2 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
";

pub(super) const MALFORMED: &str = "\
ex:claimX a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:badCommit a gmeow:InferenceCommitment ;
    gmeow:premise ex:claimX ;
    gmeow:conclusion ex:claimX .
ex:selfRival a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:selfRival .
ex:selfAttack a gmeow:Attack ;
    gmeow:attackSource ex:claimX ;
    gmeow:attackTarget ex:claimX ;
    gmeow:attackKind gmeow:attackRebut .
ex:claimY a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:selfArg a gmeow:Argument ; gmeow:argumentConclusion ex:claimY .
ex:componentSelfAttack a gmeow:Attack ;
    gmeow:attackSource ex:selfArg ;
    gmeow:attackTarget ex:claimY ;
    gmeow:attackKind gmeow:attackRebut .
";
