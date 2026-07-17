/**
 * GMEOW developer schema generated from canonical OWL. Lossy by design: restrictions, reification, standpoint, inverseOf, and temporal scope are dropped.
 *
 * GMEOW developer schema generated from canonical OWL. Lossy by design: restrictions, reification, standpoint, inverseOf, and temporal scope are dropped.
 *
 * Package: @blackcatinformatics/gmeow
 * @packageDocumentation
 */
export type JsonPrimitive = string | number | boolean | null;
export type JsonObject = { readonly [key: string]: JsonValue };
export type JsonValue = JsonPrimitive | JsonObject | readonly JsonValue[];

export type AboutnessModeEnum = (string & ("gmeow:aboutnessDescribes" | "gmeow:aboutnessEnacts"));

export type AcceptanceStatusEnum = (string & ("gmeow:acceptanceIn" | "gmeow:acceptanceOut" | "gmeow:acceptanceUndecided"));

export type AccessibilityAssertion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assertionFacet": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:assertionPolarity": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:assertionSubject": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type AccessibilityFacetEnum = (string & ("gmeow:facetAuditory" | "gmeow:facetClearance" | "gmeow:facetCognitive" | "gmeow:facetLifeSupport" | "gmeow:facetStepFree" | "gmeow:facetVisual" | "gmeow:facetWheelchair"));

export type AccessibilityPolarityEnum = (string & ("gmeow:polarityBarrier" | "gmeow:polarityFeature" | "gmeow:polarityLimited"));

export type AccountStatusEnum = (string & ("gmeow:accountStatusActive" | "gmeow:accountStatusDormant" | "gmeow:accountStatusHistorical"));

export type AdequacyAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:adequacyUnderStandard"?: (EpistemicStandardEnum | readonly (EpistemicStandardEnum)[]);
  readonly "gmeow:meetsThreshold"?: (AdequacyVerdictEnum | readonly (AdequacyVerdictEnum)[]);
  readonly [key: string]: JsonValue;
};

export type AdequacyVerdictEnum = (string & ("gmeow:adequacyMet" | "gmeow:adequacyUndetermined" | "gmeow:adequacyUnmet"));

export type AestheticQualityEnum = (string & ("gmeow:qualityElegance" | "gmeow:qualityKitsch" | "gmeow:qualitySublimity"));

export type AffectClassifierLabel = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:memberOfLabelSet": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type AffectClassifierOutput = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:classifiedTarget": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:classifierScore": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:emittedLabel": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:producedBy": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:scoreSemantics": (ScoreSemanticsEnum | readonly (ScoreSemanticsEnum)[]);
  readonly [key: string]: JsonValue;
};

export type AffectComposite = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:affectiveConstituent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type AffectDecision = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:decidedLabel": JsonValue;
  readonly "gmeow:decisionCrossedThreshold": JsonValue;
  readonly "gmeow:derivedByFunction": JsonValue;
  readonly "gmeow:observedFeature": JsonValue;
  readonly "gmeow:vantage": JsonValue;
  readonly [key: string]: JsonValue;
};

export type AffectFunctionEnum = (string & ("gmeow:fnAffectiveIntensity" | "gmeow:fnArgmax"));

export type AffectScaleProfile = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:profileRangeMax": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:profileRangeMin": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type AffectVectorObservation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:vectorComponent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:vectorProfile": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type AffectiveClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type AffectiveExperience = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:experiencer": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:feltAffect": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:mentalProcessType"?: ({
          readonly "@id": "gmeow:processAffectiveExperience";
        } | readonly ({
              readonly "@id": "gmeow:processAffectiveExperience";
            })[]);
  readonly [key: string]: JsonValue;
};

export type Agent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type AgentSession = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:sessionConfiguration"?: JsonValue;
  readonly "gmeow:sessionSubjectStage"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type AggregationFunctionEnum = (string & ("gmeow:aggAverage" | "gmeow:aggCentroid" | "gmeow:aggCount" | "gmeow:aggDensity" | "gmeow:aggMaximum" | "gmeow:aggMinimum" | "gmeow:aggSum"));

export type Analogy = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:analogicalSource": JsonValue;
  readonly "gmeow:analogicalTarget": JsonValue;
  readonly "gmeow:hasCorrespondence": JsonValue;
  readonly [key: string]: JsonValue;
};

export type AnalysisPropertyEnum = (string & ("gmeow:analysisPropertyFormFunction" | "gmeow:analysisPropertyGroove" | "gmeow:analysisPropertyHarmonyLabel" | "gmeow:analysisPropertyKey" | "gmeow:analysisPropertyMeter" | "gmeow:analysisPropertyMode" | "gmeow:analysisPropertyMotifIdentity" | "gmeow:analysisPropertySchema" | "gmeow:analysisPropertySegment" | "gmeow:analysisPropertyTuningIdentification"));

/**
 * Free-form metadata about an asserted triple (e.g. meta:accordingTo, meta:confidence, meta:assertedAt). Permissive.
 */
export type Annotation = {
  readonly [key: string]: (string | number | boolean | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
};

export type AnnotationMotivationEnum = (string & ("gmeow:motivationAssessing" | "gmeow:motivationBookmarking" | "gmeow:motivationCommenting" | "gmeow:motivationDescribing" | "gmeow:motivationHighlighting" | "gmeow:motivationLinking" | "gmeow:motivationModerating" | "gmeow:motivationQuestioning" | "gmeow:motivationReplying" | "gmeow:motivationTagging"));

export type Appellation = ({
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
} & ({
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "gmeow:fullName": JsonValue;
    readonly [key: string]: JsonValue;
  } | {
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "gmeow:hasNamePart": JsonValue;
    readonly [key: string]: JsonValue;
  }));

export type Appraisal = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:appraisalDimension"?: JsonValue;
  readonly "gmeow:appraisalOf": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:appraisalScaleProfile"?: ((AffectScaleProfile | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((AffectScaleProfile | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:appraisalValue"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:vantage": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type AppraisalDimensionEnum = (string & ("gmeow:dimensionAgency" | "gmeow:dimensionCertainty" | "gmeow:dimensionCoping" | "gmeow:dimensionGoalCongruence" | "gmeow:dimensionGoalRelevance" | "gmeow:dimensionNormCompatibility" | "gmeow:dimensionNovelty" | "gmeow:dimensionObjectFocus" | "gmeow:dimensionTemporalOrientation"));

export type ArcSample = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:samplePosition": ((NarrativePosition | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(NarrativePosition | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(NarrativePosition | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:sampleState": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:sampleSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:vantage": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type ArcTypeEnum = (string & ("gmeow:arcTypeComingOfAge" | "gmeow:arcTypeCorruption" | "gmeow:arcTypeFall" | "gmeow:arcTypeQuest" | "gmeow:arcTypeRecovery" | "gmeow:arcTypeRedemption"));

export type ArchaeologicalFindContext = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:findContextTarget"?: ((PhysicalObject | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PhysicalObject | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Argument = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:argumentConclusion": JsonValue;
  readonly "gmeow:argumentInferenceStep"?: JsonValue;
  readonly "gmeow:hasInferenceApplication"?: ((InferenceApplication | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((InferenceApplication | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:hasPremiseUse"?: ((PremiseUse | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PremiseUse | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ArgumentEvaluation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:acceptanceStatus"?: (AcceptanceStatusEnum | readonly (AcceptanceStatusEnum)[]);
  readonly "gmeow:evaluatesArgument"?: ((Argument | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Argument | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:underSemantics": JsonValue;
  readonly [key: string]: JsonValue;
};

export type ArticulationKindEnum = (string & ("gmeow:articulationAccent" | "gmeow:articulationHarmonic" | "gmeow:articulationLegato" | "gmeow:articulationMarcato" | "gmeow:articulationPizzicato" | "gmeow:articulationStaccato" | "gmeow:articulationTenuto"));

export type AssertoricForceEnum = (string & ("gmeow:assertoricAssert" | "gmeow:assertoricAssume" | "gmeow:assertoricConjecture" | "gmeow:assertoricRetract"));

export type Assessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assessmentCriterion"?: ((Criterion | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Criterion | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:assessmentRubric"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:assessmentScoreValue": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:assessmentTarget": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:vantage": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type AssetTypeEnum = (string & ("gmeow:assetTypeBond" | "gmeow:assetTypeCommodity" | "gmeow:assetTypeCryptocurrency" | "gmeow:assetTypeRealEstate" | "gmeow:assetTypeStock"));

export type AtomicConstraint = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:constraintOperator": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:leftOperand": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:rightOperand"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type Attack = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:attackKind"?: (AttackKindEnum | readonly (AttackKindEnum)[]);
  readonly "gmeow:attackSource"?: ((Argument | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Argument | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:attackTarget": JsonValue;
  readonly [key: string]: JsonValue;
};

export type AttackKindEnum = (string & ("gmeow:attackRebut" | "gmeow:attackUndercut" | "gmeow:attackUndermine"));

export type Attestation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:attestedSubject"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:attester"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:issuedAt"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type AttestationArtifact = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:artifactMediaType"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type AttestationTypeEnum = (string & ("gmeow:attestationTypeAIOutput" | "gmeow:attestationTypeBlockchainClaim" | "gmeow:attestationTypeC2PA" | "gmeow:attestationTypeCoherenceCertificate" | "gmeow:attestationTypeConformanceVerdict" | "gmeow:attestationTypeDSSE" | "gmeow:attestationTypeDocumentationArtifact" | "gmeow:attestationTypeEAT" | "gmeow:attestationTypeFactCheck" | "gmeow:attestationTypeGitSignedTag" | "gmeow:attestationTypeInToto" | "gmeow:attestationTypeNanopublication" | "gmeow:attestationTypeQualityReport" | "gmeow:attestationTypeReleaseManifest" | "gmeow:attestationTypeSCITT" | "gmeow:attestationTypeSLSAProvenance" | "gmeow:attestationTypeSignedRDF" | "gmeow:attestationTypeVerifiableCredential"));

export type AuthorityLevelEnum = (string & ("gmeow:authorityAbsolute" | "gmeow:authorityConditional" | "gmeow:authorityHigh" | "gmeow:authorityMedium"));

export type Availability = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:availabilityAgent": (Agent | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:availabilitySlot": (TimeInterval | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:availabilityStatus": AvailabilityStatusEnum;
  readonly [key: string]: JsonValue;
};

export type AvailabilityStatusEnum = (string & ("gmeow:availabilityStatusBusy" | "gmeow:availabilityStatusFree" | "gmeow:availabilityStatusOutOfOffice" | "gmeow:availabilityStatusTentative"));

export type AwarenessLevelEnum = (string & ("gmeow:levelAlert" | "gmeow:levelDrowsy" | "gmeow:levelHyperalert" | "gmeow:levelObtunded" | "gmeow:levelRelaxed" | "gmeow:levelUnresponsive"));

export type AwarenessModeEnum = (string & ("gmeow:modeAsleep" | "gmeow:modeComatose" | "gmeow:modeDormant" | "gmeow:modeDreaming" | "gmeow:modeDrowsy" | "gmeow:modeFlow" | "gmeow:modeFocused" | "gmeow:modeLucidDreaming" | "gmeow:modeMeditative" | "gmeow:modeMindWandering" | "gmeow:modeOfflineReplay" | "gmeow:modeOnlineInference" | "gmeow:modeREM" | "gmeow:modeSampling" | "gmeow:modeSedated" | "gmeow:modeTraining" | "gmeow:modeWaking"));

export type AxisContextScopeEnum = (string & ("gmeow:scopeDepsClosure" | "gmeow:scopeMergedClosure" | "gmeow:scopeSliceLocal"));

export type Blob = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:contentDigest": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Block = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:blockHash"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:blockNumber"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type BlockchainNetwork = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:chainId"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type BlockingDispositionEnum = (string & ("gmeow:blockingBlocking" | "gmeow:blockingCoherent"));

export type BlutdruckDiastolicCardinality = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0005": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckDiastolicCardinality2 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0005"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckDiastolicCardinality3 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0005": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckDiastolicCardinality4 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0005"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckDiastolicCardinality5 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0005"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0004": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality2 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0004"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality3 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0004": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality4 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0004": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality5 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0004"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicCardinality6 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0004"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckSystolicValueSet = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/definingCode"?: (({
          readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0010";
        } | {
          readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0011";
        } | {
          readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0012";
        } | {
          readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0013";
        }) | readonly (({
              readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0010";
            } | {
              readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0011";
            } | {
              readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0012";
            } | {
              readonly "@id": "gmeow:openehr/bloodpressure/terminology/local/at0013";
            }))[]);
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0000 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt000010 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt000011 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt000012 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt000013 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt000014 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/termBinding"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00002 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00003 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00004 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/definingCode"?: ({
          readonly "@id": "gmeow:openehr/bloodpressure/terminology/openehr/433";
        } | readonly ({
              readonly "@id": "gmeow:openehr/bloodpressure/terminology/openehr/433";
            })[]);
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00005 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00006 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00007 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00008 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00009 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0001 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0001": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00012 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0001"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00013 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0001": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00014 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0001"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0002 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0003 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0003": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00032 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0003"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0006 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0006"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00062 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0006": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00063 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0006"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0007 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0007": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00072 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0007"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt0011 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/occurrences/at0011": JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt00112 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/existence/at0011"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt1025 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt1030 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt1057 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BlutdruckAt1058 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BranchConditionTypeEnum = (string & ("gmeow:branchConditionIf" | "gmeow:branchConditionLoop" | "gmeow:branchConditionParallel" | "gmeow:branchConditionSwitch"));

export type BuildActivity = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:buildOutput"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:buildSource"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type BuildDataFlow = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:buildFlowFrom"?: ((PipelineStage | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PipelineStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:buildFlowTo"?: ((PipelineStage | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PipelineStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type CadastralReference = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:referenceAuthority": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:referenceJurisdiction": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:referenceType": (CadastralReferenceTypeEnum | readonly (CadastralReferenceTypeEnum)[]);
  readonly "gmeow:referenceValue": (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly [({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string), ...Array<({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string)>]);
  readonly [key: string]: JsonValue;
};

export type CadastralReferenceTypeEnum = (string & ("gmeow:referenceTypeFolio" | "gmeow:referenceTypeLot" | "gmeow:referenceTypeParcelId" | "gmeow:referenceTypeSurveyPlan" | "gmeow:referenceTypeTitle"));

export type CalendarMethodEnum = (string & ("gmeow:calendarMethodAdd" | "gmeow:calendarMethodCancel" | "gmeow:calendarMethodCounter" | "gmeow:calendarMethodDeclineCounter" | "gmeow:calendarMethodPublish" | "gmeow:calendarMethodRefresh" | "gmeow:calendarMethodReply" | "gmeow:calendarMethodRequest"));

export type CalibrationStatusEnum = (string & ("gmeow:overconfident" | "gmeow:underconfident" | "gmeow:wellCalibrated"));

export type CarrierMediumEnum = (string & ("gmeow:mediumEInkFile" | "gmeow:mediumOpticalDisc" | "gmeow:mediumPrint" | "gmeow:mediumServerObject"));

export type Cascade = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:cascadeFirstLink": ((CausalLink | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(CausalLink | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(CausalLink | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:cascadeSeverity": SeverityLevelEnum;
  readonly [key: string]: JsonValue;
};

export type CausalLink = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:causalModality": CausalModalityEnum;
  readonly "gmeow:linkAntecedent": EventTypeEnum;
  readonly "gmeow:linkConsequent": EventTypeEnum;
  readonly "gmeow:linkMechanism"?: JsonValue;
  readonly "gmeow:linkStrength"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type CausalModalityEnum = (string & ("gmeow:causallyEnables" | "gmeow:causallyNecessitates" | "gmeow:causallyPrevents" | "gmeow:causallyPromotes"));

export type CelestialObjectTypeEnum = (string & ("gmeow:celestialObjectTypeAsteroid" | "gmeow:celestialObjectTypeCluster" | "gmeow:celestialObjectTypeComet" | "gmeow:celestialObjectTypeGalaxy" | "gmeow:celestialObjectTypeNebula" | "gmeow:celestialObjectTypePlanet" | "gmeow:celestialObjectTypeSpacecraft" | "gmeow:celestialObjectTypeStar"));

export type CelestialReferenceOriginEnum = (string & ("gmeow:refOriginBarycentric" | "gmeow:refOriginGeocentric" | "gmeow:refOriginHeliocentric" | "gmeow:refOriginTopocentric"));

export type Certification = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:certifiedIdentity"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:certifiedKey"?: ((CryptographicKey | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((CryptographicKey | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:certifier"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type CharacterArc = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:arcSubject": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:arcType": (ArcTypeEnum | readonly (ArcTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type Chunk = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:chunkOf": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:spanEnd": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:spanStart": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type CitationAct = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:citationIntent": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:citedEntity": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:citingEntity": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type CitationIntentEnum = (string & ("gmeow:intentBridgedByReference" | "gmeow:intentCitesAsDataSource" | "gmeow:intentConformsTo" | "gmeow:intentDerivedFrom" | "gmeow:intentDisagreesWith" | "gmeow:intentDocuments" | "gmeow:intentExtends" | "gmeow:intentIsInspiredBy" | "gmeow:intentSupports" | "gmeow:intentUsesMethodIn"));

export type ClaimEvaluation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:evaluates"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type ClaimVeridicalityEnum = (string & ("gmeow:veridicalityLicensedFalsehood" | "gmeow:veridicalityUntrue"));

export type CodecClassEnum = (string & ("gmeow:codecClassCompress" | "gmeow:codecClassEncode" | "gmeow:codecClassEncrypt"));

export type CollectionMemberRoleEnum = (string & ("gmeow:collectionMemberRoleAscentOnly" | "gmeow:collectionMemberRoleDescentOnly" | "gmeow:collectionMemberRoleGhammaz" | "gmeow:collectionMemberRoleMember" | "gmeow:collectionMemberRoleOrnamental" | "gmeow:collectionMemberRoleSamvadi" | "gmeow:collectionMemberRoleTonicFinalis" | "gmeow:collectionMemberRoleVadi"));

export type Comment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:commentParent": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Commit = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:authoredBy"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:commitAuthorIdentity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:commitCommitterIdentity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:commitTree"?: ((SourceTree | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SourceTree | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:contentDigest": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Commitment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:commitmentBeneficiary": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:committedAgent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:intentionGoal": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type CommunitySummary = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:summarizesCommunity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type CompilationPreservationRecord = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ComplexCompilation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ComplianceAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assessedEvent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:assessedNorm": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:complianceVerdict": (EvaluationVerdictEnum | readonly (EvaluationVerdictEnum)[]);
  readonly "gmeow:vantage": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type ConceptCategorization = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationResult"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:observedFeature"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:typicality"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ConceptTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:conceptHoldsFor"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:duringInterval"?: ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Condition = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:conditionText": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type ConditionEvaluation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:evaluatedCondition": JsonValue;
  readonly "gmeow:evaluationVerdict": JsonValue;
  readonly "gmeow:vantage": JsonValue;
  readonly [key: string]: JsonValue;
};

export type ConditionExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:expressionLanguage": ExpressionLanguageEnum;
  readonly "gmeow:expressionText": ({
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      } | string);
  readonly [key: string]: JsonValue;
};

export type ConditionGroup = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:groupMember": readonly [{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, ...Array<{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }>];
  readonly "gmeow:groupOperator": GroupOperatorEnum;
  readonly [key: string]: JsonValue;
};

export type ConditionParameter = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:parameterName": JsonValue;
  readonly [key: string]: JsonValue;
};

export type ConflictStrategyEnum = (string & ("gmeow:conflictInvalid" | "gmeow:conflictPerm" | "gmeow:conflictProhibit"));

export type ConstraintLogicEnum = (string & ("gmeow:logicAnd" | "gmeow:logicAndSequence" | "gmeow:logicOr" | "gmeow:logicXone"));

export type ConstraintOperatorEnum = (string & ("gmeow:operatorEq" | "gmeow:operatorGt" | "gmeow:operatorGteq" | "gmeow:operatorHasPart" | "gmeow:operatorIsA" | "gmeow:operatorIsAllOf" | "gmeow:operatorIsAnyOf" | "gmeow:operatorIsNoneOf" | "gmeow:operatorIsPartOf" | "gmeow:operatorLt" | "gmeow:operatorLteq" | "gmeow:operatorNeq"));

export type ContactPoint = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ContactPointTypeEnum = (string & ("gmeow:contactPointTypePersonal" | "gmeow:contactPointTypePersonalDomain" | "gmeow:contactPointTypeSupport" | "gmeow:contactPointTypeWork"));

export type ContainmentTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:containmentChild": ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:containmentParent": ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:duringInterval": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type ContentDispositionEnum = (string & ("gmeow:contentDispositionAttachment" | "gmeow:contentDispositionInline"));

export type ContentOriginEnum = (string & ("gmeow:originBelieved" | "gmeow:originGenerated" | "gmeow:originImagined" | "gmeow:originPerceived" | "gmeow:originRemembered" | "gmeow:originSupposed"));

export type ContentSegment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:segmentIndex"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:segmentOf": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:segmentType"?: (ContentSegmentTypeEnum | readonly (ContentSegmentTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type ContentSegmentTypeEnum = (string & ("gmeow:segmentTypeBackMatter" | "gmeow:segmentTypeChapter" | "gmeow:segmentTypeFrontMatter" | "gmeow:segmentTypeParagraph" | "gmeow:segmentTypeScene" | "gmeow:segmentTypeSection"));

export type ContentTransferEncodingEnum = (string & ("gmeow:transferEncoding7bit" | "gmeow:transferEncoding8bit" | "gmeow:transferEncodingBase64" | "gmeow:transferEncodingBinary" | "gmeow:transferEncodingQuotedPrintable"));

export type ContinuityDetermination = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:determinationForce"?: (DeterminationForceEnum | readonly (DeterminationForceEnum)[]);
  readonly "gmeow:determinationValidity"?: ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:determiningAuthority"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ContinuityVerdictEnum = (string & ("gmeow:continuityDifferent" | "gmeow:continuityIndeterminate" | "gmeow:continuitySame"));

export type Contradiction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:contradictsClaim": readonly [{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, ...Array<{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }>];
  readonly [key: string]: JsonValue;
};

export type ContradictionKindEnum = (string & ("gmeow:contradictionKindFactual" | "gmeow:contradictionKindFraming" | "gmeow:contradictionKindNumeric" | "gmeow:contradictionKindTemporal"));

export type Contribution = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:contributionDegree"?: JsonValue;
  readonly "gmeow:contributionRole": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:contributionTarget": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:contributor": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type ContributionDegreeEnum = (string & ("gmeow:degreeEqual" | "gmeow:degreeLead" | "gmeow:degreeSupporting"));

export type ContributionRoleEnum = (string & ("gmeow:roleAIAssistant" | "gmeow:roleArranger" | "gmeow:roleAuthor" | "gmeow:roleBotContributor" | "gmeow:roleCodeReviewer" | "gmeow:roleComposer" | "gmeow:roleConceptualization" | "gmeow:roleConductor" | "gmeow:roleCoverArtist" | "gmeow:roleDataCuration" | "gmeow:roleDirector" | "gmeow:roleEditor" | "gmeow:roleFormalAnalysis" | "gmeow:roleFundingAcquisition" | "gmeow:roleIllustrator" | "gmeow:roleInventor" | "gmeow:roleInvestigation" | "gmeow:roleLLMAssistedEditor" | "gmeow:roleLetterer" | "gmeow:roleLibrettist" | "gmeow:roleLyricist" | "gmeow:roleMasteringEngineer" | "gmeow:roleMethodology" | "gmeow:roleMixingEngineer" | "gmeow:roleNarrator" | "gmeow:roleOrchestrator" | "gmeow:rolePerformer" | "gmeow:rolePhotographer" | "gmeow:roleProducer" | "gmeow:roleProjectAdministration" | "gmeow:rolePublisher" | "gmeow:roleRecordingEngineer" | "gmeow:roleReleaser" | "gmeow:roleRemixer" | "gmeow:roleResources" | "gmeow:roleSamplingArtist" | "gmeow:roleSecurityContact" | "gmeow:roleSoftware" | "gmeow:roleSoftwareDeveloper" | "gmeow:roleSoftwareMaintainer" | "gmeow:roleSongwriter" | "gmeow:roleSoundDesigner" | "gmeow:roleSupervision" | "gmeow:roleTranslator" | "gmeow:roleValidation" | "gmeow:roleVisualization" | "gmeow:roleWritingOriginalDraft" | "gmeow:roleWritingReviewEditing"));

export type ControlAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:controlAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:controlInterval"?: ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:controlLevel"?: (ControlLevelEnum | readonly (ControlLevelEnum)[]);
  readonly [key: string]: JsonValue;
};

export type ControlFlow = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:flowSource": (ProcedureStep | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:flowTarget": (ProcedureStep | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type ControlLevelEnum = (string & ("gmeow:controlContested" | "gmeow:controlFull" | "gmeow:controlPartial"));

export type CoordinateMatrix = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:coordinateMatrixFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type CoordinateObservation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observedFeature"?: ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:vantage"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Copyright = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:copyrightHolder": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:copyrightNotice"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:copyrightStatus"?: (CopyrightStatusEnum | readonly (CopyrightStatusEnum)[]);
  readonly "gmeow:copyrightWork": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:copyrightYear"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type CopyrightStatusEnum = (string & ("gmeow:copyrightStatusInCopyright" | "gmeow:copyrightStatusInCopyrightEducationalUse" | "gmeow:copyrightStatusInCopyrightEuOrphanWork" | "gmeow:copyrightStatusInCopyrightNonCommercialUse" | "gmeow:copyrightStatusInCopyrightRightsholderUnlocatable" | "gmeow:copyrightStatusNoCopyrightContractualRestrictions" | "gmeow:copyrightStatusNoCopyrightNonCommercialOnly" | "gmeow:copyrightStatusNoCopyrightOtherLegalRestrictions" | "gmeow:copyrightStatusNoCopyrightUnitedStates" | "gmeow:copyrightStatusNoKnownCopyright" | "gmeow:copyrightStatusNotEvaluated" | "gmeow:copyrightStatusPublicDomain" | "gmeow:copyrightStatusUndetermined"));

export type CoreAffectDimensionEnum = (string & ("gmeow:dimensionArousal" | "gmeow:dimensionDominance" | "gmeow:dimensionUnpredictability" | "gmeow:dimensionValence"));

export type Correspondence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:correspondingSource": JsonValue;
  readonly "gmeow:correspondingTarget": JsonValue;
  readonly [key: string]: JsonValue;
};

export type CoverageDepthEnum = (string & ("gmeow:coverageDepthPassingMention" | "gmeow:coverageDepthRoutineFiling" | "gmeow:coverageDepthSignificantCoverage"));

export type CreativeDerivation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:derivationProduct"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:derivationSource"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:derivationType"?: (DerivationTypeEnum | readonly (DerivationTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type CreativeWorkTypeEnum = (string & ("gmeow:workTypeAudiovisual" | "gmeow:workTypeCartographic" | "gmeow:workTypeChoreographic" | "gmeow:workTypeDataset" | "gmeow:workTypeFilm" | "gmeow:workTypeLiterary" | "gmeow:workTypeMusical" | "gmeow:workTypeNarrative" | "gmeow:workTypePhotographic" | "gmeow:workTypeSoftware" | "gmeow:workTypeVisual" | "gmeow:workTypeWritten"));

export type CredenceLevelEnum = (string & ("gmeow:credenceCertain" | "gmeow:credenceLikely" | "gmeow:credencePossible" | "gmeow:credenceUnspecified"));

export type Credential = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:credentialIssuer"?: ((Organization | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Organization | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Criterion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:penaltyPole": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:rewardPole": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type CriterionDomainEnum = (string & ("gmeow:criterionDomainAesthetic" | "gmeow:criterionDomainFactual" | "gmeow:criterionDomainRelational" | "gmeow:criterionDomainSafety" | "gmeow:criterionDomainStylistic"));

export type CrossNodeGlutWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:glutWitnessOf"?: ((Finding | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Finding | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type CryptoWallet = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:walletScheme": (WalletSchemeEnum | readonly (WalletSchemeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type CryptographicKey = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type DataFlow = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:dataFlowSource": (ProcedureStep | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:dataFlowTarget": (ProcedureStep | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type DatingMethodEnum = (string & ("gmeow:datingMethodAminoAcidRacemization" | "gmeow:datingMethodDendrochronology" | "gmeow:datingMethodElectronSpinResonance" | "gmeow:datingMethodOpticallyStimulatedLuminescence" | "gmeow:datingMethodPaleomagnetism" | "gmeow:datingMethodPotassiumArgon" | "gmeow:datingMethodRadiocarbon" | "gmeow:datingMethodStratigraphicCorrelation" | "gmeow:datingMethodThermoluminescence" | "gmeow:datingMethodUraniumLead"));

export type DayOfWeekEnum = (string & ("gmeow:dayFriday" | "gmeow:dayMonday" | "gmeow:daySaturday" | "gmeow:daySunday" | "gmeow:dayThursday" | "gmeow:dayTuesday" | "gmeow:dayWednesday"));

export type DegreeOfFreedom = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:dofConstraintFunction"?: JsonValue;
  readonly "gmeow:dofExpression"?: JsonValue;
  readonly "gmeow:dofParameter": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:dofStatus": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:dofWork"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type DeonticModalityEnum = (string & ("gmeow:deonticObligation" | "gmeow:deonticPermission" | "gmeow:deonticProhibition" | "gmeow:deonticRecommendation"));

export type DepictionContextEnum = (string & ("gmeow:depictionContextActionShot" | "gmeow:depictionContextCandid" | "gmeow:depictionContextChildhood" | "gmeow:depictionContextEvent" | "gmeow:depictionContextFamily" | "gmeow:depictionContextFormal" | "gmeow:depictionContextNow" | "gmeow:depictionContextPortrait" | "gmeow:depictionContextProfessional" | "gmeow:depictionContextSelfPortrait" | "gmeow:depictionContextSocial" | "gmeow:depictionContextWork"));

export type DepictionUsage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:depictionContext": DepictionContextEnum;
  readonly "gmeow:depictionImage": (MediaObject | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:depictionSubject": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type DerivationKindEnum = (string & ("gmeow:derivationAffixation" | "gmeow:derivationBackFormation" | "gmeow:derivationBorrowing" | "gmeow:derivationCalque" | "gmeow:derivationClipping" | "gmeow:derivationCompounding" | "gmeow:derivationFolkEtymology" | "gmeow:derivationInheritance" | "gmeow:derivationReanalysis" | "gmeow:derivationReconstruction" | "gmeow:derivationSemanticShift" | "gmeow:derivationSoundChange" | "gmeow:derivationSpellingChange" | "gmeow:derivationUnknownOrigin"));

export type DerivationTypeEnum = (string & ("gmeow:derivationTypeArrangement" | "gmeow:derivationTypeContrafact" | "gmeow:derivationTypeCover" | "gmeow:derivationTypeInterpolation" | "gmeow:derivationTypeMashup" | "gmeow:derivationTypeOrchestration" | "gmeow:derivationTypeParody" | "gmeow:derivationTypeQuotation" | "gmeow:derivationTypeRemix" | "gmeow:derivationTypeSample" | "gmeow:derivationTypeTranscription" | "gmeow:derivationTypeVariation"));

export type DerivedAffectIntensityObservation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Desire = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:intentBearer": JsonValue;
  readonly "gmeow:intentionGoal": JsonValue;
  readonly [key: string]: JsonValue;
};

export type DeterminacyEnum = (string & ("gmeow:determinacyCrisp" | "gmeow:determinacyDisputed" | "gmeow:determinacyFuzzy" | "gmeow:determinacyProbabilistic" | "gmeow:determinacyVague"));

export type DeterminationForceEnum = (string & ("gmeow:forceAdvisory" | "gmeow:forceBinding" | "gmeow:forceProvisional"));

export type DeterminationStatusEnum = (string & ("gmeow:determinationConstrained" | "gmeow:determinationDelegatedEnvironment" | "gmeow:determinationDelegatedPerformer" | "gmeow:determinationDelegatedProcess" | "gmeow:determinationFixed" | "gmeow:determinationFree"));

export type DiagnosticSeverityEnum = (string & ("gmeow:severityError" | "gmeow:severityInfo" | "gmeow:severityNote" | "gmeow:severityWarning"));

export type DiagnosticStandpointEnum = (string & ("gmeow:standpointAdvisory" | "gmeow:standpointBinding" | "gmeow:standpointPerspectival"));

export type Diastolic = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/magnitude"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:openehr/bloodpressure/precision"?: JsonValue;
  readonly "gmeow:openehr/bloodpressure/units": "mm[Hg]";
  readonly [key: string]: JsonValue;
};

export type Diff = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:diffFrom"?: ((Commit | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Commit | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:diffTo"?: ((Commit | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Commit | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type DigitalSubjectTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:tenureSubjectAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:tenureSupportedBy"?: ((Observation | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Observation | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:tenureVantage"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type DimensionFamilyEnum = (string & ("gmeow:familyAppraisal" | "gmeow:familyCoreAffect"));

export type DisclosurePolicyEnum = (string & ("gmeow:policyInternalOnly" | "gmeow:policyNeverPublic" | "gmeow:policyPublicCareful" | "gmeow:policyPublicOnlyWithIndependentSource" | "gmeow:policyPublicSafe" | "gmeow:policySensitive"));

export type DistanceMetricEnum = (string & ("gmeow:distanceMetricCosine" | "gmeow:distanceMetricDotProduct" | "gmeow:distanceMetricEuclidean"));

export type DocCoverageDimensionEnum = (string & ("gmeow:dimAlignment" | "gmeow:dimAnnotationCoat" | "gmeow:dimCompetencyRationale" | "gmeow:dimDefinition" | "gmeow:dimExample" | "gmeow:dimFixturePair" | "gmeow:dimLabel" | "gmeow:dimLinkageCoverage" | "gmeow:dimLossJudgmentSound" | "gmeow:dimLossLedgerRow" | "gmeow:dimProseQuality" | "gmeow:dimProvenanceHonesty" | "gmeow:dimRealizedState" | "gmeow:dimScopeNote" | "gmeow:dimTestReach" | "gmeow:dimThesisSentence" | "gmeow:dimTranslationCoverage" | "gmeow:dimUsageAdvice" | "gmeow:dimWorkedInstance"));

export type DocEvidence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:docEvidenceKind": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:docGroundedBy": JsonValue;
  readonly [key: string]: JsonValue;
};

export type DocEvidenceKindEnum = (string & ("gmeow:docEvidenceKindCompetency" | "gmeow:docEvidenceKindDiagnostics" | "gmeow:docEvidenceKindFixture" | "gmeow:docEvidenceKindLoss" | "gmeow:docEvidenceKindProvenance"));

export type DocFixtureKindEnum = (string & ("gmeow:docFixtureKindCounterExample" | "gmeow:docFixtureKindWellformed"));

export type DocMaturityEnum = (string & ("gmeow:docMaturityBasic" | "gmeow:docMaturityFull" | "gmeow:docMaturityMaximal" | "gmeow:docMaturityMinimal"));

export type DocumentationConcernEnum = (string & ("gmeow:concernDisclosure" | "gmeow:concernFrames" | "gmeow:concernGTSPackaging" | "gmeow:concernIdentifiersCoreference" | "gmeow:concernProvenanceEvidence" | "gmeow:concernStandpoints" | "gmeow:concernStatementMetadata"));

export type DocumentedTerm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:docCategory": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:docOwnerSlice": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type DoxasticStandpointClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:adequateUnder"?: ((AdequacyAssessment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((AdequacyAssessment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:basesBeliefOn"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:claimOfBelief"?: ((DoxasticState | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((DoxasticState | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:defeatedBy"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:hasAvailableEvidence"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:hasDefeatStatus"?: (JustificationStatusEnum | readonly (JustificationStatusEnum)[]);
  readonly "gmeow:supportsUnder"?: ((SupportAssessment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SupportAssessment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type DoxasticState = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:adequateUnder"?: ((AdequacyAssessment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((AdequacyAssessment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:basesBeliefOn"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:credence"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:defeatedBy"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:doxasticClaim"?: ((StandpointClaim | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((StandpointClaim | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:doxasticContent"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:epistemicAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:hasAvailableEvidence"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:hasDefeatStatus"?: (JustificationStatusEnum | readonly (JustificationStatusEnum)[]);
  readonly "gmeow:supportsUnder"?: ((SupportAssessment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SupportAssessment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type DoxasticTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:duringInterval"?: ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:tenureOfDoxasticState"?: ((DoxasticState | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((DoxasticState | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type DreamReport = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:contentOrigin"?: ({
          readonly "@id": "gmeow:originImagined";
        } | readonly ({
              readonly "@id": "gmeow:originImagined";
            })[]);
  readonly "gmeow:mentalProcessType"?: ({
          readonly "@id": "gmeow:processRecollection";
        } | readonly ({
              readonly "@id": "gmeow:processRecollection";
            })[]);
  readonly [key: string]: JsonValue;
};

export type Duty = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:ruleAction": JsonValue;
  readonly [key: string]: JsonValue;
};

export type DynamicsValueEnum = (string & ("gmeow:dynamicsF" | "gmeow:dynamicsFf" | "gmeow:dynamicsFff" | "gmeow:dynamicsMf" | "gmeow:dynamicsMp" | "gmeow:dynamicsP" | "gmeow:dynamicsPp" | "gmeow:dynamicsPpp"));

export type Embedding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:distanceMetric": DistanceMetricEnum;
  readonly "gmeow:embeddingModel": (SoftwareAgent | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:embeddingOf": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type EmbodimentAssignment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assignmentCarrier"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:assignmentSubject"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Emotion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:emotionBearer": JsonValue;
  readonly "gmeow:emotionType": JsonValue;
  readonly [key: string]: JsonValue;
};

export type EmotionTypeEnum = (string & ("gmeow:emotionAnger" | "gmeow:emotionAnticipation" | "gmeow:emotionDisgust" | "gmeow:emotionFear" | "gmeow:emotionJoy" | "gmeow:emotionSadness" | "gmeow:emotionSaudade" | "gmeow:emotionSchadenfreude" | "gmeow:emotionSurprise" | "gmeow:emotionTrust"));

export type Employment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:employmentType"?: (EmploymentTypeEnum | readonly (EmploymentTypeEnum)[]);
  readonly "gmeow:membershipMember"?: ((Person | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Person | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:membershipOrganization"?: ((Organization | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Organization | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type EmploymentTypeEnum = (string & ("gmeow:employmentTypeApprentice" | "gmeow:employmentTypeContract" | "gmeow:employmentTypeFreelance" | "gmeow:employmentTypeFullTime" | "gmeow:employmentTypeIntern" | "gmeow:employmentTypePartTime" | "gmeow:employmentTypeVolunteer"));

export type Entity = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type EntityExistence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:duringInterval": JsonValue;
  readonly "gmeow:existenceEntity": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type EpistemicStandardEnum = (string & ("gmeow:standardLegalBeyondReasonableDoubt" | "gmeow:standardLegalClearAndConvincing" | "gmeow:standardLegalPreponderance" | "gmeow:standardOrdinary" | "gmeow:standardScientific"));

export type EtymologicalDerivation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:derivationKind"?: (DerivationKindEnum | readonly (DerivationKindEnum)[]);
  readonly "gmeow:derivationTarget"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:etymonSource"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type EvaluationVerdictEnum = (string & ("gmeow:verdictHeld" | "gmeow:verdictNotHeld" | "gmeow:verdictUndetermined"));

export type Event = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type EventInvitation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:invitationEvent": (Event | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:invitationInvitee": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:invitationStatus": InvitationStatusEnum;
  readonly [key: string]: JsonValue;
};

export type EventSchedule = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:scheduleTemplateEvent"?: ((Event | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Event | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type EventTypeEnum = (string & ("gmeow:eventTypeAcquisition" | "gmeow:eventTypeAdoption" | "gmeow:eventTypeAgentEpisode" | "gmeow:eventTypeAnnulment" | "gmeow:eventTypeAudit" | "gmeow:eventTypeBaptism" | "gmeow:eventTypeBarMitzvah" | "gmeow:eventTypeBatMitzvah" | "gmeow:eventTypeBirth" | "gmeow:eventTypeBuild" | "gmeow:eventTypeBullshit" | "gmeow:eventTypeBurial" | "gmeow:eventTypeCensus" | "gmeow:eventTypeCensusActivity" | "gmeow:eventTypeChristening" | "gmeow:eventTypeClinicalTrial" | "gmeow:eventTypeCodeReview" | "gmeow:eventTypeCommit" | "gmeow:eventTypeConcert" | "gmeow:eventTypeConfirmation" | "gmeow:eventTypeCreation" | "gmeow:eventTypeCremation" | "gmeow:eventTypeDJSet" | "gmeow:eventTypeDeath" | "gmeow:eventTypeDeception" | "gmeow:eventTypeDestruction" | "gmeow:eventTypeDisinformation" | "gmeow:eventTypeDissolution" | "gmeow:eventTypeDistortion" | "gmeow:eventTypeDivorce" | "gmeow:eventTypeEmigration" | "gmeow:eventTypeEngagement" | "gmeow:eventTypeExcavation" | "gmeow:eventTypeExpressionCreation" | "gmeow:eventTypeFabrication" | "gmeow:eventTypeFirstCommunion" | "gmeow:eventTypeForgery" | "gmeow:eventTypeFuneral" | "gmeow:eventTypeGraduation" | "gmeow:eventTypeHiring" | "gmeow:eventTypeImageAnnotation" | "gmeow:eventTypeImageCapture" | "gmeow:eventTypeImageProcessing" | "gmeow:eventTypeImageScanning" | "gmeow:eventTypeImmigration" | "gmeow:eventTypeImpersonation" | "gmeow:eventTypeInhabitationTransition" | "gmeow:eventTypeJamSession" | "gmeow:eventTypeLie" | "gmeow:eventTypeManifestationProduction" | "gmeow:eventTypeMarriage" | "gmeow:eventTypeMerge" | "gmeow:eventTypeMerger" | "gmeow:eventTypeMigration" | "gmeow:eventTypeMilitaryService" | "gmeow:eventTypeMusicalPerformance" | "gmeow:eventTypeNameChange" | "gmeow:eventTypeNaturalization" | "gmeow:eventTypeOmission" | "gmeow:eventTypeOrdination" | "gmeow:eventTypeOverdub" | "gmeow:eventTypePaltering" | "gmeow:eventTypeProbate" | "gmeow:eventTypePromotion" | "gmeow:eventTypePush" | "gmeow:eventTypeRecordingSession" | "gmeow:eventTypeReflection" | "gmeow:eventTypeRehearsal" | "gmeow:eventTypeRelease" | "gmeow:eventTypeRename" | "gmeow:eventTypeResidence" | "gmeow:eventTypeResignation" | "gmeow:eventTypeRetirement" | "gmeow:eventTypeSelfDeception" | "gmeow:eventTypeSeparation" | "gmeow:eventTypeSoundcheck" | "gmeow:eventTypeSpinOff" | "gmeow:eventTypeSplit" | "gmeow:eventTypeSupersession" | "gmeow:eventTypeSurvey" | "gmeow:eventTypeTake" | "gmeow:eventTypeTermination" | "gmeow:eventTypeTransfer" | "gmeow:eventTypeTransmission" | "gmeow:eventTypeWill" | "gmeow:eventTypeWorkConception"));

export type EvidenceClassEnum = (string & ("gmeow:evidenceANECDOTAL" | "gmeow:evidenceFamilyNarrative" | "gmeow:evidenceGeneratedReport" | "gmeow:evidenceIndependentTradePress" | "gmeow:evidenceLegalFiling" | "gmeow:evidenceNewspaperLead" | "gmeow:evidenceOcrExtract" | "gmeow:evidenceOfficialSource" | "gmeow:evidencePrivateCorrespondence" | "gmeow:evidencePrivateScan" | "gmeow:evidencePublicRegistry" | "gmeow:evidenceRUMOR" | "gmeow:evidenceRawArchive" | "gmeow:evidenceSELF" | "gmeow:evidenceSelfControlledSite" | "gmeow:evidenceSourceCodeArchive" | "gmeow:evidenceVERIFIED"));

export type ExceptionTypeEnum = (string & ("gmeow:exceptionTypeCancellation" | "gmeow:exceptionTypeRescheduling"));

export type Execution = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:executesProcedure"?: ((Procedure | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Procedure | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:executesStep"?: ((ProcedureStep | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ProcedureStep | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ExecutionStatusEnum = (string & ("gmeow:executionStatusCancelled" | "gmeow:executionStatusFailed" | "gmeow:executionStatusPending" | "gmeow:executionStatusRunning" | "gmeow:executionStatusSkipped" | "gmeow:executionStatusSucceeded"));

export type Exemplar = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:citationIntent": CitationIntentEnum;
  readonly "gmeow:citedEntity": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:citingEntity": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:exemplarPolarity": ExemplarPolarityEnum;
  readonly [key: string]: JsonValue;
};

export type ExemplarPolarityEnum = (string & ("gmeow:polarityCautionary" | "gmeow:polarityNegative" | "gmeow:polarityPositive"));

export type Expression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:realizes": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type ExpressionLanguageEnum = (string & ("gmeow:exprLangCedar" | "gmeow:exprLangCel" | "gmeow:exprLangProse" | "gmeow:exprLangRego" | "gmeow:exprLangShacl" | "gmeow:exprLangSparqlAsk" | "gmeow:exprLangXacml"));

export type ExtractedRelationship = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:relationshipSource": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:relationshipTarget": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type FeedPostingKindEnum = (string & ("gmeow:feedPostingKindBlog" | "gmeow:feedPostingKindMicroblog" | "gmeow:feedPostingKindSocial"));

export type FinancialAccount = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:accountCurrency"?: ((ReferenceFrame | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:accountHolder": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:accountType"?: (FinancialAccountTypeEnum | readonly (FinancialAccountTypeEnum)[]);
  readonly "gmeow:bic"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type FinancialAccountTypeEnum = (string & ("gmeow:accountTypeBank" | "gmeow:accountTypeCredit" | "gmeow:accountTypeInvestment" | "gmeow:accountTypeWallet"));

export type FinancialTransaction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:transactionAmount": ((MonetaryAmount | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MonetaryAmount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MonetaryAmount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type Finding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:findingCode": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:findingMessage": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:findingSeverity"?: (DiagnosticSeverityEnum | readonly (DiagnosticSeverityEnum)[]);
  readonly [key: string]: JsonValue;
};

export type FlagshipScenario = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:demonstratedByCompetency": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:demonstratedByExample": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:demonstratedByProducer": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:enforcesFailureClass": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:guardedByCounterExample": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type FormFunctionEnum = (string & ("gmeow:formFunctionAlap" | "gmeow:formFunctionBridge" | "gmeow:formFunctionChorus" | "gmeow:formFunctionDevelopment" | "gmeow:formFunctionDrop" | "gmeow:formFunctionExposition" | "gmeow:formFunctionGat" | "gmeow:formFunctionIntro" | "gmeow:formFunctionOutro" | "gmeow:formFunctionRecapitulation" | "gmeow:formFunctionRiff" | "gmeow:formFunctionVerse"));

export type FrameKindEnum = (string & ("gmeow:frameKindAnalytical" | "gmeow:frameKindCartesian" | "gmeow:frameKindConfigurationSpace" | "gmeow:frameKindCylindrical" | "gmeow:frameKindGeocoding" | "gmeow:frameKindGeodetic" | "gmeow:frameKindGrid" | "gmeow:frameKindHilbert" | "gmeow:frameKindLatentSpace" | "gmeow:frameKindLinear" | "gmeow:frameKindLinearSequence" | "gmeow:frameKindManifold" | "gmeow:frameKindNarrative" | "gmeow:frameKindPhaseSpace" | "gmeow:frameKindPolar" | "gmeow:frameKindScalar" | "gmeow:frameKindTemporal" | "gmeow:frameKindTopological"));

export type FrameRealmEnum = (string & ("gmeow:frameRealmBiological" | "gmeow:frameRealmCelestial" | "gmeow:frameRealmColourspace" | "gmeow:frameRealmCurrency" | "gmeow:frameRealmIndoor" | "gmeow:frameRealmLinguistic" | "gmeow:frameRealmMathematical" | "gmeow:frameRealmMeasurement" | "gmeow:frameRealmMusicAnalysis" | "gmeow:frameRealmMusicalPitch" | "gmeow:frameRealmMusicalTime" | "gmeow:frameRealmNarrative" | "gmeow:frameRealmPerceptual" | "gmeow:frameRealmPsychological" | "gmeow:frameRealmRobotic" | "gmeow:frameRealmTemporal" | "gmeow:frameRealmTerrestrial" | "gmeow:frameRealmVirtual"));

export type GTSDocument = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GTSProfileEnum = (string & ("gmeow:gtsProfileAiPackage" | "gmeow:gtsProfileBundle" | "gmeow:gtsProfileDist" | "gmeow:gtsProfileEvidence" | "gmeow:gtsProfileGeneric" | "gmeow:gtsProfileImage" | "gmeow:gtsProfileOpaque"));

export type GTSSegment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:gtsHeadId": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:gtsProfile"?: (GTSProfileEnum | readonly (GTSProfileEnum)[]);
  readonly "gmeow:gtsSegmentIndex": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type GapShapeEnum = (string & ("gmeow:GapShapeExistentialWitness" | "gmeow:GapShapeMalformed" | "gmeow:GapShapeNativeCoverage" | "gmeow:GapShapeRoleAssertion" | "gmeow:GapShapeVendoringMultiGoal"));

export type GateVerdictEnum = (string & ("gmeow:gateCollected" | "gmeow:gateFatal"));

export type Gender = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GenderEnum = (string & ("gmeow:genderAgender" | "gmeow:genderBigender" | "gmeow:genderDemiboy" | "gmeow:genderDemigirl" | "gmeow:genderGenderfluid" | "gmeow:genderGenderqueer" | "gmeow:genderMan" | "gmeow:genderNonBinary" | "gmeow:genderQuestioning" | "gmeow:genderTwoSpirit" | "gmeow:genderWoman"));

export type GenderExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:expressionValue": (GenderExpressionStyle | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:facetSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:facetVantage": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type GenderExpressionStyle = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GenderExpressionStyleEnum = (string & ("gmeow:expressionAndrogynous" | "gmeow:expressionFeminine" | "gmeow:expressionFluid" | "gmeow:expressionMasculine" | "gmeow:expressionNeutral"));

export type GenderIdentity = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:facetSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:facetVantage": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:genderValue": (Gender | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type GenerativeProcess = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:processFunction"?: JsonValue;
  readonly "gmeow:processKind": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:processRuleText": JsonValue;
  readonly [key: string]: JsonValue;
};

export type GenerativeProcessKindEnum = (string & ("gmeow:generativeProcessKindAlgorithmic" | "gmeow:generativeProcessKindPhasing" | "gmeow:generativeProcessKindRuleBased" | "gmeow:generativeProcessKindStochastic" | "gmeow:generativeProcessKindVerbalScore"));

export type GenericQualityEnum = (string & "gmeow:pressure");

export type Geocode = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GeometryTypeEnum = (string & ("gmeow:geometryTypeLineString" | "gmeow:geometryTypeMultiLineString" | "gmeow:geometryTypeMultiPoint" | "gmeow:geometryTypeMultiPolygon" | "gmeow:geometryTypePoint" | "gmeow:geometryTypePolygon"));

export type Glossary = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GmnCompaction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:gmnCompacts": JsonValue;
  readonly "gmeow:vantage": JsonValue;
  readonly [key: string]: JsonValue;
};

export type GmnCompartmentEnum = (string & ("gmeow:gmnCompartmentNato" | "gmeow:gmnCompartmentPartner"));

export type GmnDictionary = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GmnDispositionBasisEnum = (string & ("gmeow:gmnBasisAmbiguity" | "gmeow:gmnBasisConfusability" | "gmeow:gmnBasisSemanticMismatch" | "gmeow:gmnBasisTokenCost"));

export type GmnEnvelope = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:accordingTo": JsonValue;
  readonly "gmeow:contentDigest": JsonValue;
  readonly "gmeow:gmnDictionaryVersion": JsonValue;
  readonly "gmeow:gmnEnvelopeCorrespondence"?: JsonValue;
  readonly "gmeow:gmnGlyphTableVersion": JsonValue;
  readonly "gmeow:gmnSchemaVersion": JsonValue;
  readonly "gmeow:gmnSecurityRing"?: JsonValue;
  readonly "gmeow:wasGeneratedBy": JsonValue;
  readonly [key: string]: JsonValue;
};

export type GmnFixityEnum = (string & ("gmeow:gmnFixityBracketing" | "gmeow:gmnFixityInfix" | "gmeow:gmnFixityPostfix" | "gmeow:gmnFixityPrefix"));

export type GmnRingCriterionEnum = (string & ("gmeow:gmnCriterionAutomatedUnreviewed" | "gmeow:gmnCriterionHumanReviewed"));

export type GmnRingLevelEnum = (string & ("gmeow:gmnLevelCore" | "gmeow:gmnLevelRestricted" | "gmeow:gmnLevelTrusted"));

export type GmnSecurityRing = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:gmnRingCompartment"?: (GmnCompartmentEnum | readonly (GmnCompartmentEnum)[]);
  readonly "gmeow:gmnRingLevel"?: (GmnRingLevelEnum | readonly (GmnRingLevelEnum)[]);
  readonly [key: string]: JsonValue;
};

export type GmnSecurityRingEnum = (string & ("gmeow:gmnRingCore" | "gmeow:gmnRingNato" | "gmeow:gmnRingRestricted" | "gmeow:gmnRingTrusted"));

export type GmnSigilRoleEnum = (string & ("gmeow:gmnSigilClaim" | "gmeow:gmnSigilDefeater" | "gmeow:gmnSigilEvidence" | "gmeow:gmnSigilLangAst" | "gmeow:gmnSigilLogic" | "gmeow:gmnSigilMath" | "gmeow:gmnSigilModal" | "gmeow:gmnSigilProcess" | "gmeow:gmnSigilProof" | "gmeow:gmnSigilStandpoint"));

export type GmnSymbolDispositionEnum = (string & ("gmeow:gmnDispositionAdoptedGlyph" | "gmeow:gmnDispositionNamedKey" | "gmeow:gmnDispositionSemanticRejection" | "gmeow:gmnDispositionStructuredConstructor"));

export type Goal = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type GovernanceModelEnum = (string & ("gmeow:governanceBDFL" | "gmeow:governanceCorporate" | "gmeow:governanceDAO" | "gmeow:governanceFoundation" | "gmeow:governanceMeritocracy"));

export type GrammaticalAspectEnum = (string & ("gmeow:aspectNone" | "gmeow:aspectPerfective" | "gmeow:aspectPerfectiveProgressive" | "gmeow:aspectProgressive"));

export type GrammaticalTenseEnum = (string & ("gmeow:tenseFuture" | "gmeow:tenseNone" | "gmeow:tensePast" | "gmeow:tensePresent"));

export type GranularityLevelEnum = (string & ("gmeow:granularityAddress" | "gmeow:granularityCentury" | "gmeow:granularityCity" | "gmeow:granularityCountry" | "gmeow:granularityDay" | "gmeow:granularityDecade" | "gmeow:granularityMonth" | "gmeow:granularityPoint" | "gmeow:granularityRegion" | "gmeow:granularityYear"));

export type GraphBoxRoleEnum = (string & ("gmeow:boxABox" | "gmeow:boxCBox" | "gmeow:boxConfigBox" | "gmeow:boxRBox" | "gmeow:boxTBox"));

export type GrooveProfile = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:appliesToTimeFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:grooveGridUnit": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:grooveKind": GrooveProfileKindEnum;
  readonly [key: string]: JsonValue;
};

export type GrooveProfileKindEnum = (string & ("gmeow:grooveProfileKindMeasured" | "gmeow:grooveProfileKindPositionOffsets" | "gmeow:grooveProfileKindSwingRatio"));

export type GroundingAttributeEnum = (string & ("gmeow:groundingEn" | "gmeow:groundingExemplar" | "gmeow:groundingExternalMapped" | "gmeow:groundingFr" | "gmeow:groundingZh"));

export type GroupOperatorEnum = (string & ("gmeow:operatorAll" | "gmeow:operatorAny" | "gmeow:operatorNone"));

export type HarmonicFunctionEnum = (string & ("gmeow:harmonicFunctionDominant" | "gmeow:harmonicFunctionGermanSixth" | "gmeow:harmonicFunctionLeadingTone" | "gmeow:harmonicFunctionMediant" | "gmeow:harmonicFunctionSubdominant" | "gmeow:harmonicFunctionSubmediant" | "gmeow:harmonicFunctionSupertonic" | "gmeow:harmonicFunctionTonic"));

export type Hazard = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hazardBearer": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:hazardSeverity"?: JsonValue;
  readonly "gmeow:manifestedAsType": (EventTypeEnum | readonly (EventTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type Highlight = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:annotationTargetSpan": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly [key: string]: JsonValue;
};

export type Holding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:holdingAgent": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:holdingAsset": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:holdingQuantity": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type Honorific = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type HonorificClassEnum = (string & ("gmeow:honorificClassAcademic" | "gmeow:honorificClassClerical" | "gmeow:honorificClassJudicial" | "gmeow:honorificClassMilitary" | "gmeow:honorificClassNoble" | "gmeow:honorificClassSocial"));

export type HonorificEnum = (string & ("gmeow:honorificDame" | "gmeow:honorificDr" | "gmeow:honorificHon" | "gmeow:honorificLady" | "gmeow:honorificLord" | "gmeow:honorificMr" | "gmeow:honorificMrs" | "gmeow:honorificMs" | "gmeow:honorificMx" | "gmeow:honorificProf" | "gmeow:honorificRev" | "gmeow:honorificSama" | "gmeow:honorificSan" | "gmeow:honorificSayyid" | "gmeow:honorificSir" | "gmeow:honorificSmt" | "gmeow:honorificSri"));

export type HonorificPositionEnum = (string & ("gmeow:honorificPositionPrefix" | "gmeow:honorificPositionSuffix"));

export type Identifier = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:identifierScheme": JsonValue;
  readonly "gmeow:identifierUrl"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type IdentityContinuityAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assessmentFromStage"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:assessmentToStage"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:continuityVerdict"?: (ContinuityVerdictEnum | readonly (ContinuityVerdictEnum)[]);
  readonly [key: string]: JsonValue;
};

export type IdentityFacet = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:facetSubject": (Person | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:facetVantage": (Agent | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type ImageRegion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:regionOf": (MediaObject | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:regionSelector": (RegionSelector | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type IndexAlgorithmEnum = (string & ("gmeow:indexAlgorithmFlat" | "gmeow:indexAlgorithmHnsw" | "gmeow:indexAlgorithmIvf"));

export type InferenceApplication = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:appliedRule": JsonValue;
  readonly "gmeow:appliedSubstitution"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type InferenceCommitment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:conclusion"?: ((StandpointClaim | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((StandpointClaim | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:inferenceModeOf"?: JsonValue;
  readonly "gmeow:premise": JsonValue;
  readonly [key: string]: JsonValue;
};

export type InferenceModeEnum = (string & ("gmeow:modeAbduction" | "gmeow:modeAnalogical" | "gmeow:modeDeduction" | "gmeow:modeInduction"));

export type InhabitationConfiguration = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:configurationEmbodiment"?: ((EmbodimentAssignment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((EmbodimentAssignment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:configurationOfTenure"?: ((InhabitationTenure | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((InhabitationTenure | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:duringInterval"?: ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type InhabitationLocusKindEnum = (string & ("gmeow:locusSelf" | "gmeow:locusVessel"));

export type InhabitationTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type InquiryTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:tenureQuestion"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type InscriptionReading = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:readingOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type InscriptionTranslation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:translationOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type InscriptionTransliteration = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:transliterationOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Instant = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:inTemporalFrame": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type InstrumentConfiguration = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:configurationInstrumentType"?: JsonValue;
  readonly "gmeow:configurationInterval"?: JsonValue;
  readonly "gmeow:configurationModification": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:configurationOf"?: JsonValue;
  readonly "gmeow:configurationTuningFrame"?: (TuningSystem | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type InstrumentModificationEnum = (string & ("gmeow:instrumentModificationCapo" | "gmeow:instrumentModificationElectrified" | "gmeow:instrumentModificationExtendedRange" | "gmeow:instrumentModificationMute" | "gmeow:instrumentModificationPrepared" | "gmeow:instrumentModificationScordatura"));

export type InstrumentTypeEnum = (string & ("gmeow:instrumentTypeAdaptedGuitar" | "gmeow:instrumentTypeDoubleBass" | "gmeow:instrumentTypeDrumKit" | "gmeow:instrumentTypeElectricGuitar" | "gmeow:instrumentTypeGamelan" | "gmeow:instrumentTypeModularSynth" | "gmeow:instrumentTypePiano" | "gmeow:instrumentTypeSitar" | "gmeow:instrumentTypeTabla" | "gmeow:instrumentTypeTurntables" | "gmeow:instrumentTypeViolin" | "gmeow:instrumentTypeVoice"));

export type Intention = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:intentBearer": JsonValue;
  readonly "gmeow:intentionGoal": JsonValue;
  readonly [key: string]: JsonValue;
};

export type IntentionTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:tenureAgent": JsonValue;
  readonly "gmeow:tenureIntention": JsonValue;
  readonly [key: string]: JsonValue;
};

export type InterpersonalRelationship = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:relationshipParty"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type InvitationStatusEnum = (string & ("gmeow:invitationStatusAccepted" | "gmeow:invitationStatusDeclined" | "gmeow:invitationStatusNeedsAction" | "gmeow:invitationStatusTentative"));

export type Invoice = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type InvoiceStatusEnum = (string & ("gmeow:invoiceStatusCancelled" | "gmeow:invoiceStatusDraft" | "gmeow:invoiceStatusOverdue" | "gmeow:invoiceStatusPaid" | "gmeow:invoiceStatusSent"));

export type Item = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:exemplifies": ((Manifestation | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Manifestation | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Manifestation | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type JournalEntry = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:journalEntryPostings": readonly [JsonValue, JsonValue, ...Array<JsonValue>];
  readonly [key: string]: JsonValue;
};

export type JurisdictionTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:duringInterval": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:jurisdictionPlace": ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:jurisdictionPolity": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type JustificationStatusEnum = (string & ("gmeow:justificationStatusDefeated" | "gmeow:justificationStatusGettier" | "gmeow:justificationStatusRebutted" | "gmeow:justificationStatusUndercut" | "gmeow:justificationStatusUndermined"));

export type KeySchemeEnum = (string & ("gmeow:keySchemeNostr" | "gmeow:keySchemePGP" | "gmeow:keySchemeSSH" | "gmeow:keySchemeX509"));

export type KinRelationship = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:relationshipChild": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:relationshipParent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type KnowledgeAttribution = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:attributedKnower"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:attributedProposition"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:attributingAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type KnowledgeClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:knowerAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:knownInWorld"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:knownProposition"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:underStandard"?: (EpistemicStandardEnum | readonly (EpistemicStandardEnum)[]);
  readonly [key: string]: JsonValue;
};

export type KnowledgeLevelEnum = (string & ("gmeow:knowledgeAware" | "gmeow:knowledgeKnowsAbout" | "gmeow:knowledgeMastered" | "gmeow:knowledgeUnderstands"));

export type KnowledgeProficiency = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:knowledgeProficiencyAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:knowledgeProficiencyLevel"?: (KnowledgeLevelEnum | readonly (KnowledgeLevelEnum)[]);
  readonly "gmeow:knowledgeProficiencyScale"?: (ProficiencyScaleEnum | readonly (ProficiencyScaleEnum)[]);
  readonly "gmeow:knowledgeProficiencySubject"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type LabelSetDecisionRuleEnum = (string & ("gmeow:decisionArgmax" | "gmeow:decisionIndependentThreshold"));

export type LandTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:duringInterval": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:tenureParty": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:tenurePlace": ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:tenureType": (LandTenureTypeEnum | readonly (LandTenureTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type LandTenureTypeEnum = (string & ("gmeow:tenureTypeCrownLease" | "gmeow:tenureTypeEasement" | "gmeow:tenureTypeFreehold" | "gmeow:tenureTypeLeasehold" | "gmeow:tenureTypeMortgage" | "gmeow:tenureTypeOwnership" | "gmeow:tenureTypeUsufruct"));

export type Language = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LanguageChangeTypeEnum = (string & ("gmeow:changeBorrowing" | "gmeow:changeExtinction" | "gmeow:changeGrammaticalChange" | "gmeow:changeLanguageContact" | "gmeow:changeLexicalInnovation" | "gmeow:changeMerger" | "gmeow:changeRevitalization" | "gmeow:changeRevival" | "gmeow:changeSemanticDrift" | "gmeow:changeSoundShift" | "gmeow:changeSpellingReform" | "gmeow:changeSplit" | "gmeow:changeStandardization"));

export type LanguageModalityEnum = (string & ("gmeow:modalityMachine" | "gmeow:modalityMultimodal" | "gmeow:modalitySigned" | "gmeow:modalitySpoken" | "gmeow:modalityTactile" | "gmeow:modalityWhistled" | "gmeow:modalityWritten"));

export type LanguageOriginEnum = (string & ("gmeow:originAiGenerated" | "gmeow:originConstructedArtistic" | "gmeow:originConstructedAuxiliary" | "gmeow:originConstructedEngineered" | "gmeow:originConstructedRitual" | "gmeow:originCreole" | "gmeow:originFormal" | "gmeow:originMarkup" | "gmeow:originMixed" | "gmeow:originNatural" | "gmeow:originPidgin" | "gmeow:originProgramming" | "gmeow:originQuery" | "gmeow:originReconstructed"));

export type LanguageState = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:stateAuthority"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:stateLanguage"?: ((Language | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Language | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type LanguageStatusEnum = (string & ("gmeow:statusConstructedActive" | "gmeow:statusDormant" | "gmeow:statusEmerging" | "gmeow:statusExtinct" | "gmeow:statusHistorical" | "gmeow:statusLiving" | "gmeow:statusProposed" | "gmeow:statusRevived"));

export type LanguageVarietyKindEnum = (string & ("gmeow:kindCreole" | "gmeow:kindDialect" | "gmeow:kindIdiolect" | "gmeow:kindJargon" | "gmeow:kindKoine" | "gmeow:kindLanguage" | "gmeow:kindLinguaFranca" | "gmeow:kindLocalizedVariant" | "gmeow:kindPidgin" | "gmeow:kindRegister" | "gmeow:kindSlang" | "gmeow:kindSociolect" | "gmeow:kindStandard"));

export type LearningEventTypeEnum = (string & ("gmeow:learningBeingTaught" | "gmeow:learningConceptFormation" | "gmeow:learningConsolidation" | "gmeow:learningSkillAcquisition" | "gmeow:learningTransfer" | "gmeow:learningUnlearning"));

export type LearningPathEnum = (string & ("gmeow:pathAuditAiOrGraphRag" | "gmeow:pathModelAContestedClaim" | "gmeow:pathModelAPerson" | "gmeow:pathPublishWebStructuredData" | "gmeow:pathShipOfflineGtsDocs"));

export type LedgerAccount = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:ledgerAccountHolder": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:ledgerAccountType": (LedgerAccountTypeEnum | readonly (LedgerAccountTypeEnum)[]);
  readonly [key: string]: JsonValue;
};

export type LedgerAccountTypeEnum = (string & ("gmeow:ledgerAccountTypeAsset" | "gmeow:ledgerAccountTypeEquity" | "gmeow:ledgerAccountTypeExpense" | "gmeow:ledgerAccountTypeLiability" | "gmeow:ledgerAccountTypeRevenue"));

export type LedgerEvent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:logIndex"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type LedgerFinalityStatusEnum = (string & ("gmeow:finalityStatusConfirmed" | "gmeow:finalityStatusFinalized" | "gmeow:finalityStatusOrphaned" | "gmeow:finalityStatusPending" | "gmeow:finalityStatusReorged"));

export type LedgerTransaction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:transactionHash"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type LeftOperandEnum = (string & ("gmeow:leftOpAbsolutePosition" | "gmeow:leftOpAbsoluteSize" | "gmeow:leftOpAbsoluteSpatialPosition" | "gmeow:leftOpAbsoluteTemporalPosition" | "gmeow:leftOpCount" | "gmeow:leftOpDateTime" | "gmeow:leftOpDelayPeriod" | "gmeow:leftOpDeliveryChannel" | "gmeow:leftOpDevice" | "gmeow:leftOpElapsedTime" | "gmeow:leftOpEvent" | "gmeow:leftOpFileFormat" | "gmeow:leftOpIndustry" | "gmeow:leftOpLanguage" | "gmeow:leftOpMedia" | "gmeow:leftOpMeteredTime" | "gmeow:leftOpPayAmount" | "gmeow:leftOpPercentage" | "gmeow:leftOpProduct" | "gmeow:leftOpPurpose" | "gmeow:leftOpRecipient" | "gmeow:leftOpRelativePosition" | "gmeow:leftOpRelativeSize" | "gmeow:leftOpRelativeSpatialPosition" | "gmeow:leftOpRelativeTemporalPosition" | "gmeow:leftOpResolution" | "gmeow:leftOpSpatial" | "gmeow:leftOpSpatialCoordinates" | "gmeow:leftOpSystem" | "gmeow:leftOpSystemDevice" | "gmeow:leftOpTimeInterval" | "gmeow:leftOpUnitOfCount" | "gmeow:leftOpVersion" | "gmeow:leftOpVirtualLocation"));

export type LexicalFormTypeEnum = (string & ("gmeow:formNormalized" | "gmeow:formReconstructed" | "gmeow:formRendered" | "gmeow:formSigned" | "gmeow:formSpoken" | "gmeow:formTranslated" | "gmeow:formTransliterated" | "gmeow:formWritten"));

export type License = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:licenseFamily"?: (LicenseFamilyEnum | readonly (LicenseFamilyEnum)[]);
  readonly "gmeow:licensedWork"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:licensee"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:licensor": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:spdxLicenseId"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:spdxLicenseName"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type LicenseFamilyEnum = (string & ("gmeow:licenseFamilyCC" | "gmeow:licenseFamilyCopyleft" | "gmeow:licenseFamilyDual" | "gmeow:licenseFamilyPermissive" | "gmeow:licenseFamilyProprietary" | "gmeow:licenseFamilyPublicDomain"));

export type Location = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LocationState = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:stateReferenceFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type LogicalConstraint = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:constraintLogic": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:logicConstraintMember": readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, {
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>];
  readonly [key: string]: JsonValue;
};

export type MaintenanceStatusEnum = (string & ("gmeow:statusAbandoned" | "gmeow:statusActive" | "gmeow:statusDeprecated" | "gmeow:statusEOL" | "gmeow:statusMaintained"));

export type Manifestation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:embodies": ((Expression | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Expression | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Expression | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type ManifestationFormatEnum = (string & ("gmeow:formatAudiobook" | "gmeow:formatCD" | "gmeow:formatCassette" | "gmeow:formatComicIssue" | "gmeow:formatDigitalFile" | "gmeow:formatEPUB" | "gmeow:formatHardcover" | "gmeow:formatLosslessDigitalAudio" | "gmeow:formatMEI" | "gmeow:formatMIDIFile" | "gmeow:formatMusicXML" | "gmeow:formatPDF" | "gmeow:formatPaperback" | "gmeow:formatPrintedScore" | "gmeow:formatStreamingAudio" | "gmeow:formatVinyl" | "gmeow:formatWebPage" | "gmeow:formatWebSerial"));

export type Mark = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:markText"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type MaximViolationTypeEnum = (string & ("gmeow:maximViolationManner" | "gmeow:maximViolationQuality" | "gmeow:maximViolationQuantity" | "gmeow:maximViolationRelation"));

export type Measurement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationMethod": ObservationMethodEnum;
  readonly [key: string]: JsonValue;
};

export type MediaObject = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:colourspace"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type Membership = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MemoryItem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:memoryOf": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MemoryKindEnum = (string & ("gmeow:memoryKindEpisodic" | "gmeow:memoryKindProcedural" | "gmeow:memoryKindSemantic" | "gmeow:memoryKindWorking"));

export type MentalProcessTypeEnum = (string & ("gmeow:processAffectiveExperience" | "gmeow:processAttention" | "gmeow:processAudit" | "gmeow:processDeliberation" | "gmeow:processDreaming" | "gmeow:processExport" | "gmeow:processImagining" | "gmeow:processLearning" | "gmeow:processMindWandering" | "gmeow:processPerception" | "gmeow:processReasoning" | "gmeow:processRecollection" | "gmeow:processTraining"));

export type MentalReferenceFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:isHostedBy"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Merge = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:mergeBase": JsonValue;
  readonly "gmeow:mergeSource": JsonValue;
  readonly "gmeow:mergeTarget": JsonValue;
  readonly [key: string]: JsonValue;
};

export type MessageKeywordEnum = (string & ("gmeow:keywordAnswered" | "gmeow:keywordDraft" | "gmeow:keywordFlagged" | "gmeow:keywordForwarded" | "gmeow:keywordJunk" | "gmeow:keywordSeen"));

export type MessageKindEnum = (string & ("gmeow:messageKindAutoGenerated" | "gmeow:messageKindBounce" | "gmeow:messageKindCalendarInvitation" | "gmeow:messageKindDeliveryStatusNotification" | "gmeow:messageKindFeedbackReport" | "gmeow:messageKindReadReceipt"));

export type MessageParticipant = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:participantAddress"?: {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:participantMessage": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:participantRole": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly [key: string]: JsonValue;
};

export type MessageParticipantRoleEnum = (string & ("gmeow:messageRoleBcc" | "gmeow:messageRoleCc" | "gmeow:messageRoleDeliveredTo" | "gmeow:messageRoleEnvelopeFrom" | "gmeow:messageRoleEnvelopeTo" | "gmeow:messageRoleErrorsTo" | "gmeow:messageRoleFrom" | "gmeow:messageRoleOriginalTo" | "gmeow:messageRoleReplyTo" | "gmeow:messageRoleResentCc" | "gmeow:messageRoleResentFrom" | "gmeow:messageRoleResentTo" | "gmeow:messageRoleReturnPath" | "gmeow:messageRoleSender" | "gmeow:messageRoleTo"));

export type MeterAssignment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assignedMeter": (MetricStructure | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:assignmentSpan": (MusicalTimeSpan | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:meterCarrier": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MetricGroup = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:groupAccentWeight"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:groupLengthDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:groupLengthNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:metricGroupOrder": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MetricKindEnum = (string & ("gmeow:metricCosine" | "gmeow:metricEditDistance" | "gmeow:metricEuclidean" | "gmeow:metricGeodesic" | "gmeow:metricGraphHops" | "gmeow:metricLogarithmic" | "gmeow:metricPositionalDistance" | "gmeow:metricSymplectic"));

export type MetricModulation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:modulationFromFrame": (MusicalTimeFrame | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:modulationToFrame": (MusicalTimeFrame | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:pivotSourceValue": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:pivotTargetValue": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MetricStructure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasMetricGroup": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Mitigation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:mitigationCounters": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:mitigationMeasure": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:mitigationStatus"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MitigationStatusEnum = (string & ("gmeow:mitigationActive" | "gmeow:mitigationProposed" | "gmeow:mitigationRetired"));

export type ModalForceEnum = (string & ("gmeow:modalForceActual" | "gmeow:modalForceCounterfactual" | "gmeow:modalForceNecessary" | "gmeow:modalForcePossible"));

export type ModelCard = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:describesModelArtifact"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:describesModelService"?: ((ModelDeployment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ModelDeployment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ModelDeployment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:deploymentArtifact"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:deploymentHost"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:deploymentService"?: ((SoftwareAgent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SoftwareAgent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ModelInferenceRun = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:modelIdentifier": (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly [({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string), ...Array<({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string)>]);
  readonly "gmeow:modelRevision": (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly [({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string), ...Array<({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string)>]);
  readonly [key: string]: JsonValue;
};

export type MonetaryAmount = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:currency"?: ((ReferenceFrame | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:monetaryValue": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type Motif = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:motifOccursIn"?: ((ContentSegment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ContentSegment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "rdfs:label": (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly [({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string), ...Array<({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string)>]);
  readonly [key: string]: JsonValue;
};

export type MotifKindEnum = (string & ("gmeow:motifKindLeitmotif" | "gmeow:motifKindRunningGag" | "gmeow:motifKindSymbol" | "gmeow:motifKindTaleType" | "gmeow:motifKindTheme" | "gmeow:motifKindTrope"));

export type MultipartTypeEnum = (string & ("gmeow:multipartTypeAlternative" | "gmeow:multipartTypeDigest" | "gmeow:multipartTypeEncrypted" | "gmeow:multipartTypeMixed" | "gmeow:multipartTypeParallel" | "gmeow:multipartTypeRelated" | "gmeow:multipartTypeReport" | "gmeow:multipartTypeSigned"));

export type MusicAnalysisClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:analysisFrame"?: ((ReferenceFrame | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:analysisProperty"?: (AnalysisPropertyEnum | readonly (AnalysisPropertyEnum)[]);
  readonly "gmeow:analysisResult": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:analysisTarget": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:vantage": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MusicalParameterEnum = (string & ("gmeow:musicalParameterDuration" | "gmeow:musicalParameterDynamics" | "gmeow:musicalParameterInstrumentation" | "gmeow:musicalParameterLocation" | "gmeow:musicalParameterOrder" | "gmeow:musicalParameterPerformerCount" | "gmeow:musicalParameterPitch" | "gmeow:musicalParameterSoundContent" | "gmeow:musicalParameterTacet" | "gmeow:musicalParameterTempo" | "gmeow:musicalParameterTimbre"));

export type MusicalSegment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:segmentKind": JsonValue;
  readonly "gmeow:segmentSpan"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MusicalTimeFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:dimensionCount": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:frameKind": FrameKindEnum;
  readonly "gmeow:frameRealm": FrameRealmEnum;
  readonly "gmeow:hasAxis": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:requiresHost": (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MusicalTimeSpan = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasMusicalTimeFrame": (MusicalTimeFrame | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:timeDurationDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:timeDurationNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:timeStartDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:timeStartNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type Myth = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:affectedConsumerSurface"?: (ProjectionContextEnum | readonly (ProjectionContextEnum)[]);
  readonly "gmeow:hasMythTelling"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:recurringRisk"?: (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type NamePartTypeEnum = (string & ("gmeow:namePartAgnomen" | "gmeow:namePartBirthOrderName" | "gmeow:namePartBirthSurname" | "gmeow:namePartClanName" | "gmeow:namePartCognomen" | "gmeow:namePartCourtesyName" | "gmeow:namePartExtension" | "gmeow:namePartGenerationName" | "gmeow:namePartGenerationalOrdinal" | "gmeow:namePartGenerationalSuffix" | "gmeow:namePartGiven" | "gmeow:namePartHonorificPrefix" | "gmeow:namePartHonorificSuffix" | "gmeow:namePartHouseName" | "gmeow:namePartInitial" | "gmeow:namePartIsm" | "gmeow:namePartKunya" | "gmeow:namePartLaqab" | "gmeow:namePartMaternalSurname" | "gmeow:namePartMatronymic" | "gmeow:namePartMiddle" | "gmeow:namePartMononym" | "gmeow:namePartNasab" | "gmeow:namePartNickname" | "gmeow:namePartNisba" | "gmeow:namePartNomen" | "gmeow:namePartParticle" | "gmeow:namePartPaternalSurname" | "gmeow:namePartPatronymic" | "gmeow:namePartPraenomen" | "gmeow:namePartReligiousName" | "gmeow:namePartStem" | "gmeow:namePartSurname" | "gmeow:namePartTeknonym"));

export type NamePurposeEnum = (string & ("gmeow:namePurposeBirth" | "gmeow:namePurposeCeremonial" | "gmeow:namePurposeChosen" | "gmeow:namePurposeDeadname" | "gmeow:namePurposeEndonym" | "gmeow:namePurposeExonym" | "gmeow:namePurposeGlossonym" | "gmeow:namePurposeLegal" | "gmeow:namePurposeNickname" | "gmeow:namePurposeOnlineHandle" | "gmeow:namePurposePenStage" | "gmeow:namePurposeProfessional" | "gmeow:namePurposeRegnal" | "gmeow:namePurposeReligious" | "gmeow:namePurposeSuperseded"));

export type NameRegisterEnum = (string & ("gmeow:registerCasual" | "gmeow:registerFormal" | "gmeow:registerIntimate" | "gmeow:registerProfessional"));

export type NameUsage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:usageAppellation": (Appellation | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:usageNamed": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type NamedPeriod = ({
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
} & (({
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly [key: string]: JsonValue;
  } & ({
      readonly "@annotation"?: Annotation;
      readonly "@id"?: string;
      readonly "@type"?: (string | readonly (string)[]);
      readonly "gmeow:periodStart": ((Instant | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }) | readonly [(Instant | {
                  readonly "@id": string;
                  readonly [key: string]: JsonValue;
                }), ...Array<(Instant | {
                  readonly "@id": string;
                  readonly [key: string]: JsonValue;
                })>]);
      readonly [key: string]: JsonValue;
    } & {
      readonly "@annotation"?: Annotation;
      readonly "@id"?: string;
      readonly "@type"?: (string | readonly (string)[]);
      readonly "gmeow:periodEnd": ((Instant | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }) | readonly [(Instant | {
                  readonly "@id": string;
                  readonly [key: string]: JsonValue;
                }), ...Array<(Instant | {
                  readonly "@id": string;
                  readonly [key: string]: JsonValue;
                })>]);
      readonly [key: string]: JsonValue;
    })) | {
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "gmeow:hasTemporalMeasurement": ((TemporalMeasurement | {
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }) | readonly [(TemporalMeasurement | {
                readonly "@id": string;
                readonly [key: string]: JsonValue;
              }), ...Array<(TemporalMeasurement | {
                readonly "@id": string;
                readonly [key: string]: JsonValue;
              })>]);
    readonly [key: string]: JsonValue;
  }));

export type NarrationModeEnum = (string & ("gmeow:narrationDirect" | "gmeow:narrationDream" | "gmeow:narrationFlashback" | "gmeow:narrationHypothetical" | "gmeow:narrationMentioned" | "gmeow:narrationUnreliable"));

export type NarrationUsage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:narrationMode": (NarrationModeEnum | readonly (NarrationModeEnum)[]);
  readonly "gmeow:narrationSegment": ((ContentSegment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(ContentSegment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(ContentSegment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:narrationSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type NarrativeFrameLink = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:narrativeFrameLinkRelation": (NarrativeFrameRelationEnum | readonly (NarrativeFrameRelationEnum)[]);
  readonly "gmeow:narrativeFrameLinkSource": ((NarrativeReferenceFrame | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(NarrativeReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(NarrativeReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:narrativeFrameLinkTarget": ((NarrativeReferenceFrame | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(NarrativeReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(NarrativeReferenceFrame | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type NarrativeFrameRelationEnum = (string & ("gmeow:relationAdaptationOf" | "gmeow:relationAlternateContinuity" | "gmeow:relationCanon" | "gmeow:relationCrossover" | "gmeow:relationExpandedUniverse" | "gmeow:relationFanon"));

export type NarrativePosition = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:positionFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type NarrativeReferenceFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:frameKind": (FrameKindEnum | readonly (FrameKindEnum)[]);
  readonly "gmeow:frameRealm": (FrameRealmEnum | readonly (FrameRealmEnum)[]);
  readonly [key: string]: JsonValue;
};

export type NarrativeRoleEnum = (string & ("gmeow:roleAntagonist" | "gmeow:roleConfidant" | "gmeow:roleFoil" | "gmeow:roleLoveInterest" | "gmeow:roleMentor" | "gmeow:roleNarratingVoice" | "gmeow:roleProtagonist" | "gmeow:roleTrickster"));

export type NarrativeTimeAxisEnum = (string & ("gmeow:axisDiscourseTime" | "gmeow:axisStoryTime"));

export type NarrativeTimeFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type NeoRiemannianOperationEnum = (string & ("gmeow:neoRiemannianL" | "gmeow:neoRiemannianN" | "gmeow:neoRiemannianP" | "gmeow:neoRiemannianR" | "gmeow:neoRiemannianS"));

export type NetworkAddress = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:networkAddressFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type NetworkAddressTypeEnum = (string & ("gmeow:networkAddressTypeBGP" | "gmeow:networkAddressTypeDNS" | "gmeow:networkAddressTypeIPv4" | "gmeow:networkAddressTypeIPv6" | "gmeow:networkAddressTypeMAC" | "gmeow:networkAddressTypePort" | "gmeow:networkAddressTypeURL"));

/**
 * Validated by @type: a node typed gmeow:Foo MUST satisfy #/$defs/Foo (closed-world). Nodes typed only by unmodeled classes are permissively allowed.
 */
export type Node = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Norm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:deonticModality"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type NotationProjectionProfile = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:notationSystemOf"?: ((NotationSystem | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((NotationSystem | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:projectionFunction": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type NotationSystem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:smuflCodepoint"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type NotationSystemUsage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:notationUsageInterval": ((TimeInterval | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(TimeInterval | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:notationUsageNotationSystem": ((NotationSystem | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(NotationSystem | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(NotationSystem | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:notationUsageRole": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly "gmeow:notationUsageTarget": ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type NotationUsageRoleEnum = (string & ("gmeow:notationRoleCipher" | "gmeow:notationRoleCommunication" | "gmeow:notationRoleEncoding" | "gmeow:notationRoleExpression" | "gmeow:notationRoleRepresentation" | "gmeow:notationRoleShorthand" | "gmeow:notationRoleTranscription"));

export type Note = ({
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
} & ({
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "rdfs:label": JsonValue;
    readonly [key: string]: JsonValue;
  } | {
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "gmeow:noteContent": JsonValue;
    readonly [key: string]: JsonValue;
  }));

export type ObservablePropertyEnum = (string & ("gmeow:observablePropertyAirQualityIndex" | "gmeow:observablePropertyAtmosphericPressure" | "gmeow:observablePropertyHumidity" | "gmeow:observablePropertyLightIntensity" | "gmeow:observablePropertyLoudness" | "gmeow:observablePropertyRadiationLevel" | "gmeow:observablePropertyRoughness" | "gmeow:observablePropertySoundPressureLevel" | "gmeow:observablePropertyTemperature" | "gmeow:observablePropertyTimbre" | "gmeow:observablePropertyTimingDeviation"));

export type Observation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observedAt"?: (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:observedFeature"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ObservationMethodEnum = (string & ("gmeow:methodComputationalModel" | "gmeow:methodDirectObservation" | "gmeow:methodExpertJudgement" | "gmeow:methodGNSSRTK" | "gmeow:methodGPS" | "gmeow:methodInstrumentalReading" | "gmeow:methodLiDAR" | "gmeow:methodLlmExtraction" | "gmeow:methodNliDerivation" | "gmeow:methodPhotogrammetry" | "gmeow:methodRemoteSensing" | "gmeow:methodStreaming" | "gmeow:methodSurvey" | "gmeow:methodTotalStation"));

export type ObservationTypeEnum = (string & ("gmeow:observationTypeDerived" | "gmeow:observationTypeIdentity" | "gmeow:observationTypeKinship" | "gmeow:observationTypeMeasurement" | "gmeow:observationTypeNaming" | "gmeow:observationTypeRights" | "gmeow:observationTypeSensory" | "gmeow:observationTypeSimulation" | "gmeow:observationTypeStandpoint" | "gmeow:observationTypeStreaming"));

export type OpacityReasonEnum = (string & ("gmeow:opacityDamaged" | "gmeow:opacityMissingKey" | "gmeow:opacityUnknownCodec"));

export type OpaqueFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:opacityReason"?: (OpacityReasonEnum | readonly (OpacityReasonEnum)[]);
  readonly "gmeow:opaqueFrameIn"?: ((GTSSegment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((GTSSegment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Order = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type OrderStatusEnum = (string & ("gmeow:orderStatusCancelled" | "gmeow:orderStatusConfirmed" | "gmeow:orderStatusDelivered" | "gmeow:orderStatusPending" | "gmeow:orderStatusShipped"));

export type Organization = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type OrganizationTypeEnum = (string & ("gmeow:organizationTypeAssociation" | "gmeow:organizationTypeCollaboration" | "gmeow:organizationTypeCompany" | "gmeow:organizationTypeEducationalInstitution" | "gmeow:organizationTypeGovernmentBody" | "gmeow:organizationTypeNonprofit"));

export type Orientation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type OrnamentProfile = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:ornamentProfileKind": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:ornamentReferenceFrame"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type OrnamentProfileKindEnum = (string & ("gmeow:ornamentProfileKindBaroqueAgrement" | "gmeow:ornamentProfileKindGamaka" | "gmeow:ornamentProfileKindGraceNote" | "gmeow:ornamentProfileKindJazzTurn" | "gmeow:ornamentProfileKindMordent"));

export type ParentChildRelationship = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:relationshipChild"?: ((Person | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Person | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:relationshipParent"?: ((Person | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Person | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ParticipantRoleEnum = (string & ("gmeow:roleAccompanist" | "gmeow:roleAgent" | "gmeow:roleAttendee" | "gmeow:roleBeneficiary" | "gmeow:roleBeneficiaryOfDeception" | "gmeow:roleConductor" | "gmeow:roleDeceived" | "gmeow:roleDeceiver" | "gmeow:roleDupe" | "gmeow:roleEmployee" | "gmeow:roleEmployer" | "gmeow:roleEnsembleMember" | "gmeow:roleImproviser" | "gmeow:roleIntermediary" | "gmeow:roleLearner" | "gmeow:roleOfficiant" | "gmeow:roleOrganizer" | "gmeow:roleParticipantPrincipal" | "gmeow:rolePayee" | "gmeow:rolePayer" | "gmeow:rolePerformer" | "gmeow:roleProducer" | "gmeow:roleSessionMusician" | "gmeow:roleSoloist" | "gmeow:roleSpinDoctor" | "gmeow:roleTransmitter" | "gmeow:roleVictim" | "gmeow:roleWitness"));

export type Participation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:participationEvent": (Event | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:participationParticipant": ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type PaymentMethodEnum = (string & ("gmeow:paymentMethodBankTransfer" | "gmeow:paymentMethodCash" | "gmeow:paymentMethodCheque" | "gmeow:paymentMethodCreditCard" | "gmeow:paymentMethodCrypto"));

export type PerformanceDecision = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:decisionConstraint": (TraversalConstraint | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:decisionPerformance": (Event | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:decisionSequence": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type PerformanceParticipation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:participationConfiguration"?: JsonValue;
  readonly "gmeow:participationEvent": (Event | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:participationInstrumentItem"?: JsonValue;
  readonly "gmeow:participationPart"?: JsonValue;
  readonly "gmeow:participationParticipant": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:participationRole": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:participationTechnique"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type PeriodTypeEnum = (string & ("gmeow:periodTypeFiscalYear" | "gmeow:periodTypeGeologicAge" | "gmeow:periodTypeGeologicEon" | "gmeow:periodTypeGeologicEpoch" | "gmeow:periodTypeGeologicEra" | "gmeow:periodTypeGeologicPeriod" | "gmeow:periodTypeHistoricalDynasty" | "gmeow:periodTypeHistoricalEra"));

export type Permission = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Person = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type PersonName = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Persona = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:expressesNorm"?: ((Norm | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Norm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:personaBearer": JsonValue;
  readonly "gmeow:personaRegister": JsonValue;
  readonly [key: string]: JsonValue;
};

export type PhysicalCarrierTypeEnum = (string & ("gmeow:carrierBone" | "gmeow:carrierCoin" | "gmeow:carrierManuscript" | "gmeow:carrierMetal" | "gmeow:carrierOstracon" | "gmeow:carrierPapyrus" | "gmeow:carrierPotterySherd" | "gmeow:carrierSeal" | "gmeow:carrierStela" | "gmeow:carrierTablet" | "gmeow:carrierWallInscription" | "gmeow:carrierWood"));

export type PhysicalObject = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Pipeline = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasStage"?: ((PipelineStage | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PipelineStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type PipelineStage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:dataflowConsumes"?: ((PipelineStage | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((PipelineStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:hasCapability"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:requiresResource"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:stageImpl": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type PitchAnchor = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:anchorDegree": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:anchorFrequency": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:pitchAnchorOf": (TuningSystem | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type PitchCollection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:collectionKind": PitchCollectionKindEnum;
  readonly "gmeow:collectionPartOrder"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:derivedFromSpectrum"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type PitchCollectionKindEnum = (string & ("gmeow:pitchCollectionKindJins" | "gmeow:pitchCollectionKindMaqam" | "gmeow:pitchCollectionKindMode" | "gmeow:pitchCollectionKindModeOfLimitedTransposition" | "gmeow:pitchCollectionKindPCSet" | "gmeow:pitchCollectionKindPathet" | "gmeow:pitchCollectionKindRaga" | "gmeow:pitchCollectionKindRowSeries" | "gmeow:pitchCollectionKindScale" | "gmeow:pitchCollectionKindSpectrumCollection" | "gmeow:pitchCollectionKindThaat"));

export type PitchCollectionMembership = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:membershipCollection": (PitchCollection | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:membershipDegreeIndex"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:membershipPitch": (PitchValue | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:membershipRole": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly [key: string]: JsonValue;
};

export type PitchExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasTuningFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type PitchInterval = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasTuningFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:ratioDenominator"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type PitchSpelling = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:spelledName": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:spellingPitch": (PitchValue | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:spellingSystem": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type PitchTrajectory = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:interpolationKind": JsonValue;
  readonly "gmeow:trajectoryControlPoint": readonly [JsonValue, JsonValue, ...Array<JsonValue>];
  readonly [key: string]: JsonValue;
};

export type PitchTrajectoryControlPoint = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:controlPointOfTrajectory": (PitchTrajectory | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:controlPointOrder": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:controlPointPitch": (PitchValue | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:controlPointTimeFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:controlPointTimePositionDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:controlPointTimePositionNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type PitchTrajectoryInterpolationKindEnum = (string & ("gmeow:interpolationExponential" | "gmeow:interpolationLinearCents" | "gmeow:interpolationStochasticByReference"));

export type PitchValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasTuningFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:ratioDenominator"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Place = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasCentroid"?: {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type PlaceTypeEnum = (string & ("gmeow:placeTypeAdministrativeArea" | "gmeow:placeTypeBuilding" | "gmeow:placeTypeCity" | "gmeow:placeTypeCountry" | "gmeow:placeTypeFloor" | "gmeow:placeTypeNaturalFeature" | "gmeow:placeTypeNeighborhood" | "gmeow:placeTypeParcel" | "gmeow:placeTypePointOfInterest" | "gmeow:placeTypePremises" | "gmeow:placeTypeRegion" | "gmeow:placeTypeRoom" | "gmeow:placeTypeSite" | "gmeow:placeTypeThoroughfare"));

export type PlayingTechniqueEnum = (string & ("gmeow:playingTechniqueArco" | "gmeow:playingTechniqueBentNote" | "gmeow:playingTechniqueColLegno" | "gmeow:playingTechniqueGrowl" | "gmeow:playingTechniqueHarmonics" | "gmeow:playingTechniqueKonnakol" | "gmeow:playingTechniqueMultiphonics" | "gmeow:playingTechniquePizzicato" | "gmeow:playingTechniquePreparedPiano" | "gmeow:playingTechniqueSlap" | "gmeow:playingTechniqueTapping"));

export type PolarityEnum = (string & ("gmeow:polarityAffirm" | "gmeow:polarityDeny" | "gmeow:polaritySuspend"));

export type Pose = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasPoseOrientation": (Orientation | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:hasPosePosition": (SpatialCoordinates | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type Post = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:postIn"?: ((Organization | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Organization | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type PostalAddress = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:postalAddressFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type Posting = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:postingAccount": ((LedgerAccount | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(LedgerAccount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(LedgerAccount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:postingAmount": ((MonetaryAmount | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MonetaryAmount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MonetaryAmount | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:postingDirection": (PostingDirectionEnum | readonly (PostingDirectionEnum)[]);
  readonly "gmeow:postingJournalEntry": ((JournalEntry | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(JournalEntry | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(JournalEntry | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type PostingDirectionEnum = (string & ("gmeow:postingDirectionCredit" | "gmeow:postingDirectionDebit"));

export type PrecedenceTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:precedenceHigher": ((Norm | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Norm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Norm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:precedenceLower": ((Norm | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Norm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Norm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:precedenceScope": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type PremiseUse = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:premiseUsed": JsonValue;
  readonly [key: string]: JsonValue;
};

export type PrivacyNotice = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Procedure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasProcedureStep"?: ((ProcedureStep | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((ProcedureStep | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type ProcedureStep = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ProcedureTypeEnum = (string & ("gmeow:procedureTypeAgentFlow" | "gmeow:procedureTypeBusinessProcess" | "gmeow:procedureTypeCiBuild" | "gmeow:procedureTypeDataPipeline" | "gmeow:procedureTypeIngestion" | "gmeow:procedureTypeLabProtocol" | "gmeow:procedureTypeRecipe" | "gmeow:procedureTypeResearchPlan"));

export type ProficiencyLevelEnum = (string & ("gmeow:assessedBeginner" | "gmeow:assessedCompetent" | "gmeow:assessedExpert" | "gmeow:cefrA1" | "gmeow:cefrA2" | "gmeow:cefrB1" | "gmeow:cefrB2" | "gmeow:cefrC1" | "gmeow:cefrC2" | "gmeow:dreyfusAdvancedBeginner" | "gmeow:dreyfusCompetent" | "gmeow:dreyfusExpert" | "gmeow:dreyfusNovice" | "gmeow:dreyfusProficient" | "gmeow:levelHeritage" | "gmeow:levelNative" | "gmeow:nihAdvanced" | "gmeow:nihBeginner" | "gmeow:nihExpert" | "gmeow:nihIntermediate"));

export type ProficiencyModalityEnum = (string & ("gmeow:profModalityComprehension" | "gmeow:profModalityListening" | "gmeow:profModalityOverall" | "gmeow:profModalityReading" | "gmeow:profModalitySigning" | "gmeow:profModalitySpeaking" | "gmeow:profModalityWriting"));

export type ProficiencyScaleEnum = (string & ("gmeow:scaleACTFL" | "gmeow:scaleAssessed" | "gmeow:scaleBloomRevised" | "gmeow:scaleCEFR" | "gmeow:scaleDreyfus" | "gmeow:scaleILR" | "gmeow:scaleKnowledgeDepth" | "gmeow:scaleNIH" | "gmeow:scaleSOLO" | "gmeow:scaleSelfReported"));

export type Profile = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:profileAppliesTo"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:profileDescriptor": JsonValue;
  readonly "http://www.w3.org/2004/02/skos/core#definition": JsonValue;
  readonly "rdfs:label": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Prohibition = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:ruleAction": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Project = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ProjectionContextEnum = (string & ("gmeow:consumerAdviceCatalog" | "gmeow:consumerAgentMemory" | "gmeow:consumerFoafExport" | "gmeow:consumerInternalArchive" | "gmeow:consumerPublicSite" | "gmeow:consumerResearchQueue" | "gmeow:consumerSchemaOrgJsonLd" | "gmeow:consumerWikidata" | "gmeow:consumerWikipedia"));

export type ProjectionLossEnum = (string & ("gmeow:lossDropsDynamics" | "gmeow:lossDropsInstrumentation" | "gmeow:lossDropsMicrotiming" | "gmeow:lossDropsPerformerCount" | "gmeow:lossDropsSpatialSoundContext" | "gmeow:lossDropsSpectralDerivation" | "gmeow:lossDropsTacet" | "gmeow:lossDropsTimbre" | "gmeow:lossDropsTraversalConstraints" | "gmeow:lossDropsTuningFrame" | "gmeow:lossQuantizesPitchTo12Edo" | "gmeow:lossQuantizesTimeToRationalGrid" | "gmeow:lossSymbolizesContinuousTrajectory"));

export type PromptRoleEnum = (string & ("gmeow:promptRoleAssistant" | "gmeow:promptRoleSystem" | "gmeow:promptRoleTool" | "gmeow:promptRoleUser"));

export type PronounSet = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type ProximityMeasurement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationResult": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:observedFeature": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:proximityTo": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type QualityAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:assessedEntity"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type QualityAxisEnum = (string & ("gmeow:axisDocMaturity" | "gmeow:axisDocumentation" | "gmeow:axisFlagshipCounterExampleDepth" | "gmeow:axisGmn1Coverage" | "gmeow:axisGmnGlyphOptimality" | "gmeow:axisMaximalGrounding" | "gmeow:axisMaximalInformation" | "gmeow:axisMaximalLinkage" | "gmeow:axisMaximalProjection" | "gmeow:axisOptimalTesting" | "gmeow:axisProseQuality" | "gmeow:axisProvenanceHonesty" | "gmeow:axisReasonerDerived" | "gmeow:axisShapeMigration" | "gmeow:axisTranslationCoverage"));

export type QualityDimensionEnum = (string & ("gmeow:qualityDimensionCompleteness" | "gmeow:qualityDimensionLineage" | "gmeow:qualityDimensionLogicalConsistency" | "gmeow:qualityDimensionPositionalAccuracy" | "gmeow:qualityDimensionTemporalAccuracy" | "gmeow:qualityDimensionThematicAccuracy" | "gmeow:qualityDimensionTopologicalConsistency"));

export type QualityTierEnum = (string & ("gmeow:tierExemplified" | "gmeow:tierGrounded" | "gmeow:tierLinked" | "gmeow:tierMaximal" | "gmeow:tierRegistered"));

export type QuestionTypeEnum = (string & ("gmeow:typeAlternative" | "gmeow:typeHow" | "gmeow:typePolar" | "gmeow:typeWh" | "gmeow:typeWhy"));

export type ReadingOrder = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type RealizationModeEnum = (string & ("gmeow:realizationModeImprovised" | "gmeow:realizationModeMachineGenerated" | "gmeow:realizationModeNotated" | "gmeow:realizationModeOral" | "gmeow:realizationModePerformed"));

export type RecipeEnum = (string & ("gmeow:recipeContestedOrAttributedFacts" | "gmeow:recipeDocumentsAndSchemaOrg" | "gmeow:recipeEventsAndParticipants" | "gmeow:recipeGraphRagDatasetLineage" | "gmeow:recipeOfflineGtsDistribution" | "gmeow:recipePersonNamesAndDisplay"));

export type ReferenceFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type RegionSelector = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:selectorType": SelectorTypeEnum;
  readonly "gmeow:selectorValue": ({
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      } | string);
  readonly [key: string]: JsonValue;
};

export type RegisterEnum = (string & ("gmeow:registerBrandVoice" | "gmeow:registerCeremonial" | "gmeow:registerClinical" | "gmeow:registerPrivate" | "gmeow:registerPublic"));

export type RegulatoryOverlay = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:duringInterval": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:overlayAuthority": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "gmeow:overlayPlace": ((Place | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Place | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type RegulatoryOverlayTypeEnum = (string & ("gmeow:overlayTypeAerodromeTrafficZone" | "gmeow:overlayTypeAirway" | "gmeow:overlayTypeAlertArea" | "gmeow:overlayTypeCivilTimeZone" | "gmeow:overlayTypeContiguousZone" | "gmeow:overlayTypeContinentalShelf" | "gmeow:overlayTypeControlZone" | "gmeow:overlayTypeCustomsZone" | "gmeow:overlayTypeElectoralDistrict" | "gmeow:overlayTypeFishingZone" | "gmeow:overlayTypeFlightInformationRegion" | "gmeow:overlayTypeHighSeas" | "gmeow:overlayTypeMarineProtectedArea" | "gmeow:overlayTypeMilitaryOperationsArea" | "gmeow:overlayTypeNOTAM" | "gmeow:overlayTypePostalZone" | "gmeow:overlayTypeProtectedArea" | "gmeow:overlayTypeRestrictedAirspace" | "gmeow:overlayTypeSanctions" | "gmeow:overlayTypeTaxDistrict" | "gmeow:overlayTypeTerminalControlArea" | "gmeow:overlayTypeTerritorialSea" | "gmeow:overlayTypeWarningArea" | "gmeow:overlayTypeZoning"));

export type Release = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:releaseOf"?: ((Project | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Project | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type Reminder = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:reminderAction": ReminderActionEnum;
  readonly "gmeow:reminderTarget": (Event | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:reminderTrigger": JsonValue;
  readonly [key: string]: JsonValue;
};

export type ReminderActionEnum = (string & ("gmeow:reminderActionAudio" | "gmeow:reminderActionDisplay" | "gmeow:reminderActionEmail"));

export type Repository = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:repositoryType": JsonValue;
  readonly [key: string]: JsonValue;
};

export type RepositoryTypeEnum = (string & ("gmeow:repoTypeFossil" | "gmeow:repoTypeGit" | "gmeow:repoTypeHg" | "gmeow:repoTypeJJ" | "gmeow:repoTypePijul" | "gmeow:repoTypeSVN"));

export type RetrievalEvent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:againstIndex": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type RightsActionEnum = (string & ("gmeow:actionAcceptTracking" | "gmeow:actionAggregate" | "gmeow:actionAnnotate" | "gmeow:actionAnonymize" | "gmeow:actionArchive" | "gmeow:actionAttribute" | "gmeow:actionCommercialize" | "gmeow:actionCompensate" | "gmeow:actionConcurrentUse" | "gmeow:actionDelete" | "gmeow:actionDerive" | "gmeow:actionDigitize" | "gmeow:actionDisplay" | "gmeow:actionDistribute" | "gmeow:actionEnsureExclusivity" | "gmeow:actionExecute" | "gmeow:actionExtract" | "gmeow:actionGive" | "gmeow:actionGrantUse" | "gmeow:actionInclude" | "gmeow:actionIndex" | "gmeow:actionInform" | "gmeow:actionInstall" | "gmeow:actionLease" | "gmeow:actionLend" | "gmeow:actionModify" | "gmeow:actionMove" | "gmeow:actionNextPolicy" | "gmeow:actionObtainConsent" | "gmeow:actionPlay" | "gmeow:actionPresent" | "gmeow:actionPrint" | "gmeow:actionProcessPersonalData" | "gmeow:actionRead" | "gmeow:actionReproduce" | "gmeow:actionRetainNotice" | "gmeow:actionReviewPolicy" | "gmeow:actionSell" | "gmeow:actionShareAlike" | "gmeow:actionStream" | "gmeow:actionSynchronize" | "gmeow:actionTextToSpeech" | "gmeow:actionTransfer" | "gmeow:actionTransform" | "gmeow:actionTranslate" | "gmeow:actionUninstall" | "gmeow:actionUse" | "gmeow:actionWatermark"));

export type RightsStatement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type RightsTypeEnum = (string & ("gmeow:rightsTypeCopyright" | "gmeow:rightsTypeDatabaseRight" | "gmeow:rightsTypeIndustrialDesign" | "gmeow:rightsTypeMoralRights" | "gmeow:rightsTypePatent" | "gmeow:rightsTypePlantBreedersRights" | "gmeow:rightsTypeRelatedRights" | "gmeow:rightsTypeTradeSecret" | "gmeow:rightsTypeTrademark"));

export type RoleInNarrative = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:narrativeRoleBearer": JsonValue;
  readonly "gmeow:narrativeRoleScope": JsonValue;
  readonly "gmeow:narrativeRoleValue": JsonValue;
  readonly [key: string]: JsonValue;
};

export type RomanticOrientation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:facetSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:facetVantage": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:romanticOrientationValue": ((RomanticOrientationValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(RomanticOrientationValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(RomanticOrientationValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type RomanticOrientationValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type RomanticOrientationValueEnum = (string & ("gmeow:romanticAromantic" | "gmeow:romanticBiromantic" | "gmeow:romanticDemiromantic" | "gmeow:romanticHeteroromantic" | "gmeow:romanticHomoromantic" | "gmeow:romanticPanromantic" | "gmeow:romanticQueerromantic" | "gmeow:romanticQuestioning"));

export type RouteKindEnum = (string & ("gmeow:routeKindAccessible" | "gmeow:routeKindCitation" | "gmeow:routeKindDependency" | "gmeow:routeKindFlight" | "gmeow:routeKindNetwork" | "gmeow:routeKindSocial" | "gmeow:routeKindTransit" | "gmeow:routeKindWalking"));

export type RsvpStatusEnum = (string & ("gmeow:rsvpStatusAccepted" | "gmeow:rsvpStatusDeclined" | "gmeow:rsvpStatusNeedsAction" | "gmeow:rsvpStatusTentative"));

export type Rule = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:ruleAction"?: (RightsActionEnum | readonly (RightsActionEnum)[]);
  readonly [key: string]: JsonValue;
};

export type SLSALevelEnum = (string & ("gmeow:slsaLevel1" | "gmeow:slsaLevel2" | "gmeow:slsaLevel3" | "gmeow:slsaLevel4"));

export type ScalePolarityEnum = (string & ("gmeow:polarityBipolar" | "gmeow:polarityUnipolar"));

export type SceneGraphEdge = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:sceneConfidence"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:sceneObject": (ImageRegion | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:sceneRelation": SceneRelationTypeEnum;
  readonly "gmeow:sceneSubject": (ImageRegion | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type SceneRelationTypeEnum = (string & ("gmeow:sceneRelationAbove" | "gmeow:sceneRelationBelow" | "gmeow:sceneRelationEating" | "gmeow:sceneRelationFarFrom" | "gmeow:sceneRelationHolding" | "gmeow:sceneRelationInside" | "gmeow:sceneRelationLeftOf" | "gmeow:sceneRelationNear" | "gmeow:sceneRelationPartOf" | "gmeow:sceneRelationPlaying" | "gmeow:sceneRelationRiding" | "gmeow:sceneRelationRightOf" | "gmeow:sceneRelationSameAs" | "gmeow:sceneRelationTouching" | "gmeow:sceneRelationWearing"));

export type ScheduleException = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:exceptionOriginalDate"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:exceptionSchedule"?: ((EventSchedule | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((EventSchedule | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SchenkerLevelEnum = (string & ("gmeow:schenkerLevelBackground" | "gmeow:schenkerLevelForeground" | "gmeow:schenkerLevelMiddleground"));

export type ScoreAnchor = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:anchorMeaning": JsonValue;
  readonly "gmeow:anchorRangeMax": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:anchorRangeMin": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type ScoreScale = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:scaleMax": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:scaleMin": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:scaleStep"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type ScoreSemanticsEnum = (string & ("gmeow:scoreCalibratedProbability" | "gmeow:scoreEntailment" | "gmeow:scoreLogit" | "gmeow:scoreMargin" | "gmeow:scoreSigmoid" | "gmeow:scoreSoftmax"));

export type ScriptLanguageAttribution = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:attributionTarget"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SegmentKind = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "rdfs:subClassOf"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type SegmentKindEnum = (string & ("gmeow:segmentKindCell" | "gmeow:segmentKindColor" | "gmeow:segmentKindDrone" | "gmeow:segmentKindFragment" | "gmeow:segmentKindLoop" | "gmeow:segmentKindMotif" | "gmeow:segmentKindPhrase" | "gmeow:segmentKindRiff" | "gmeow:segmentKindSection" | "gmeow:segmentKindTalea" | "gmeow:segmentKindToneEventContainer"));

export type SegmentTransformation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:transformationParameter"?: JsonValue;
  readonly "gmeow:transformationSource": (MusicalSegment | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:transformationTarget": (MusicalSegment | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:transformationType": (TransformationType | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type SelectorTypeEnum = (string & ("gmeow:selectorTypeCocoRleMask" | "gmeow:selectorTypeDicomSegMask" | "gmeow:selectorTypeFractionalRectangle" | "gmeow:selectorTypePixelMask" | "gmeow:selectorTypePixelRectangle" | "gmeow:selectorTypePolygonPath" | "gmeow:selectorTypeRunLengthEncoded" | "gmeow:selectorTypeSvgPath" | "gmeow:selectorTypeWebAnnotationFragment"));

export type SeniorityLevelEnum = (string & ("gmeow:seniorityEntry" | "gmeow:seniorityExecutive" | "gmeow:seniorityLead" | "gmeow:seniorityMid" | "gmeow:senioritySenior"));

export type SensitivityLevelEnum = (string & ("gmeow:sensitivityConfidential" | "gmeow:sensitivityInternal" | "gmeow:sensitivityPublic" | "gmeow:sensitivityRestricted" | "gmeow:sensitivitySensitivePersonal"));

export type SensoryEnvironment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:environmentAtLocation"?: ((Location | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Location | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SensoryModalityEnum = (string & ("gmeow:sensoryModalityAirQuality" | "gmeow:sensoryModalityAuditory" | "gmeow:sensoryModalityGustatory" | "gmeow:sensoryModalityOlfactory" | "gmeow:sensoryModalityTactile" | "gmeow:sensoryModalityThermal" | "gmeow:sensoryModalityVisual"));

export type SensoryObservation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationMethod": ObservationMethodEnum;
  readonly "gmeow:sensoryObservationOf"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:sensoryProperty"?: (ObservablePropertyEnum | readonly (ObservablePropertyEnum)[]);
  readonly "gmeow:sensoryResult"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:vantage"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SensoryPerception = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:perceptionEnvironment"?: ((SensoryEnvironment | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SensoryEnvironment | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SequenceCoordinates = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:inReferenceAssembly": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type SequenceFeatureTypeEnum = (string & ("gmeow:sequenceFeatureTypeCDS" | "gmeow:sequenceFeatureTypeChromosome" | "gmeow:sequenceFeatureTypeExon" | "gmeow:sequenceFeatureTypeGene" | "gmeow:sequenceFeatureTypeIntron" | "gmeow:sequenceFeatureTypeSNP"));

export type ServiceStatusEnum = (string & ("gmeow:serviceStatusLive" | "gmeow:serviceStatusShutDown"));

export type SeverityLevelEnum = (string & ("gmeow:severityCatastrophic" | "gmeow:severityMinor" | "gmeow:severityModerate" | "gmeow:severitySevere"));

export type SexAssignedAtBirth = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type SexAssignedAtBirthEnum = (string & ("gmeow:saabFemale" | "gmeow:saabIntersex" | "gmeow:saabMale" | "gmeow:saabUnknown"));

export type SexualOrientation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:facetSubject": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:facetVantage": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:sexualOrientationValue": ((SexualOrientationValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(SexualOrientationValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(SexualOrientationValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type SexualOrientationValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type SexualOrientationValueEnum = (string & ("gmeow:orientAsexual" | "gmeow:orientBisexual" | "gmeow:orientDemisexual" | "gmeow:orientHeterosexual" | "gmeow:orientHomosexual" | "gmeow:orientPansexual" | "gmeow:orientQueer" | "gmeow:orientQuestioning"));

export type SignatureSchemeEnum = (string & ("gmeow:signatureSchemeBls12381" | "gmeow:signatureSchemeECDSAP256" | "gmeow:signatureSchemeECDSASecp256k1" | "gmeow:signatureSchemeEd25519" | "gmeow:signatureSchemeRSASHA256"));

export type SiteTypeEnum = (string & ("gmeow:siteTypeBranch" | "gmeow:siteTypeHeadquarters" | "gmeow:siteTypeRegistered"));

export type SkillProficiency = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:skillProficiencyAgent"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:skillProficiencyLevel"?: (ProficiencyLevelEnum | readonly (ProficiencyLevelEnum)[]);
  readonly "gmeow:skillProficiencyOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:skillProficiencyScale"?: (ProficiencyScaleEnum | readonly (ProficiencyScaleEnum)[]);
  readonly [key: string]: JsonValue;
};

export type SliceQualityDimensionEnum = (string & ("gmeow:qualityDimensionCounterExampleDepth" | "gmeow:qualityDimensionDocumentation" | "gmeow:qualityDimensionGlyphOptimality" | "gmeow:qualityDimensionGrounding" | "gmeow:qualityDimensionInferentialDensity" | "gmeow:qualityDimensionLinkage" | "gmeow:qualityDimensionProjection" | "gmeow:qualityDimensionProseQuality" | "gmeow:qualityDimensionProvenanceHonesty" | "gmeow:qualityDimensionTesting" | "gmeow:qualityDimensionTranslationCoverage"));

export type SmartContract = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:contractAddress"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type SoftwareAgent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type SoftwareProduct = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type SourceIndependenceEnum = (string & ("gmeow:sourceIndependenceIndependent" | "gmeow:sourceIndependenceSelfOrIssuerOriginated"));

export type SourceTierEnum = (string & ("gmeow:sourceTierPrimary" | "gmeow:sourceTierSecondary" | "gmeow:sourceTierTertiary"));

export type SourceTree = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type SpatialAggregation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:aggregationFunction": AggregationFunctionEnum;
  readonly "gmeow:minimumPopulation"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:observationResult": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:observedFeature": (Place | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:vantage": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type SpatialCoordinates = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:coordinateFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type SpatialMeasurement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observedFeature"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:vantage"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type StandpointClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type StandpointModalityEnum = (string & ("gmeow:bullshit" | "gmeow:conceivable" | "gmeow:probable" | "gmeow:refuted" | "gmeow:unequivocal"));

export type StandpointTenure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:standpointClaim": (StandpointClaim | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:tenurePosition"?: JsonValue;
  readonly "gmeow:tenureStandpoint": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly [key: string]: JsonValue;
};

export type StepTypeEnum = (string & ("gmeow:stepTypeAtomic" | "gmeow:stepTypeBranch" | "gmeow:stepTypeEnd" | "gmeow:stepTypeParallel" | "gmeow:stepTypeStart" | "gmeow:stepTypeSubprocess"));

export type StorageMediumEnum = (string & ("gmeow:storageMediumCloudService" | "gmeow:storageMediumContentAddressed" | "gmeow:storageMediumLocalFilesystem" | "gmeow:storageMediumObjectStore" | "gmeow:storageMediumPhysicalDisk" | "gmeow:storageMediumRemovableMedia"));

export type StrandOrientationEnum = (string & ("gmeow:strandBoth" | "gmeow:strandForward" | "gmeow:strandReverse"));

export type Stream = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:streamOf"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type StyleGuide = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:styleGuideFor": JsonValue;
  readonly [key: string]: JsonValue;
};

export type Support = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:supportSource": JsonValue;
  readonly "gmeow:supportTarget"?: ((Argument | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Argument | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type SupportAssessment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:supportGround"?: JsonValue;
  readonly "gmeow:supportStrength"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:supportUnderStandard"?: (EpistemicStandardEnum | readonly (EpistemicStandardEnum)[]);
  readonly [key: string]: JsonValue;
};

export type SupportPolarityEnum = (string & ("gmeow:polarityNeutral" | "gmeow:polarityRefutes" | "gmeow:polaritySupports"));

export type SupportStatusEnum = (string & ("gmeow:supportBoth" | "gmeow:supportNeither" | "gmeow:supportOpposed" | "gmeow:supportSupported"));

export type SymbolicSystemKindEnum = (string & ("gmeow:symbolicKindCommunicationConvention" | "gmeow:symbolicKindCryptographic" | "gmeow:symbolicKindEmoji" | "gmeow:symbolicKindEncoding" | "gmeow:symbolicKindGesture" | "gmeow:symbolicKindMathematical" | "gmeow:symbolicKindMusical" | "gmeow:symbolicKindPlatformConvention" | "gmeow:symbolicKindStenographic" | "gmeow:symbolicKindTranscription"));

export type Systolic = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/bloodpressure/magnitude"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:openehr/bloodpressure/precision"?: JsonValue;
  readonly "gmeow:openehr/bloodpressure/units": "mm[Hg]";
  readonly [key: string]: JsonValue;
};

export type Tagging = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Task = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:taskStatus": TaskStatusEnum;
  readonly [key: string]: JsonValue;
};

export type TaskStatusEnum = (string & ("gmeow:taskStatusCancelled" | "gmeow:taskStatusCompleted" | "gmeow:taskStatusInProgress" | "gmeow:taskStatusNotStarted"));

export type Teaching = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:learner"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:subjectTaught": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:teacher"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TempoMap = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasTempoMapSegment"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type TempoMapKindEnum = (string & ("gmeow:tempoMapKindConstant" | "gmeow:tempoMapKindCurve" | "gmeow:tempoMapKindLinearRamp" | "gmeow:tempoMapKindMeasured"));

export type TempoMapSegment = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:segmentMapRatioDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:segmentMapRatioNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:segmentSpan": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:segmentTempoMapKind": TempoMapKindEnum;
  readonly "gmeow:tempoMapSegmentOf": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type TemporalFrame = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:frameKind"?: (FrameKindEnum | readonly (FrameKindEnum)[]);
  readonly "gmeow:frameRealm"?: (FrameRealmEnum | readonly (FrameRealmEnum)[]);
  readonly "gmeow:frameTimeScale"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type TemporalMeasurement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationMethod": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly "gmeow:observedFeature": (Entity | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:vantage": ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type TemporalPrecisionEnum = (string & ("gmeow:precisionCirca" | "gmeow:precisionDay" | "gmeow:precisionDecade" | "gmeow:precisionMonth" | "gmeow:precisionYear"));

export type TermStabilityEnum = (string & ("gmeow:stabilityDeprecated" | "gmeow:stabilityExperimental" | "gmeow:stabilityStable"));

export type TimbreDescriptorEnum = (string & ("gmeow:timbreDescriptorBreathy" | "gmeow:timbreDescriptorBright" | "gmeow:timbreDescriptorDark" | "gmeow:timbreDescriptorGritty" | "gmeow:timbreDescriptorHollow"));

export type TimeInterval = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasTemporalFrame": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type TimeMapping = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:mapRatioDenominator"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:mapsFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:mapsToFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:timeMappingKind": TimeMappingKindEnum;
  readonly [key: string]: JsonValue;
};

export type TimeMappingKindEnum = (string & ("gmeow:timeMappingKindSyncUnsynchronized" | "gmeow:timeMappingKindTempoCanon" | "gmeow:timeMappingKindTempoMap" | "gmeow:timeMappingKindTuplet"));

export type TimeZone = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:timeZoneIanaId": (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type ToneEvent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:toneEventArticulation"?: JsonValue;
  readonly "gmeow:toneEventDynamics"?: JsonValue;
  readonly "gmeow:toneEventIsUnpitched"?: (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:toneEventPitchTrajectory"?: JsonValue;
  readonly "gmeow:toneEventPitchValue"?: JsonValue;
  readonly "gmeow:toneEventTimbre"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type ToolCall = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:calledByInvocation"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:toolArguments"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:toolResult"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:usedTool"?: ((SoftwareAgent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((SoftwareAgent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TopologyClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type TopologyComputation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type Trademark = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type TrademarkStatusEnum = (string & ("gmeow:trademarkStatusCancelled" | "gmeow:trademarkStatusExpired" | "gmeow:trademarkStatusPending" | "gmeow:trademarkStatusRegistered" | "gmeow:trademarkStatusUnregistered"));

export type Trajectory = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:trajectoryReferenceFrame": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type TransactionStatusEnum = (string & ("gmeow:transactionStatusCompleted" | "gmeow:transactionStatusFailed" | "gmeow:transactionStatusPending" | "gmeow:transactionStatusReversed"));

export type TransactionTypeEnum = (string & ("gmeow:transactionTypeDeposit" | "gmeow:transactionTypeFee" | "gmeow:transactionTypeInterest" | "gmeow:transactionTypePayment" | "gmeow:transactionTypeRefund" | "gmeow:transactionTypeTransfer" | "gmeow:transactionTypeWithdrawal"));

export type TransformCodecEnum = (string & ("gmeow:codecBase64" | "gmeow:codecBase85" | "gmeow:codecCoseEncrypt0" | "gmeow:codecGzip" | "gmeow:codecIdentity" | "gmeow:codecLzma2" | "gmeow:codecZstd"));

export type TransformationType = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "rdfs:subClassOf"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TransformationTypeEnum = (string & ("gmeow:transformAugmentation" | "gmeow:transformDiminution" | "gmeow:transformInversion" | "gmeow:transformOctaveDisplacement" | "gmeow:transformOrnamentation" | "gmeow:transformPhaseShift" | "gmeow:transformQuotation" | "gmeow:transformReaccentuation" | "gmeow:transformReduction" | "gmeow:transformRetrograde" | "gmeow:transformSpectralCompression" | "gmeow:transformTimbreReorchestration" | "gmeow:transformTransposition"));

export type TransliterationSchemeEnum = (string & ("gmeow:schemeBGNPCGN" | "gmeow:schemeHepburn" | "gmeow:schemeIAST" | "gmeow:schemeIPA" | "gmeow:schemeISO15919" | "gmeow:schemeISO233" | "gmeow:schemeKunreiShiki" | "gmeow:schemeMcCuneReischauer" | "gmeow:schemeNihonShiki" | "gmeow:schemePinyin" | "gmeow:schemeRevisedRomanization" | "gmeow:schemeWadeGiles"));

export type TransparencyLogEntry = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:logEntryIndex"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:logEntryUrl"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly [key: string]: JsonValue;
};

export type TraversalConstraint = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:constraintAppliesTo": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "gmeow:constraintFunction"?: JsonValue;
  readonly "gmeow:constraintText": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TreeEntry = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:treeEntryMode"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:treeEntryName"?: (({
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        } | string) | readonly (({
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            } | string))[]);
  readonly "gmeow:treeEntryObject"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type TrustAssertion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:trustee"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:trustor"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TruthDirectednessEnum = (string & ("gmeow:truthAimed" | "gmeow:truthIndifferent" | "gmeow:truthStrategic"));

export type TuningSystem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:dividedIntervalDenominator"?: (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:frameKind": FrameKindEnum;
  readonly "gmeow:frameRealm": FrameRealmEnum;
  readonly "gmeow:requiresHost": (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "gmeow:tuningKind": TuningSystemKindEnum;
  readonly [key: string]: JsonValue;
};

export type TuningSystemKindEnum = (string & ("gmeow:tuningSystemKindAdaptive" | "gmeow:tuningSystemKindEqualDivision" | "gmeow:tuningSystemKindInstrumentRelative" | "gmeow:tuningSystemKindJustIntonation" | "gmeow:tuningSystemKindSpectralDerived" | "gmeow:tuningSystemKindTablatureRelative" | "gmeow:tuningSystemKindUnpitched" | "gmeow:tuningSystemKindWellTemperament"));

export type UsageAttestation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:attestedForm"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "gmeow:attestedInSource"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type VerdictLatticeRelationEnum = (string & ("gmeow:VerdictEquivalent" | "gmeow:VerdictIncomparable" | "gmeow:VerdictWeaker"));

export type VerificationResult = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasVerificationStatus"?: (VerificationStatusEnum | readonly (VerificationStatusEnum)[]);
  readonly [key: string]: JsonValue;
};

export type VerificationStatusEnum = (string & ("gmeow:verificationStatusExpired" | "gmeow:verificationStatusFailed" | "gmeow:verificationStatusFinalityPending" | "gmeow:verificationStatusPolicyFailed" | "gmeow:verificationStatusRevoked" | "gmeow:verificationStatusUnverified" | "gmeow:verificationStatusVerified"));

export type VersionMembership = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:membershipAuthority"?: ((Agent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Agent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:versionMember"?: ((Entity | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((Entity | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:versionSet"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type VersionRoleEnum = (string & ("gmeow:roleCanonical" | "gmeow:roleCollected" | "gmeow:roleDeprecated" | "gmeow:roleDraft" | "gmeow:roleLTS" | "gmeow:roleLatest" | "gmeow:rolePublished" | "gmeow:roleRevised" | "gmeow:roleStable" | "gmeow:roleVariant" | "gmeow:roleWithdrawn" | "gmeow:roleYanked"));

export type VersionScaleEnum = (string & ("gmeow:scaleMajor" | "gmeow:scaleMinor" | "gmeow:scaleTrivial"));

export type VirtualLocationTypeEnum = (string & ("gmeow:virtualLocationTypeChatSpace" | "gmeow:virtualLocationTypeMetaverseRoom" | "gmeow:virtualLocationTypeOnlineForum" | "gmeow:virtualLocationTypeSocialMediaPage" | "gmeow:virtualLocationTypeStreamingChannel" | "gmeow:virtualLocationTypeVideoConference" | "gmeow:virtualLocationTypeVirtualEventSpace" | "gmeow:virtualLocationTypeWebsite"));

export type Voice = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:voiceMetricStructure"?: JsonValue;
  readonly "gmeow:voiceTimeFrame"?: JsonValue;
  readonly "gmeow:voiceTuningFrame"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type WalletSchemeEnum = (string & ("gmeow:walletSchemeBTC" | "gmeow:walletSchemeETH" | "gmeow:walletSchemeSOL" | "gmeow:walletSchemeXMR"));

export type WeightingPolicyEnum = (string & ("gmeow:weightingEqualCoreAffect" | "gmeow:weightingValenceDominant"));

export type LangComposedForm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangDeclaredTerminologyHomograph = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:homographSource"?: (string | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type LangDenotation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangFeatureValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangForm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:inSignSystem": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LangFormSlot = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangGmnImportedPlane = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:cites": JsonValue;
  readonly "http://purl.org/dc/terms/hasVersion": JsonValue;
  readonly [key: string]: JsonValue;
};

export type LangGrammar = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:grammarFor": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LangGrammarRule = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:grammarRuleOf": ((LangGrammar | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(LangGrammar | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(LangGrammar | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LangGrapheme = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangInterpretationAct = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangMorphFeature = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:featureKey": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "lang:featureValue": ((LangFeatureValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(LangFeatureValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(LangFeatureValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LangParaphrase = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:paraphraseForm"?: ((LangForm | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((LangForm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "lang:paraphraseOf"?: ((LangForm | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((LangForm | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "lang:paraphraseSamenessKind": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LangProjectionEmission = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:projectionTargetName": JsonValue;
  readonly "logic:preservationKind": JsonValue;
  readonly [key: string]: JsonValue;
};

export type LangRendering = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:renderedContent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "lang:renderingConvention": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "lang:renderingKind": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "lang:renderingPreservation": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LangSense = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangSurfaceAnchor = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:anchorEnd": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "lang:anchorSource": JsonValue;
  readonly "lang:anchorStart": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "lang:offsetSpace": ({
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        } | readonly [{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }, ...Array<{
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }>]);
  readonly [key: string]: JsonValue;
};

export type LangSurfaceForm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:analysisLevel": {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      };
  readonly [key: string]: JsonValue;
};

export type LangTranslationUnit = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LangTransliterationMap = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:transliterationScheme": JsonValue;
  readonly "lang:transliterationSourceOrthography": JsonValue;
  readonly "lang:transliterationTargetOrthography": JsonValue;
  readonly [key: string]: JsonValue;
};

export type LangWordForm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "lang:lexemeOf": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicActionSchema = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:capability": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:precondition": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicChaseAcceptanceWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:carriesTerminationCertificate": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicClosureAcceptanceWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:closureEntailedAtom": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicCollection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicConjecture = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:conjectureFormula": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:conjectureLifecycleState": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:conjectureStandpoint": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:verdictProvenance": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicContradictionWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:witnessIndividual": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:witnessPremise": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:witnessWorld": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicCounterfactualAcceptanceWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:selectedByEntrenchment": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicEndurant = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicEvent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicExistentialChaseDemonstrand = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:demonstratesChaseWitness"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type LogicFormalizationCandidate = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:candidateCategory": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:candidateContract": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:candidateExtractionProvenance": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:candidateLifecycle": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:candidateProjectionBehavior": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:candidateScope": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:candidateSemanticRisk": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:candidateSourceField"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "logic:candidateSourceHash": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicFormula = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicFreshnessGuard = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:freshnessHorizon"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "logic:freshnessWindowEnd"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "logic:freshnessWindowStart"?: ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "logic:guardsPrecondition": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicFunctionTerm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicFunctionalComplex = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicGoalEvaluation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:evaluatesGoal": ((Goal | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Goal | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Goal | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:feasibilityStatus": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:goalEvaluationStatus": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:lifecycleStatus": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:satisfactionStatus": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicMcpActionSchema = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:capability": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:compensation": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:precondition": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicNonEntailmentObligation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:obligationDischargeCondition": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:obligationForbiddenPredicate": ((string | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(string | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicNotificationWaitSchema = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:awaitsSignal": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicPlan = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:planGoal": ((Goal | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(Goal | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(Goal | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "logic:planSuccessMode": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicReasoningProgram = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type LogicRecoveryCase = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:recoveryTransform": ((LogicFormula | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(LogicFormula | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(LogicFormula | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicRefutationAcceptanceWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:namesContradictionWitness": ((LogicContradictionWitness | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(LogicContradictionWitness | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(LogicContradictionWitness | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type LogicSectionAcceptanceWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:dischargesSectionObligation": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type LogicTermCarrier = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathAlgebraicStructure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:satisfiesAxiom": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:structureOperation": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:underlyingSet": (MathSet | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathAnalyticProperty = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathApplicationExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:operator": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathApproximateValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:approximates": (MathNumber | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "math:approximationError": JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathArgumentSlot = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:slotExpression": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:slotIndex": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathArithmeticOperation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:operatorCodomain"?: JsonValue;
  readonly "math:operatorDomain"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathAutomorphismGroup = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:automorphismGroupOf": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathBayesianResult = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathBindingExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:argumentSlot": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:boundVariable": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathCalibrationDiagnostic = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathCell = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:cellDimension"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathCellIncidence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:incidenceCoface"?: JsonValue;
  readonly "math:incidenceFace"?: JsonValue;
  readonly "math:incidenceSign": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathCellularSheaf = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasStalk"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:restrictionMap"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:sheafBaseComplex"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathChain = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:chainOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathChart = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:chartDomain": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:coordinateMap": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:targetCoordinateSpace": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathCliffordAlgebra = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:cliffordInvolution"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:hasBasis"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:hasBasisBlade"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:hasGrading"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:metricSignature"?: ((MathMetricSignature | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathMetricSignature | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:pseudoscalarSquare": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:scalarField": JsonValue;
  readonly "math:spaceDimension": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathCliffordAnticommutationWitness = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:anticommutationVerified": ({
        readonly "@type": "xsd:boolean";
        readonly "@value": "true";
      } & (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      }));
  readonly "math:leftGenerator"?: JsonValue;
  readonly "math:rightGenerator"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathCliffordExtension = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:baseAlgebra"?: ((MathCliffordAlgebra | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCliffordAlgebra | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:extendedAlgebra"?: ((MathCliffordAlgebra | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCliffordAlgebra | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:extensionGenerator"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathCliffordModuleDecomposition = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:decomposedObject"?: JsonValue;
  readonly "math:moduleBaseSummand"?: JsonValue;
  readonly "math:moduleExtensionSummand"?: JsonValue;
  readonly "math:splitJoinVerified": ({
        readonly "@type": "xsd:boolean";
        readonly "@value": "true";
      } & (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      }));
  readonly [key: string]: JsonValue;
};

export type MathClosedFormFunction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:definingExpression"?: JsonValue;
  readonly "math:formalArgument"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathCoboundary = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:coboundaryOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathCochainComplex = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathCombinatorialLaplacian = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:combinatorialLaplacianComplex"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:laplacianDegree"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:lowerBoundaryOperator"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:upperBoundaryOperator"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathCompactSpace = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:isCompact": (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathCompactification = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:boundaryAtInfinity"?: JsonValue;
  readonly "math:compactifiedSpace": JsonValue;
  readonly "math:compactifyingMap": JsonValue;
  readonly "math:originalSpace": JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathComplement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:ambientSpace": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:complementSemantics": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathConditionalIndependenceAssertion = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:independentGiven": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathConditionalProbability = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:conditionalOn": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathConfidenceInterval = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:confidenceLevel": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:hasIntervalBound": readonly [{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, ...Array<{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }>];
  readonly [key: string]: JsonValue;
};

export type MathConformalCompactification = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:conformalFactor": JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathConnectedSpace = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:isConnected": (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathConnection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:connectionOn": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathContinuousMap = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:isContinuous": (boolean | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathConvergence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:convergenceMode": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:convergesTo": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathCredibleInterval = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:credibleMass": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:hasPosterior": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathCrossEntropy = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:informationBase"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:referenceDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathCycle = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:cycleOf"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathDerivative = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:derivativeOf": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:derivativeOrder": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:withRespectToVariable": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathDerivedDimension = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:baseDimensionExponent": ((MathDimensionExponent | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathDimensionExponent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathDimensionExponent | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathDimensionExponent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:exponentDenominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:exponentNumerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:exponentOfDimension": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathDimensionalExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:homogeneousOperand": readonly [{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }, ...Array<{
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }>];
  readonly [key: string]: JsonValue;
};

export type MathDimensionalReduction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:analysisInput"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:analysisOutput"?: ((MathEmbedding | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathEmbedding | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:targetDimension": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathDistribution = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:distributionFamily": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathEffectSize = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:hasReferenceFrame": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:effectSizeContrast": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:effectSizeScale": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathEmbedding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:embeddingFunction": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:embeddingModel": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:embeddingSource": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:targetSpace": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathEntropy = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:informationBase"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:informationUnit"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathEstimand = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathEstimate = ({
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:estimator": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
} & ({
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "math:estimatedParameter": JsonValue;
    readonly [key: string]: JsonValue;
  } | {
    readonly "@annotation"?: Annotation;
    readonly "@id"?: string;
    readonly "@type"?: (string | readonly (string)[]);
    readonly "math:estimatesEstimand": JsonValue;
    readonly [key: string]: JsonValue;
  }));

export type MathFiltration = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasFiltrationStage": ((MathFiltrationStage | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathFiltrationStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathFiltrationStage | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathFiltrationStage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:filtrationThreshold"?: {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:stageStructure": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathFisherInformation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:likelihoodModel"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:scoreParameter"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathFittedModel = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:fittedToData": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:modelFormula": ((MathModelFormula | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathModelFormula | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathModelFormula | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathFlatConnection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:transportCochain"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathFormalVerificationResult = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathFrequentistResult = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathFunction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:codomain": (MathSet | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly "math:domain": (MathSet | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathFunctionPiece = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:pieceDomain"?: JsonValue;
  readonly "math:pieceExpression"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathGlobalSection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:overSheaf"?: ((MathCellularSheaf | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCellularSheaf | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:sectionRegion"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathGluingObstruction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:obstructionOf"?: ((MathCellularSheaf | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCellularSheaf | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathGramMatrix = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:inBasis": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:representsForm": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathHamiltonianSystem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:generatesFlow"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:hamiltonianFunction"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:stateSpace"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:symplecticForm"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathHodgeDecomposition = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:coexactComponent"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:coexactHarmonicInnerProduct"?: ((MathRationalValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathRationalValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:decomposesFlow"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:exactCoexactInnerProduct"?: ((MathRationalValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathRationalValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:exactComponent"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:exactHarmonicInnerProduct"?: ((MathRationalValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathRationalValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:harmonicComponent"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:hodgeBoundaryOperator"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overSheaf"?: ((MathCellularSheaf | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCellularSheaf | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:reconstructionResidual"?: ((MathRationalValue | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathRationalValue | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathHolonomy = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:holonomyLoop": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:holonomyOf"?: ((MathConnection | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathConnection | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathHomomorphicEncryptionScheme = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:homomorphicOver": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:noiseModel": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:securityAssumption": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathHomomorphism = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:preservationLaw": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:preservedOperation": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathInferenceRun = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathIngestRun = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "logic:instantiatesPlan": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "logic:instantiatesSchema": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:ingestCorrespondence": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:parseSource": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathIntegral = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:integrand": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:integrationDomain": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:withRespectTo": (MathMeasure | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathInterval = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:lowerEndpoint": JsonValue;
  readonly "math:lowerInclusion"?: JsonValue;
  readonly "math:upperEndpoint": JsonValue;
  readonly "math:upperInclusion"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathKullbackLeiblerDivergence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:informationBase"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:referenceDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathLieGroup = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasRootSystem": ((MathRootSystem | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathRootSystem | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathRootSystem | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathLimit = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasLimitResult"?: JsonValue;
  readonly "math:limitOf": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:limitPoint": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathLimitResult = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:limitOutcome"?: JsonValue;
  readonly "math:limitResultValue"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathLocalSection = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:overSheaf"?: ((MathCellularSheaf | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathCellularSheaf | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:sectionRegion"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathLogOddsValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasDimension": ({
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    } & {
        readonly "@id": "math:dimensionless";
      });
  readonly [key: string]: JsonValue;
};

export type MathManifold = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:manifoldDimension": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:manifoldStructureKind": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathMapperConstruction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:clusteringRule"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:coverScheme"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:filterFunction"?: ((MathFunction | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathFunction | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:outputNerve"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:sourceMetricSpace"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathMathematicalConstant = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:isExact": (({
          readonly "@type": "xsd:boolean";
          readonly "@value": "true";
        } & (boolean | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        })) | readonly [({
              readonly "@type": "xsd:boolean";
              readonly "@value": "true";
            } & (boolean | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })), ...Array<({
              readonly "@type": "xsd:boolean";
              readonly "@value": "true";
            } & (boolean | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))>]);
  readonly "math:quantityValue"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathMathematicalExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathMeasure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:measureOn": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:totalMass": JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathMeasureEvaluation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:evaluatedMeasure"?: JsonValue;
  readonly "math:measureResult": JsonValue;
  readonly "math:measuredSubset"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathMetricSignature = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathModelDiagnostic = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:diagnosticFor": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:diagnosticMethod": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathModelFormula = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:argumentSlot": ((MathArgumentSlot | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathArgumentSlot | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathArgumentSlot | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathMultiparameterFiltration = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathMutualInformation = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:informationBase"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:informationUnit"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overDistribution"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathNorm = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:inducedByForm": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathNumber = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:inNumberSystem": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathOddsValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasDimension": ({
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    } & {
        readonly "@id": "math:dimensionless";
      });
  readonly [key: string]: JsonValue;
};

export type MathOrthogonalComplement = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:definedByInnerProduct": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathPCAAnalysis = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:analysisInput": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:centeringPolicy": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:covarianceOperator": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:eigensolver": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:explainedVarianceRatio": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:loadingVector": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:principalComponent": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:residualSubspace": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:scalingPolicy": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:scoreVector": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathPValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:alternativeSidedness"?: (({
          readonly "@id": "math:exactTail";
        } | {
          readonly "@id": "math:greaterAlternative";
        } | {
          readonly "@id": "math:lessAlternative";
        } | {
          readonly "@id": "math:midPTail";
        } | {
          readonly "@id": "math:oneSidedAlternative";
        } | {
          readonly "@id": "math:twoSidedAlternative";
        }) | readonly (({
              readonly "@id": "math:exactTail";
            } | {
              readonly "@id": "math:greaterAlternative";
            } | {
              readonly "@id": "math:lessAlternative";
            } | {
              readonly "@id": "math:midPTail";
            } | {
              readonly "@id": "math:oneSidedAlternative";
            } | {
              readonly "@id": "math:twoSidedAlternative";
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathParallelTransport = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:transportAlong": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:transportConnection"?: ((MathConnection | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathConnection | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathPersistenceLifetime = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathPersistenceModule = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:moduleIndex"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:structureMap"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathPersistenceMorphism = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:morphismSource"?: ((MathPersistenceModule | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathPersistenceModule | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "math:morphismTarget"?: ((MathPersistenceModule | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathPersistenceModule | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathPersistentHomology = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:analysisInput"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:analysisOutput"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overFiltration"?: ((MathFiltration | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathFiltration | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathPiecewiseFunction = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasPiece"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type MathProbabilityEvent = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathProbabilityMeasure = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:totalMass"?: {
        readonly "@type": "xsd:integer";
        readonly "@value": "1";
      };
  readonly [key: string]: JsonValue;
};

export type MathProbabilitySpace = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:eventSigmaAlgebra": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:probabilityMeasure": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:sampleSpace": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathProbabilityValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathProofCheckActivity = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathQuantity = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathRandomVariable = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:randomVariableCodomain": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "math:randomVariableDomain": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathRationalValue = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:denominator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly "math:numerator": (number | {
        readonly "@type"?: string;
        readonly "@value": JsonValue;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathResidualInterpretationClaim = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:observationResult": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:observedFeature": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly "gmeow:vantage": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathRestrictionImage = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:imageSourceValue": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "math:imageTargetValue": ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly [(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }), ...Array<(number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathRing = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:satisfiesDistributivity": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathRootSystem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathSequence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasConvergence": ((MathConvergence | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathConvergence | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathConvergence | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly [key: string]: JsonValue;
};

export type MathSeries = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasConvergence": ((MathConvergence | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathConvergence | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathConvergence | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "math:seriesTerm": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathSet = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathSetBuilderExpression = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:memberCondition": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathSimplicialComplex = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathStabilityCalibrationRecord = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:calibrationEvidence": ((MathPersistenceLifetime | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly [(MathPersistenceLifetime | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }), ...Array<(MathPersistenceLifetime | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            })>]);
  readonly "math:credenceDerivationKind": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly "math:stabilityGuarantee": (MathTheorem | {
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      });
  readonly [key: string]: JsonValue;
};

export type MathStatisticalVariable = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathSurprisal = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:informationBase"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:ofOutcome"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overDistribution"?: ((MathDistribution | {
          readonly "@id": string;
          readonly [key: string]: JsonValue;
        }) | readonly ((MathDistribution | {
              readonly "@id": string;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly [key: string]: JsonValue;
};

export type MathSymbolReference = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:hasMathematicalSymbol": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathTangentSpace = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type MathTensorComputationGraph = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:computationNode": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathTheorem = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:roleInTheory": ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly [{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }, ...Array<{
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          }>]);
  readonly [key: string]: JsonValue;
};

export type MathVariableOccurrence = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:declaredVariable": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathVectorBinding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:operationCapacity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overBasis"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overVectorSpace"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:recoveryLossContract"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:vsaOperand"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathVectorBundling = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:operationCapacity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overBasis"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overVectorSpace"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:recoveryLossContract"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:vsaOperand"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathVectorUnbinding = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:boundVector"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:operationCapacity"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overBasis"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:overVectorSpace"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:recoveredOperand"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:recoveryLossContract"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly "math:unbindingKey"?: ({
        readonly "@id": string;
        readonly [key: string]: JsonValue;
      } | readonly ({
            readonly "@id": string;
            readonly [key: string]: JsonValue;
          })[]);
  readonly [key: string]: JsonValue;
};

export type MathWeightTensor = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "math:inParameterSpace": {
      readonly "@id": string;
      readonly [key: string]: JsonValue;
    };
  readonly [key: string]: JsonValue;
};

export type MathZigzagDiagram = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0000 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00002 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00003 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00004 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/definingCode"?: ({
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/openehr/433";
        } | readonly ({
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/openehr/433";
            })[]);
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00005 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0000"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00006 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0000": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0001 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0001": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00012 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0001"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0002 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0002"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00022 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0002": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0003 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0003": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00032 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0003"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0004 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0004"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00042 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0004"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00043 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0004": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0005 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0005"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00052 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0005"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00053 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0005": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00054 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0005": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00055 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/definingCode"?: (({
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0007";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0008";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0009";
        }) | readonly (({
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0007";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0008";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0009";
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0006 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0006"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00062 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0006"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00063 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0006": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00064 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0006": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0010 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0010"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00102 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0010"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00103 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/magnitude"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:openehr/testdatatypes/units": "°C";
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0011 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0011"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00112 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0011"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00113 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0011": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00114 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0011": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0012 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0012"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00122 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0012"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00123 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0012": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00124 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0012": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00125 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/value"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0013 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0013"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00132 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0013"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00133 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/value"?: (({
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0014";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0015";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0016";
        }) | readonly (({
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0014";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0015";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/local/at0016";
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0017 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0017"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00172 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0017"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00173 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0017": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00174 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0017": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0018 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0018"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00182 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0018"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00183 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0018": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00184 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0018": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0019 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0019"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00192 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0019"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00193 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00194 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00195 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00196 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00197 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00198 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00199 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0019": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0020 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0020"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00202 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0020"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00203 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0020": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00204 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0020": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00205 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/magnitude"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:openehr/testdatatypes/precision"?: JsonValue;
  readonly "gmeow:openehr/testdatatypes/units": "mg";
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00206 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0020": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00207 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/magnitude"?: ((number | {
          readonly "@type"?: string;
          readonly "@value": JsonValue;
          readonly [key: string]: JsonValue;
        }) | readonly ((number | {
              readonly "@type"?: string;
              readonly "@value": JsonValue;
              readonly [key: string]: JsonValue;
            }))[]);
  readonly "gmeow:openehr/testdatatypes/precision"?: JsonValue;
  readonly "gmeow:openehr/testdatatypes/units": "mg";
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0021 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0021"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At002110 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At002111 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/value"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00212 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0021"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00213 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00214 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00215 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00216 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00217 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/value"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00218 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00219 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0021": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0022 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0022"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00222 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0022"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00223 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0022": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00224 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0022": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00225 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/definingCode"?: (({
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/application/dicom";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/image/jpeg";
        } | {
          readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/image/png";
        }) | readonly (({
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/application/dicom";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/image/jpeg";
            } | {
              readonly "@id": "gmeow:openehr/testdatatypes/terminology/IANA_media-types/image/png";
            }))[]);
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0023 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0023"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00232 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0023"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00233 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0023": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0024 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0024"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00242 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0024"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00243 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0024": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00244 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0024": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00245 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0024": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0025 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0025"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00252 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0025"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00253 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0025": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00254 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0025": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00255 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00256 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0025": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00257 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00258 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0025": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00259 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/text"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0026 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0026"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00262 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0026"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00263 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0026": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00264 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0026": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0027 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0027"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00272 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0027"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00273 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0027": JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At0028 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0028"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00282 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/existence/at0028"?: JsonValue;
  readonly [key: string]: JsonValue;
};

export type TestAllDatatypesEnV1At00283 = {
  readonly "@annotation"?: Annotation;
  readonly "@id"?: string;
  readonly "@type"?: (string | readonly (string)[]);
  readonly "gmeow:openehr/testdatatypes/occurrences/at0028": JsonValue;
  readonly [key: string]: JsonValue;
};
