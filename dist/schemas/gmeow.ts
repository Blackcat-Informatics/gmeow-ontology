
export enum AccessibilityFacetEnum {

    auditory_access = "facetAuditory",
    physical_clearance = "facetClearance",
    cognitive_access = "facetCognitive",
    life_support_access = "facetLifeSupport",
    step_free_access = "facetStepFree",
    visual_access = "facetVisual",
    wheelchair_access = "facetWheelchair",
};

export enum AccessibilityPolarityEnum {

    barrier = "polarityBarrier",
    feature = "polarityFeature",
    limited = "polarityLimited",
};

export enum AggregationFunctionEnum {

    average = "aggAverage",
    centroid = "aggCentroid",
    count = "aggCount",
    density = "aggDensity",
    maximum = "aggMaximum",
    minimum = "aggMinimum",
    sum = "aggSum",
};

export enum AnnotationMotivationEnum {

    assessing = "motivationAssessing",
    bookmarking = "motivationBookmarking",
    commenting = "motivationCommenting",
    describing = "motivationDescribing",
    highlighting = "motivationHighlighting",
    linking = "motivationLinking",
    moderating = "motivationModerating",
    questioning = "motivationQuestioning",
    replying = "motivationReplying",
    tagging = "motivationTagging",
};

export enum ArcTypeEnum {

    coming_of_age = "arcTypeComingOfAge",
    corruption = "arcTypeCorruption",
    fall = "arcTypeFall",
    quest = "arcTypeQuest",
    recovery = "arcTypeRecovery",
    redemption = "arcTypeRedemption",
};

export enum AssetTypeEnum {

    bond = "assetTypeBond",
    commodity = "assetTypeCommodity",
    cryptocurrency = "assetTypeCryptocurrency",
    real_estate = "assetTypeRealEstate",
    stock = "assetTypeStock",
};

export enum AttestationTypeEnum {

    AI_output_attestation = "attestationTypeAIOutput",
    blockchain_claim = "attestationTypeBlockchainClaim",
    C2PA_manifest = "attestationTypeC2PA",
    DSSE_envelope = "attestationTypeDSSE",
    EAT_token = "attestationTypeEAT",
    fact_check = "attestationTypeFactCheck",
    git_signed_tag = "attestationTypeGitSignedTag",
    in_toto_attestation = "attestationTypeInToto",
    nanopublication = "attestationTypeNanopublication",
    quality_report_attestation = "attestationTypeQualityReport",
    release_manifest = "attestationTypeReleaseManifest",
    SCITT_signed_statement = "attestationTypeSCITT",
    SLSA_provenance = "attestationTypeSLSAProvenance",
    signed_RDF_graph = "attestationTypeSignedRDF",
    verifiable_credential = "attestationTypeVerifiableCredential",
};

export enum AvailabilityStatusEnum {

    busy = "availabilityStatusBusy",
    free = "availabilityStatusFree",
    out_of_office = "availabilityStatusOutOfOffice",
    tentative = "availabilityStatusTentative",
};

export enum AxisEnum {

    address_locality_axis = "axisAddressLocality",
    address_region_axis = "axisAddressRegion",
    allocentric_X = "axisAllocentricX",
    allocentric_Y = "axisAllocentricY",
    altitude = "axisAltitude",
    angular_velocity_X_axis = "axisAngularVelocityX",
    angular_velocity_Y_axis = "axisAngularVelocityY",
    angular_velocity_Z_axis = "axisAngularVelocityZ",
    arousal = "axisArousal",
    CIE_aASTERISK_red_green = "axisAstar",
    BGP_autonomous_system_axis = "axisBGPAS",
    bearing_axis = "axisBearing",
    blue_channel = "axisBlue",
    CIE_bASTERISK_blue_yellow = "axisBstar",
    conceptual_similarity = "axisConceptualSimilarity",
    configuration_vector_axis = "axisConfigurationVector",
    country_code_axis = "axisCountryCode",
    cyan_channel = "axisCyan",
    DNS_name_axis = "axisDNSName",
    day = "axisDay",
    declination = "axisDeclination",
    depth = "axisDepth",
    egocentric_forward = "axisEgocentricForward",
    egocentric_lateral = "axisEgocentricLateral",
    elevation = "axisElevation",
    extended_address_axis = "axisExtendedAddress",
    flight_level = "axisFlightLevel",
    frequency = "axisFrequency",
    galactic_latitude = "axisGalacticLatitude",
    galactic_longitude = "axisGalacticLongitude",
    generalized_coordinate = "axisGeneralizedCoordinate",
    generalized_momentum = "axisGeneralizedMomentum",
    geohash_string = "axisGeohash",
    green_channel = "axisGreen",
    heading_axis = "axisHeading",
    Hilbert_state_vector = "axisHilbertState",
    hour = "axisHour",
    IPv4_address_axis = "axisIPv4Address",
    IPv6_address_axis = "axisIPv6Address",
    imagined_space_X = "axisImaginedSpaceX",
    imagined_space_Y = "axisImaginedSpaceY",
    imagined_space_Z = "axisImaginedSpaceZ",
    joint_angle_1 = "axisJointAngle1",
    joint_angle_2 = "axisJointAngle2",
    joint_angle_3 = "axisJointAngle3",
    joint_angle_4 = "axisJointAngle4",
    joint_angle_5 = "axisJointAngle5",
    joint_angle_6 = "axisJointAngle6",
    key_LEFT_PARENTHESISblackRIGHT_PARENTHESIS_channel = "axisKey",
    latent_vector = "axisLatentVector",
    latitude = "axisLatitude",
    CIE_LASTERISK_lightness = "axisLightness",
    linear_velocity_X_axis = "axisLinearVelocityX",
    linear_velocity_Y_axis = "axisLinearVelocityY",
    linear_velocity_Z_axis = "axisLinearVelocityZ",
    longitude = "axisLongitude",
    MAC_address_axis = "axisMACAddress",
    MGRS_grid_reference = "axisMGRS",
    magenta_channel = "axisMagenta",
    magnitude = "axisMagnitude",
    mile_marker_SOLIDUS_chainage = "axisMileMarker",
    minute = "axisMinute",
    momentum_X = "axisMomentumX",
    momentum_Y = "axisMomentumY",
    momentum_Z = "axisMomentumZ",
    month = "axisMonth",
    pitch_axis = "axisPitch",
    Plus_Code_cell = "axisPlusCode",
    port_number_axis = "axisPortNumber",
    post_office_box_axis = "axisPostOfficeBox",
    postal_code_axis = "axisPostalCode",
    predicted_mean_vote_LEFT_PARENTHESISPMVRIGHT_PARENTHESIS = "axisPredictedMeanVote",
    predicted_percentage_dissatisfied_LEFT_PARENTHESISPPDRIGHT_PARENTHESIS = "axisPredictedPercentageDissatisfied",
    quaternion_W_axis = "axisQuaternionW",
    quaternion_X_axis = "axisQuaternionX",
    quaternion_Y_axis = "axisQuaternionY",
    quaternion_Z_axis = "axisQuaternionZ",
    red_channel = "axisRed",
    right_ascension = "axisRightAscension",
    roll_axis = "axisRoll",
    scalar_axis = "axisScalar",
    second = "axisSecond",
    sequence_position = "axisSequencePosition",
    street_address_axis = "axisStreetAddress",
    CIE_X_tristimulus = "axisTristimulusX",
    CIE_Y_tristimulus = "axisTristimulusY",
    CIE_Z_tristimulus = "axisTristimulusZ",
    UNSOLIDUSLOCODE_code = "axisUNLocode",
    URL_axis = "axisURL",
    valence = "axisValence",
    virtual_address_axis = "axisVirtualAddress",
    what3words_word_triple = "axisWhat3Words",
    X_axis = "axisX",
    Y_axis = "axisY",
    yaw_axis = "axisYaw",
    year = "axisYear",
    yellow_channel = "axisYellow",
    Z_axis = "axisZ",
};

export enum BranchConditionTypeEnum {

    if = "branchConditionIf",
    loop = "branchConditionLoop",
    parallel = "branchConditionParallel",
    switch = "branchConditionSwitch",
};

export enum CadastralReferenceTypeEnum {

    folio_number = "referenceTypeFolio",
    lot_number = "referenceTypeLot",
    parcel_identifier = "referenceTypeParcelId",
    survey_plan_reference = "referenceTypeSurveyPlan",
    title_number = "referenceTypeTitle",
};

export enum CalendarMethodEnum {

    add = "calendarMethodAdd",
    cancel = "calendarMethodCancel",
    counter = "calendarMethodCounter",
    decline_counter = "calendarMethodDeclineCounter",
    publish = "calendarMethodPublish",
    refresh = "calendarMethodRefresh",
    reply = "calendarMethodReply",
    request = "calendarMethodRequest",
};

export enum CalendarSystemEnum {

    Chinese_calendar = "calendarChinese",
    Coptic_calendar = "calendarCoptic",
    Ethiopian_calendar = "calendarEthiopian",
    Gregorian_calendar = "calendarGregorian",
    Hebrew_calendar = "calendarHebrew",
    ISO_week_date = "calendarISOWeek",
    Islamic_LEFT_PARENTHESISHijriRIGHT_PARENTHESIS_calendar = "calendarIslamic",
    Julian_calendar = "calendarJulian",
    Persian_LEFT_PARENTHESISSolar_HijriRIGHT_PARENTHESIS_calendar = "calendarPersian",
};

export enum CarrierMediumEnum {

    e_ink_file = "mediumEInkFile",
    optical_disc = "mediumOpticalDisc",
    print = "mediumPrint",
    server_object = "mediumServerObject",
};

export enum CelestialObjectTypeEnum {

    asteroid = "celestialObjectTypeAsteroid",
    star_cluster = "celestialObjectTypeCluster",
    comet = "celestialObjectTypeComet",
    galaxy = "celestialObjectTypeGalaxy",
    nebula = "celestialObjectTypeNebula",
    planet = "celestialObjectTypePlanet",
    spacecraft_SOLIDUS_artificial_satellite = "celestialObjectTypeSpacecraft",
    star = "celestialObjectTypeStar",
};

export enum CelestialReferenceOriginEnum {

    barycentric_LEFT_PARENTHESISsolar_systemRIGHT_PARENTHESIS = "refOriginBarycentric",
    geocentric = "refOriginGeocentric",
    heliocentric = "refOriginHeliocentric",
    topocentric_LEFT_PARENTHESISobservatory_siteRIGHT_PARENTHESIS = "refOriginTopocentric",
};

export enum CitationIntentEnum {

    bridged_by_reference = "intentBridgedByReference",
    cites_as_data_source = "intentCitesAsDataSource",
    conforms_to = "intentConformsTo",
    derived_from = "intentDerivedFrom",
    disagrees_with = "intentDisagreesWith",
    documents = "intentDocuments",
    extends = "intentExtends",
    is_inspired_by = "intentIsInspiredBy",
    supports = "intentSupports",
    uses_method_in = "intentUsesMethodIn",
};

export enum ClaimVeridicalityEnum {

    licensed_falsehood = "veridicalityLicensedFalsehood",
    untrue = "veridicalityUntrue",
};

export enum ConflictStrategyEnum {

    policy_void_on_conflict = "conflictInvalid",
    permission_wins = "conflictPerm",
    prohibition_wins = "conflictProhibit",
};

export enum ConstraintLogicEnum {

    and_LEFT_PARENTHESISallRIGHT_PARENTHESIS = "logicAnd",
    and_LEFT_PARENTHESISorderedRIGHT_PARENTHESIS = "logicAndSequence",
    or_LEFT_PARENTHESISanyRIGHT_PARENTHESIS = "logicOr",
    exactly_one = "logicXone",
};

export enum ConstraintOperatorEnum {

    equal_to = "operatorEq",
    greater_than = "operatorGt",
    greater_than_or_equal_to = "operatorGteq",
    has_part = "operatorHasPart",
    is_a = "operatorIsA",
    is_all_of = "operatorIsAllOf",
    is_any_of = "operatorIsAnyOf",
    is_none_of = "operatorIsNoneOf",
    is_part_of = "operatorIsPartOf",
    less_than = "operatorLt",
    less_than_or_equal_to = "operatorLteq",
    not_equal_to = "operatorNeq",
};

export enum ContentDispositionEnum {

    attachment = "contentDispositionAttachment",
    inline = "contentDispositionInline",
};

export enum ContentSegmentTypeEnum {

    back_matter = "segmentTypeBackMatter",
    chapter = "segmentTypeChapter",
    front_matter = "segmentTypeFrontMatter",
    paragraph = "segmentTypeParagraph",
    scene = "segmentTypeScene",
    section = "segmentTypeSection",
};

export enum ContentTransferEncodingEnum {

    number_7bit = "transferEncoding7bit",
    number_8bit = "transferEncoding8bit",
    base64 = "transferEncodingBase64",
    binary = "transferEncodingBinary",
    quoted_printable = "transferEncodingQuotedPrintable",
};

export enum ContributionDegreeEnum {

    equal = "degreeEqual",
    lead = "degreeLead",
    supporting = "degreeSupporting",
};

export enum ContributionRoleEnum {

    AI_assistant = "roleAIAssistant",
    author = "roleAuthor",
    bot_contributor = "roleBotContributor",
    code_reviewer = "roleCodeReviewer",
    composer = "roleComposer",
    conceptualization = "roleConceptualization",
    cover_artist = "roleCoverArtist",
    data_curation = "roleDataCuration",
    director = "roleDirector",
    editor = "roleEditor",
    formal_analysis = "roleFormalAnalysis",
    funding_acquisition = "roleFundingAcquisition",
    illustrator = "roleIllustrator",
    investigation = "roleInvestigation",
    LLM_assisted_editor = "roleLLMAssistedEditor",
    letterer = "roleLetterer",
    methodology = "roleMethodology",
    narrator = "roleNarrator",
    photographer = "rolePhotographer",
    project_administration = "roleProjectAdministration",
    releaser = "roleReleaser",
    resources = "roleResources",
    security_contact = "roleSecurityContact",
    software = "roleSoftware",
    software_developer = "roleSoftwareDeveloper",
    software_maintainer = "roleSoftwareMaintainer",
    supervision = "roleSupervision",
    translator = "roleTranslator",
    validation = "roleValidation",
    visualization = "roleVisualization",
    writing_EN_DASH_original_draft = "roleWritingOriginalDraft",
    writing_EN_DASH_review_AMPERSAND_editing = "roleWritingReviewEditing",
};

export enum ControlFlowEnum {

    ingestion_1 = "flowIngestion1",
    ingestion_2 = "flowIngestion2",
    ingestion_3 = "flowIngestion3",
    ingestion_4 = "flowIngestion4",
    ingestion_5 = "flowIngestion5",
};

export enum CopyrightStatusEnum {

    in_copyright = "copyrightStatusInCopyright",
    in_copyright_EM_DASH_educational_use_permitted = "copyrightStatusInCopyrightEducationalUse",
    in_copyright_EM_DASH_EU_orphan_work = "copyrightStatusInCopyrightEuOrphanWork",
    in_copyright_EM_DASH_non_commercial_use_permitted = "copyrightStatusInCopyrightNonCommercialUse",
    in_copyright_EM_DASH_rights_holder_unlocatable = "copyrightStatusInCopyrightRightsholderUnlocatable",
    no_copyright_EM_DASH_contractual_restrictions = "copyrightStatusNoCopyrightContractualRestrictions",
    no_copyright_EM_DASH_non_commercial_use_only = "copyrightStatusNoCopyrightNonCommercialOnly",
    no_copyright_EM_DASH_other_known_legal_restrictions = "copyrightStatusNoCopyrightOtherLegalRestrictions",
    no_copyright_EM_DASH_United_States = "copyrightStatusNoCopyrightUnitedStates",
    no_known_copyright = "copyrightStatusNoKnownCopyright",
    copyright_not_evaluated = "copyrightStatusNotEvaluated",
    public_domain = "copyrightStatusPublicDomain",
    copyright_undetermined = "copyrightStatusUndetermined",
};

export enum CoverageDepthEnum {

    passing_mention = "coverageDepthPassingMention",
    routine_filing = "coverageDepthRoutineFiling",
    significant_coverage = "coverageDepthSignificantCoverage",
};

export enum CreativeWorkTypeEnum {

    audiovisual = "workTypeAudiovisual",
    cartographic = "workTypeCartographic",
    choreographic = "workTypeChoreographic",
    composed_musical = "workTypeComposedMusical",
    dataset = "workTypeDataset",
    film = "workTypeFilm",
    literary = "workTypeLiterary",
    musical = "workTypeMusical",
    narrative = "workTypeNarrative",
    photographic = "workTypePhotographic",
    software = "workTypeSoftware",
    visual = "workTypeVisual",
    written = "workTypeWritten",
};

export enum DatingMethodEnum {

    amino_acid_racemization = "datingMethodAminoAcidRacemization",
    dendrochronology = "datingMethodDendrochronology",
    electron_spin_resonance_LEFT_PARENTHESISESRRIGHT_PARENTHESIS = "datingMethodElectronSpinResonance",
    optically_stimulated_luminescence_LEFT_PARENTHESISOSLRIGHT_PARENTHESIS = "datingMethodOpticallyStimulatedLuminescence",
    paleomagnetism = "datingMethodPaleomagnetism",
    potassium_argon_LEFT_PARENTHESISK_ArRIGHT_PARENTHESIS = "datingMethodPotassiumArgon",
    radiocarbon_dating_LEFT_PARENTHESIS14CRIGHT_PARENTHESIS = "datingMethodRadiocarbon",
    stratigraphic_correlation = "datingMethodStratigraphicCorrelation",
    thermoluminescence = "datingMethodThermoluminescence",
    uranium_lead_LEFT_PARENTHESISU_PbRIGHT_PARENTHESIS = "datingMethodUraniumLead",
};

export enum DepictionContextEnum {

    action_shot = "depictionContextActionShot",
    candid = "depictionContextCandid",
    childhood = "depictionContextChildhood",
    event = "depictionContextEvent",
    family = "depictionContextFamily",
    formal = "depictionContextFormal",
    now_SOLIDUS_current = "depictionContextNow",
    portrait = "depictionContextPortrait",
    professional = "depictionContextProfessional",
    self_portrait = "depictionContextSelfPortrait",
    social = "depictionContextSocial",
    work = "depictionContextWork",
};

export enum DerivationKindEnum {

    affixation = "derivationAffixation",
    back_formation = "derivationBackFormation",
    borrowing = "derivationBorrowing",
    calque = "derivationCalque",
    clipping = "derivationClipping",
    compounding = "derivationCompounding",
    folk_etymology = "derivationFolkEtymology",
    inheritance = "derivationInheritance",
    reanalysis = "derivationReanalysis",
    reconstruction = "derivationReconstruction",
    semantic_shift = "derivationSemanticShift",
    sound_change = "derivationSoundChange",
    spelling_change = "derivationSpellingChange",
    unknown_origin = "derivationUnknownOrigin",
};

export enum DeterminacyEnum {

    crisp = "determinacyCrisp",
    disputed = "determinacyDisputed",
    fuzzy = "determinacyFuzzy",
    probabilistic = "determinacyProbabilistic",
    vague = "determinacyVague",
};

export enum DisclosurePolicyEnum {

    internal_only = "policyInternalOnly",
    never_public = "policyNeverPublic",
    public_careful = "policyPublicCareful",
    public_only_with_independent_source = "policyPublicOnlyWithIndependentSource",
    public_safe = "policyPublicSafe",
    sensitive = "policySensitive",
};

export enum EmploymentTypeEnum {

    apprentice = "employmentTypeApprentice",
    contract = "employmentTypeContract",
    freelance = "employmentTypeFreelance",
    full_time = "employmentTypeFullTime",
    intern = "employmentTypeIntern",
    part_time = "employmentTypePartTime",
    volunteer = "employmentTypeVolunteer",
};

export enum EntityEnum {

    raw_root_data_source = "procedureIngestionRawRoot",
};

export enum EventTypeEnum {

    acquisition = "eventTypeAcquisition",
    adoption = "eventTypeAdoption",
    annulment = "eventTypeAnnulment",
    audit = "eventTypeAudit",
    baptism = "eventTypeBaptism",
    bar_mitzvah = "eventTypeBarMitzvah",
    bat_mitzvah = "eventTypeBatMitzvah",
    birth = "eventTypeBirth",
    build = "eventTypeBuild",
    bullshit = "eventTypeBullshit",
    burial = "eventTypeBurial",
    census = "eventTypeCensus",
    census_activity = "eventTypeCensusActivity",
    christening = "eventTypeChristening",
    clinical_trial = "eventTypeClinicalTrial",
    code_review = "eventTypeCodeReview",
    commit = "eventTypeCommit",
    confirmation = "eventTypeConfirmation",
    creation = "eventTypeCreation",
    cremation = "eventTypeCremation",
    death = "eventTypeDeath",
    deception = "eventTypeDeception",
    destruction = "eventTypeDestruction",
    disinformation_campaign = "eventTypeDisinformation",
    dissolution = "eventTypeDissolution",
    distortion_SOLIDUS_spin = "eventTypeDistortion",
    divorce = "eventTypeDivorce",
    emigration = "eventTypeEmigration",
    engagement = "eventTypeEngagement",
    excavation = "eventTypeExcavation",
    expression_creation = "eventTypeExpressionCreation",
    fabrication = "eventTypeFabrication",
    first_communion = "eventTypeFirstCommunion",
    forgery = "eventTypeForgery",
    funeral = "eventTypeFuneral",
    graduation = "eventTypeGraduation",
    hiring = "eventTypeHiring",
    image_annotation = "eventTypeImageAnnotation",
    image_capture = "eventTypeImageCapture",
    image_processing = "eventTypeImageProcessing",
    image_scanning = "eventTypeImageScanning",
    immigration = "eventTypeImmigration",
    impersonation = "eventTypeImpersonation",
    lie_SOLIDUS_falsification = "eventTypeLie",
    manifestation_production = "eventTypeManifestationProduction",
    marriage = "eventTypeMarriage",
    merge = "eventTypeMerge",
    merger = "eventTypeMerger",
    military_service = "eventTypeMilitaryService",
    name_change = "eventTypeNameChange",
    naturalization = "eventTypeNaturalization",
    omission_SOLIDUS_concealment = "eventTypeOmission",
    ordination = "eventTypeOrdination",
    paltering = "eventTypePaltering",
    probate = "eventTypeProbate",
    promotion = "eventTypePromotion",
    push = "eventTypePush",
    release = "eventTypeRelease",
    rename = "eventTypeRename",
    residence = "eventTypeResidence",
    resignation = "eventTypeResignation",
    retirement = "eventTypeRetirement",
    self_deception = "eventTypeSelfDeception",
    separation = "eventTypeSeparation",
    spin_off = "eventTypeSpinOff",
    split = "eventTypeSplit",
    supersession = "eventTypeSupersession",
    survey = "eventTypeSurvey",
    termination = "eventTypeTermination",
    transfer = "eventTypeTransfer",
    will = "eventTypeWill",
    work_conception = "eventTypeWorkConception",
};

export enum EvidenceClassEnum {

    anecdotal_evidence = "evidenceANECDOTAL",
    family_narrative_evidence = "evidenceFamilyNarrative",
    generated_report_evidence = "evidenceGeneratedReport",
    independent_trade_press_evidence = "evidenceIndependentTradePress",
    legal_filing_evidence = "evidenceLegalFiling",
    newspaper_lead_evidence = "evidenceNewspaperLead",
    OCR_extract_evidence = "evidenceOcrExtract",
    official_source_evidence = "evidenceOfficialSource",
    private_correspondence_evidence = "evidencePrivateCorrespondence",
    private_scan_evidence = "evidencePrivateScan",
    public_registry_evidence = "evidencePublicRegistry",
    rumour_evidence = "evidenceRUMOR",
    raw_archive_evidence = "evidenceRawArchive",
    self_evidence = "evidenceSELF",
    self_controlled_site_evidence = "evidenceSelfControlledSite",
    source_code_archive_evidence = "evidenceSourceCodeArchive",
    verified_evidence = "evidenceVERIFIED",
};

export enum ExceptionTypeEnum {

    cancellation = "exceptionTypeCancellation",
    rescheduling = "exceptionTypeRescheduling",
};

export enum ExecutionStatusEnum {

    cancelled = "executionStatusCancelled",
    failed = "executionStatusFailed",
    pending = "executionStatusPending",
    running = "executionStatusRunning",
    skipped = "executionStatusSkipped",
    succeeded = "executionStatusSucceeded",
};

export enum FinancialAccountTypeEnum {

    bank_account = "accountTypeBank",
    credit_account = "accountTypeCredit",
    investment_account = "accountTypeInvestment",
    wallet = "accountTypeWallet",
};

export enum FrameKindEnum {

    cartesian = "frameKindCartesian",
    configuration_space = "frameKindConfigurationSpace",
    cylindrical = "frameKindCylindrical",
    geocoding_SOLIDUS_discrete_location_code = "frameKindGeocoding",
    geodetic = "frameKindGeodetic",
    grid = "frameKindGrid",
    Hilbert_space = "frameKindHilbert",
    latent_vector_space = "frameKindLatentSpace",
    linear_referencing_SOLIDUS_distance_along = "frameKindLinear",
    linear_sequence = "frameKindLinearSequence",
    manifold = "frameKindManifold",
    narrative = "frameKindNarrative",
    phase_space = "frameKindPhaseSpace",
    polar = "frameKindPolar",
    scalar = "frameKindScalar",
    temporal = "frameKindTemporal",
    topological = "frameKindTopological",
};

export enum FrameRealmEnum {

    biological_SOLIDUS_genomic_sequence = "frameRealmBiological",
    celestial_SOLIDUS_astronomical = "frameRealmCelestial",
    colourspace = "frameRealmColourspace",
    currency = "frameRealmCurrency",
    indoor = "frameRealmIndoor",
    linguistic = "frameRealmLinguistic",
    mathematical_SOLIDUS_n_D = "frameRealmMathematical",
    measurement = "frameRealmMeasurement",
    narrative_SOLIDUS_fictional = "frameRealmNarrative",
    perceptual = "frameRealmPerceptual",
    psychological_SOLIDUS_cognitive = "frameRealmPsychological",
    robotic = "frameRealmRobotic",
    temporal = "frameRealmTemporal",
    terrestrial = "frameRealmTerrestrial",
    virtual_SOLIDUS_network = "frameRealmVirtual",
};

export enum GenderEnum {

    agender = "genderAgender",
    bigender = "genderBigender",
    demiboy = "genderDemiboy",
    demigirl = "genderDemigirl",
    genderfluid = "genderGenderfluid",
    genderqueer = "genderGenderqueer",
    man = "genderMan",
    non_binary = "genderNonBinary",
    questioning = "genderQuestioning",
    Two_Spirit = "genderTwoSpirit",
    woman = "genderWoman",
};

export enum GenderExpressionStyleEnum {

    androgynous = "expressionAndrogynous",
    feminine = "expressionFeminine",
    fluid = "expressionFluid",
    masculine = "expressionMasculine",
    neutral = "expressionNeutral",
};

export enum GeometryTypeEnum {

    line_string = "geometryTypeLineString",
    multi_line_string = "geometryTypeMultiLineString",
    multi_point = "geometryTypeMultiPoint",
    multi_polygon = "geometryTypeMultiPolygon",
    point = "geometryTypePoint",
    polygon = "geometryTypePolygon",
};

export enum GovernanceModelEnum {

    BDFL = "governanceBDFL",
    corporate = "governanceCorporate",
    DAO = "governanceDAO",
    foundation = "governanceFoundation",
    meritocracy = "governanceMeritocracy",
};

export enum GrammaticalAspectEnum {

    none = "aspectNone",
    perfective = "aspectPerfective",
    perfective_progressive = "aspectPerfectiveProgressive",
    progressive = "aspectProgressive",
};

export enum GrammaticalTenseEnum {

    future = "tenseFuture",
    none = "tenseNone",
    past = "tensePast",
    present = "tensePresent",
};

export enum GranularityLevelEnum {

    address_level = "granularityAddress",
    century_level = "granularityCentury",
    city_level = "granularityCity",
    country_level = "granularityCountry",
    day_level = "granularityDay",
    decade_level = "granularityDecade",
    month_level = "granularityMonth",
    point_SOLIDUS_exact_coordinate_level = "granularityPoint",
    region_level = "granularityRegion",
    year_level = "granularityYear",
};

export enum HonorificEnum {

    Dame = "honorificDame",
    Dr = "honorificDr",
    Hon = "honorificHon",
    Lady = "honorificLady",
    Lord = "honorificLord",
    Mr = "honorificMr",
    Mrs = "honorificMrs",
    Ms = "honorificMs",
    Mx = "honorificMx",
    Prof = "honorificProf",
    Rev = "honorificRev",
    _sama = "honorificSama",
    _san = "honorificSan",
    Sayyid = "honorificSayyid",
    Sir = "honorificSir",
    Smt_LEFT_PARENTHESISSrimatiRIGHT_PARENTHESIS = "honorificSmt",
    Sri = "honorificSri",
};

export enum HonorificClassEnum {

    academic = "honorificClassAcademic",
    clerical = "honorificClassClerical",
    judicial = "honorificClassJudicial",
    military = "honorificClassMilitary",
    noble = "honorificClassNoble",
    social = "honorificClassSocial",
};

export enum HonorificPositionEnum {

    prefix = "honorificPositionPrefix",
    suffix = "honorificPositionSuffix",
};

export enum InvitationStatusEnum {

    accepted = "invitationStatusAccepted",
    declined = "invitationStatusDeclined",
    needs_action = "invitationStatusNeedsAction",
    tentative = "invitationStatusTentative",
};

export enum InvoiceStatusEnum {

    cancelled = "invoiceStatusCancelled",
    draft = "invoiceStatusDraft",
    overdue = "invoiceStatusOverdue",
    paid = "invoiceStatusPaid",
    sent = "invoiceStatusSent",
};

export enum KeySchemeEnum {

    Nostr = "keySchemeNostr",
    OpenPGP = "keySchemePGP",
    SSH = "keySchemeSSH",
    XFULL_STOP509 = "keySchemeX509",
};

export enum LandTenureTypeEnum {

    crown_lease = "tenureTypeCrownLease",
    easement = "tenureTypeEasement",
    freehold = "tenureTypeFreehold",
    leasehold = "tenureTypeLeasehold",
    mortgage = "tenureTypeMortgage",
    ownership = "tenureTypeOwnership",
    usufruct = "tenureTypeUsufruct",
};

export enum LanguageEnum {

    English = "languageEnglish",
    français = "languageFrench",
    普通话 = "languageMandarin",
};

export enum LanguageChangeTypeEnum {

    borrowing = "changeBorrowing",
    extinction = "changeExtinction",
    grammatical_change = "changeGrammaticalChange",
    language_contact = "changeLanguageContact",
    lexical_innovation = "changeLexicalInnovation",
    merger = "changeMerger",
    revitalization = "changeRevitalization",
    revival = "changeRevival",
    semantic_drift = "changeSemanticDrift",
    sound_shift = "changeSoundShift",
    spelling_reform = "changeSpellingReform",
    split = "changeSplit",
    standardization = "changeStandardization",
};

export enum LanguageModalityEnum {

    machine_SOLIDUS_programmatic = "modalityMachine",
    multimodal = "modalityMultimodal",
    signed = "modalitySigned",
    spoken = "modalitySpoken",
    tactile_LEFT_PARENTHESISeFULL_STOPgFULL_STOP_Braille_ProtactileRIGHT_PARENTHESIS = "modalityTactile",
    whistled = "modalityWhistled",
    written = "modalityWritten",
};

export enum LanguageOriginEnum {

    AI_SOLIDUS_machine_generated = "originAiGenerated",
    constructed_EM_DASH_artistic_LEFT_PARENTHESISeFULL_STOPgFULL_STOP_Quenya_KlingonRIGHT_PARENTHESIS = "originConstructedArtistic",
    constructed_EM_DASH_auxiliary_LEFT_PARENTHESISIAL_eFULL_STOPgFULL_STOP_EsperantoRIGHT_PARENTHESIS = "originConstructedAuxiliary",
    constructed_EM_DASH_engineered_LEFT_PARENTHESISeFULL_STOPgFULL_STOP_Lojban_IthkuilRIGHT_PARENTHESIS = "originConstructedEngineered",
    constructed_EM_DASH_ritual_SOLIDUS_liturgical = "originConstructedRitual",
    creole = "originCreole",
    formal_LEFT_PARENTHESISlogic_SOLIDUS_schemaRIGHT_PARENTHESIS = "originFormal",
    markup = "originMarkup",
    mixed_SOLIDUS_contact_language = "originMixed",
    natural = "originNatural",
    pidgin = "originPidgin",
    programming = "originProgramming",
    query = "originQuery",
    reconstructed_LEFT_PARENTHESISproto_languageRIGHT_PARENTHESIS = "originReconstructed",
};

export enum LanguageStatusEnum {

    constructed_EM_DASH_actively_used = "statusConstructedActive",
    dormant = "statusDormant",
    emerging = "statusEmerging",
    extinct = "statusExtinct",
    historical = "statusHistorical",
    living = "statusLiving",
    proposed = "statusProposed",
    revived = "statusRevived",
};

export enum LanguageVarietyKindEnum {

    creole = "kindCreole",
    dialect = "kindDialect",
    idiolect = "kindIdiolect",
    jargon = "kindJargon",
    koine = "kindKoine",
    language_LEFT_PARENTHESISstandpointed_classificationRIGHT_PARENTHESIS = "kindLanguage",
    lingua_franca = "kindLinguaFranca",
    localized_variant = "kindLocalizedVariant",
    pidgin = "kindPidgin",
    register = "kindRegister",
    slang = "kindSlang",
    sociolect = "kindSociolect",
    standard = "kindStandard",
};

export enum LedgerAccountTypeEnum {

    asset = "ledgerAccountTypeAsset",
    equity = "ledgerAccountTypeEquity",
    expense = "ledgerAccountTypeExpense",
    liability = "ledgerAccountTypeLiability",
    revenue = "ledgerAccountTypeRevenue",
};

export enum LedgerFinalityStatusEnum {

    confirmed = "finalityStatusConfirmed",
    finalized = "finalityStatusFinalized",
    orphaned = "finalityStatusOrphaned",
    pending = "finalityStatusPending",
    reorged = "finalityStatusReorged",
};

export enum LeftOperandEnum {

    absolute_position = "leftOpAbsolutePosition",
    absolute_size = "leftOpAbsoluteSize",
    absolute_spatial_position = "leftOpAbsoluteSpatialPosition",
    absolute_temporal_position = "leftOpAbsoluteTemporalPosition",
    use_count = "leftOpCount",
    dateSOLIDUStime = "leftOpDateTime",
    delay_period = "leftOpDelayPeriod",
    delivery_channel = "leftOpDeliveryChannel",
    device = "leftOpDevice",
    elapsed_time = "leftOpElapsedTime",
    event = "leftOpEvent",
    file_format = "leftOpFileFormat",
    industry = "leftOpIndustry",
    language = "leftOpLanguage",
    media_context = "leftOpMedia",
    metered_time = "leftOpMeteredTime",
    pay_amount = "leftOpPayAmount",
    percentage = "leftOpPercentage",
    product_context = "leftOpProduct",
    purpose = "leftOpPurpose",
    recipient = "leftOpRecipient",
    relative_position = "leftOpRelativePosition",
    relative_size = "leftOpRelativeSize",
    relative_spatial_position = "leftOpRelativeSpatialPosition",
    relative_temporal_position = "leftOpRelativeTemporalPosition",
    rendition_resolution = "leftOpResolution",
    spatial_region = "leftOpSpatial",
    spatial_coordinates = "leftOpSpatialCoordinates",
    system = "leftOpSystem",
    system_device = "leftOpSystemDevice",
    recurring_time_interval = "leftOpTimeInterval",
    unit_of_count = "leftOpUnitOfCount",
    asset_version = "leftOpVersion",
    virtual_location = "leftOpVirtualLocation",
};

export enum LexicalFormTypeEnum {

    normalized = "formNormalized",
    reconstructed = "formReconstructed",
    rendered = "formRendered",
    signed = "formSigned",
    spoken = "formSpoken",
    translated = "formTranslated",
    transliterated = "formTransliterated",
    written = "formWritten",
};

export enum LicenseFamilyEnum {

    Creative_Commons = "licenseFamilyCC",
    copyleft = "licenseFamilyCopyleft",
    dual_licensed = "licenseFamilyDual",
    permissive = "licenseFamilyPermissive",
    proprietary = "licenseFamilyProprietary",
    public_domain = "licenseFamilyPublicDomain",
};

export enum MaintenanceStatusEnum {

    abandoned = "statusAbandoned",
    active = "statusActive",
    deprecated = "statusDeprecated",
    end_of_life = "statusEOL",
    maintained = "statusMaintained",
};

export enum ManifestationFormatEnum {

    audiobook = "formatAudiobook",
    comic_issue = "formatComicIssue",
    digital_file = "formatDigitalFile",
    EPUB = "formatEPUB",
    hardcover = "formatHardcover",
    PDF = "formatPDF",
    paperback = "formatPaperback",
    vinyl = "formatVinyl",
    web_page = "formatWebPage",
    web_serial = "formatWebSerial",
};

export enum MaximViolationTypeEnum {

    maxim_violation_EM_DASH_manner = "maximViolationManner",
    maxim_violation_EM_DASH_quality = "maximViolationQuality",
    maxim_violation_EM_DASH_quantity = "maximViolationQuantity",
    maxim_violation_EM_DASH_relation = "maximViolationRelation",
};

export enum MentalReferenceFrameEnum {

    Russell_Affective_Circumplex_Reference_Frame = "referenceFrameAffectiveCircumplex",
    Allocentric_Cognitive_Map_Reference_Frame = "referenceFrameCognitiveMapAllocentric",
    Egocentric_Cognitive_Map_Reference_Frame = "referenceFrameCognitiveMapEgocentric",
    Gärdenfors_Conceptual_Space_Reference_Frame = "referenceFrameConceptualSpace",
    Imagined_Space_Reference_Frame = "referenceFrameImaginedSpace",
    ASHRAE_Thermal_Comfort_Reference_Frame = "referenceFrameThermalComfort",
};

export enum MessageKeywordEnum {

    answered = "keywordAnswered",
    draft = "keywordDraft",
    flagged = "keywordFlagged",
    forwarded = "keywordForwarded",
    junk = "keywordJunk",
    seen = "keywordSeen",
};

export enum MessageKindEnum {

    auto_generated = "messageKindAutoGenerated",
    bounce = "messageKindBounce",
    calendar_invitation = "messageKindCalendarInvitation",
    delivery_status_notification = "messageKindDeliveryStatusNotification",
    feedback_report = "messageKindFeedbackReport",
    read_receipt = "messageKindReadReceipt",
};

export enum MessageParticipantRoleEnum {

    bcc = "messageRoleBcc",
    cc = "messageRoleCc",
    delivered_to = "messageRoleDeliveredTo",
    envelope_from = "messageRoleEnvelopeFrom",
    envelope_to = "messageRoleEnvelopeTo",
    errors_to = "messageRoleErrorsTo",
    from = "messageRoleFrom",
    original_to = "messageRoleOriginalTo",
    reply_to = "messageRoleReplyTo",
    resent_cc = "messageRoleResentCc",
    resent_from = "messageRoleResentFrom",
    resent_to = "messageRoleResentTo",
    return_path = "messageRoleReturnPath",
    sender = "messageRoleSender",
    to = "messageRoleTo",
};

export enum MetricKindEnum {

    cosine_similarity = "metricCosine",
    edit_distance = "metricEditDistance",
    Euclidean = "metricEuclidean",
    geodesic = "metricGeodesic",
    graph_hops = "metricGraphHops",
    positional_distance = "metricPositionalDistance",
    phase_space_Euclidean = "metricSymplectic",
};

export enum MultipartTypeEnum {

    alternative = "multipartTypeAlternative",
    digest = "multipartTypeDigest",
    encrypted = "multipartTypeEncrypted",
    mixed = "multipartTypeMixed",
    parallel = "multipartTypeParallel",
    related = "multipartTypeRelated",
    report = "multipartTypeReport",
    signed = "multipartTypeSigned",
};

export enum NamePartTypeEnum {

    agnomen_LEFT_PARENTHESISRoman_earned_epithetRIGHT_PARENTHESIS = "namePartAgnomen",
    birth_order_SOLIDUS_day_name = "namePartBirthOrderName",
    birth_surname_SOLIDUS_maiden_name = "namePartBirthSurname",
    clan_SOLIDUS_lineage_name = "namePartClanName",
    cognomen_LEFT_PARENTHESISRoman_family_branchRIGHT_PARENTHESIS = "namePartCognomen",
    courtesy_SOLIDUS_art_name = "namePartCourtesyName",
    filename_extension = "namePartExtension",
    generation_name_LEFT_PARENTHESISEast_Asian_lineage_markerRIGHT_PARENTHESIS = "namePartGenerationName",
    generational_ordinal_LEFT_PARENTHESISIII_SOLIDUS_APOSTROPHEthe_ThirdAPOSTROPHERIGHT_PARENTHESIS = "namePartGenerationalOrdinal",
    generational_suffix_LEFT_PARENTHESISJr_SOLIDUS_SrRIGHT_PARENTHESIS = "namePartGenerationalSuffix",
    given_name = "namePartGiven",
    honorific_prefix = "namePartHonorificPrefix",
    honorific_suffix = "namePartHonorificSuffix",
    house_SOLIDUS_estate_name = "namePartHouseName",
    expandable_initial = "namePartInitial",
    ism_LEFT_PARENTHESISArabic_personal_nameRIGHT_PARENTHESIS = "namePartIsm",
    kunya_LEFT_PARENTHESISArabic_teknonymRIGHT_PARENTHESIS = "namePartKunya",
    laqab_LEFT_PARENTHESISArabic_epithetRIGHT_PARENTHESIS = "namePartLaqab",
    maternal_surname = "namePartMaternalSurname",
    matronymic = "namePartMatronymic",
    middle_SOLIDUS_additional_name = "namePartMiddle",
    mononym = "namePartMononym",
    nasab_LEFT_PARENTHESISArabic_patronymic_lineageRIGHT_PARENTHESIS = "namePartNasab",
    nickname_SOLIDUS_hypocorism = "namePartNickname",
    nisba_LEFT_PARENTHESISArabic_origin_SOLIDUS_affiliation_nameRIGHT_PARENTHESIS = "namePartNisba",
    nomen_LEFT_PARENTHESISRoman_gens_SOLIDUS_clan_nameRIGHT_PARENTHESIS = "namePartNomen",
    nobiliary_SOLIDUS_nominal_particle = "namePartParticle",
    paternal_surname = "namePartPaternalSurname",
    patronymic = "namePartPatronymic",
    praenomen_LEFT_PARENTHESISRoman_personal_nameRIGHT_PARENTHESIS = "namePartPraenomen",
    religious_SOLIDUS_regnal_name = "namePartReligiousName",
    filename_stem = "namePartStem",
    surname_SOLIDUS_family_name = "namePartSurname",
    teknonym_LEFT_PARENTHESISparent_of_nameRIGHT_PARENTHESIS = "namePartTeknonym",
};

export enum NamePurposeEnum {

    birth_name = "namePurposeBirth",
    ceremonial_name = "namePurposeCeremonial",
    chosen_SOLIDUS_self_identified_name = "namePurposeChosen",
    deadname_LEFT_PARENTHESIShistorical_do_not_displayRIGHT_PARENTHESIS = "namePurposeDeadname",
    endonym_LEFT_PARENTHESISname_used_by_a_placeAPOSTROPHEs_own_inhabitants_SOLIDUS_a_languageAPOSTROPHEs_own_speakersRIGHT_PARENTHESIS = "namePurposeEndonym",
    exonym_LEFT_PARENTHESISname_used_by_outsiders_SOLIDUS_in_another_languageRIGHT_PARENTHESIS = "namePurposeExonym",
    glossonym_LEFT_PARENTHESISname_of_a_languageRIGHT_PARENTHESIS = "namePurposeGlossonym",
    legal_name = "namePurposeLegal",
    nickname_SOLIDUS_familiar_name = "namePurposeNickname",
    online_handle_SOLIDUS_username = "namePurposeOnlineHandle",
    pen_SOLIDUS_stage_name = "namePurposePenStage",
    professional_name = "namePurposeProfessional",
    regnal_name = "namePurposeRegnal",
    religious_name = "namePurposeReligious",
    superseded_SOLIDUS_former_name = "namePurposeSuperseded",
};

export enum NameRegisterEnum {

    casual = "registerCasual",
    formal = "registerFormal",
    intimate_SOLIDUS_familial = "registerIntimate",
    professional = "registerProfessional",
};

export enum NamedPeriodEnum {

    Cenozoic_Era = "periodCenozoic",
    Holocene_Epoch = "periodHolocene",
    Phanerozoic_Eon = "periodPhanerozoic",
    Quaternary_Period = "periodQuaternary",
};

export enum NarrativeFrameRelationEnum {

    adaptation_of = "relationAdaptationOf",
    alternate_continuity = "relationAlternateContinuity",
    canon = "relationCanon",
    crossover = "relationCrossover",
    expanded_universe = "relationExpandedUniverse",
    fanon = "relationFanon",
};

export enum NetworkAddressTypeEnum {

    BGP_autonomous_system = "networkAddressTypeBGP",
    DNS_name = "networkAddressTypeDNS",
    IPv4_address = "networkAddressTypeIPv4",
    IPv6_address = "networkAddressTypeIPv6",
    MAC_address = "networkAddressTypeMAC",
    port_number = "networkAddressTypePort",
    URL = "networkAddressTypeURL",
};

export enum NotationUsageRoleEnum {

    cipher = "notationRoleCipher",
    communication = "notationRoleCommunication",
    encoding = "notationRoleEncoding",
    expression = "notationRoleExpression",
    representation = "notationRoleRepresentation",
    shorthand = "notationRoleShorthand",
    transcription = "notationRoleTranscription",
};

export enum ObservablePropertyEnum {

    air_quality_index = "observablePropertyAirQualityIndex",
    atmospheric_pressure = "observablePropertyAtmosphericPressure",
    humidity = "observablePropertyHumidity",
    light_intensity = "observablePropertyLightIntensity",
    radiation_level = "observablePropertyRadiationLevel",
    sound_pressure_level = "observablePropertySoundPressureLevel",
    temperature = "observablePropertyTemperature",
};

export enum ObservationMethodEnum {

    computational_model = "methodComputationalModel",
    direct_observation = "methodDirectObservation",
    expert_judgement = "methodExpertJudgement",
    GNSS_RTK_survey = "methodGNSSRTK",
    GPS_survey = "methodGPS",
    instrumental_reading = "methodInstrumentalReading",
    LiDAR_survey = "methodLiDAR",
    photogrammetry = "methodPhotogrammetry",
    remote_sensing = "methodRemoteSensing",
    streaming = "methodStreaming",
    survey = "methodSurvey",
    total_station_survey = "methodTotalStation",
};

export enum ObservationTypeEnum {

    derived_inference = "observationTypeDerived",
    identity_claim = "observationTypeIdentity",
    kinship_claim = "observationTypeKinship",
    measurement = "observationTypeMeasurement",
    naming_claim = "observationTypeNaming",
    rights_claim = "observationTypeRights",
    sensory_reading = "observationTypeSensory",
    simulation_output = "observationTypeSimulation",
    standpoint_claim = "observationTypeStandpoint",
    streaming = "observationTypeStreaming",
};

export enum OrderStatusEnum {

    cancelled = "orderStatusCancelled",
    confirmed = "orderStatusConfirmed",
    delivered = "orderStatusDelivered",
    pending = "orderStatusPending",
    shipped = "orderStatusShipped",
};

export enum OrganizationEnum {

    International_Commission_on_Stratigraphy = "agentInternationalCommissionOnStratigraphy",
};

export enum OrganizationTypeEnum {

    association = "organizationTypeAssociation",
    collaboration = "organizationTypeCollaboration",
    company = "organizationTypeCompany",
    educational_institution = "organizationTypeEducationalInstitution",
    government_body = "organizationTypeGovernmentBody",
    nonprofit = "organizationTypeNonprofit",
};

export enum ParticipantRoleEnum {

    agent = "roleAgent",
    attendee = "roleAttendee",
    beneficiary = "roleBeneficiary",
    beneficiary_of_deception = "roleBeneficiaryOfDeception",
    deceived = "roleDeceived",
    deceiver = "roleDeceiver",
    dupe = "roleDupe",
    employee = "roleEmployee",
    employer = "roleEmployer",
    intermediary = "roleIntermediary",
    officiant = "roleOfficiant",
    organizer = "roleOrganizer",
    principal_SOLIDUS_subject = "roleParticipantPrincipal",
    payee = "rolePayee",
    payer = "rolePayer",
    performer = "rolePerformer",
    spin_doctor = "roleSpinDoctor",
    victim = "roleVictim",
    witness = "roleWitness",
};

export enum PaymentMethodEnum {

    bank_transfer = "paymentMethodBankTransfer",
    cash = "paymentMethodCash",
    cheque = "paymentMethodCheque",
    credit_card = "paymentMethodCreditCard",
    cryptocurrency = "paymentMethodCrypto",
};

export enum PeriodTypeEnum {

    fiscal_year = "periodTypeFiscalYear",
    geologic_age = "periodTypeGeologicAge",
    geologic_eon = "periodTypeGeologicEon",
    geologic_epoch = "periodTypeGeologicEpoch",
    geologic_era = "periodTypeGeologicEra",
    geologic_period = "periodTypeGeologicPeriod",
    historical_dynasty = "periodTypeHistoricalDynasty",
    historical_era = "periodTypeHistoricalEra",
};

export enum PhysicalCarrierTypeEnum {

    bone = "carrierBone",
    coin = "carrierCoin",
    manuscript = "carrierManuscript",
    metal = "carrierMetal",
    ostracon = "carrierOstracon",
    papyrus = "carrierPapyrus",
    pottery_sherd = "carrierPotterySherd",
    seal = "carrierSeal",
    stela = "carrierStela",
    tablet = "carrierTablet",
    wall_inscription = "carrierWallInscription",
    wood = "carrierWood",
};

export enum PlaceTypeEnum {

    administrative_area = "placeTypeAdministrativeArea",
    building = "placeTypeBuilding",
    city_SOLIDUS_populated_place = "placeTypeCity",
    country = "placeTypeCountry",
    floor_SOLIDUS_level = "placeTypeFloor",
    natural_feature = "placeTypeNaturalFeature",
    neighborhood = "placeTypeNeighborhood",
    parcel_SOLIDUS_lot = "placeTypeParcel",
    point_of_interest = "placeTypePointOfInterest",
    premises_SOLIDUS_address_point = "placeTypePremises",
    region_SOLIDUS_state_SOLIDUS_province = "placeTypeRegion",
    room = "placeTypeRoom",
    site_SOLIDUS_campus = "placeTypeSite",
    thoroughfare_SOLIDUS_street = "placeTypeThoroughfare",
};

export enum PostingDirectionEnum {

    credit = "postingDirectionCredit",
    debit = "postingDirectionDebit",
};

export enum ProcedureEnum {

    Canonical_Ingestion_Procedure = "procedureIngestionCanonical",
};

export enum ProcedureStepEnum {

    derived_claims_SOLIDUS_events_generation = "stepIngestionDerivedClaims",
    file_copy_SOLIDUS_staging = "stepIngestionFileCopy",
    OCR_SOLIDUS_text_extraction = "stepIngestionOcrExtract",
    privacy_posture_assessment = "stepIngestionPrivacyPosture",
    raw_root_acquisition = "stepIngestionRawRoot",
    unresolved_leads_identification = "stepIngestionUnresolvedLeads",
};

export enum ProcedureTypeEnum {

    agent_flow = "procedureTypeAgentFlow",
    business_process = "procedureTypeBusinessProcess",
    CI_build = "procedureTypeCiBuild",
    data_pipeline = "procedureTypeDataPipeline",
    ingestion = "procedureTypeIngestion",
    lab_protocol = "procedureTypeLabProtocol",
    recipe = "procedureTypeRecipe",
    research_plan = "procedureTypeResearchPlan",
};

export enum ProficiencyLevelEnum {

    assessed_beginner = "assessedBeginner",
    assessed_competent = "assessedCompetent",
    assessed_expert = "assessedExpert",
    CEFR_A1 = "cefrA1",
    CEFR_A2 = "cefrA2",
    CEFR_B1 = "cefrB1",
    CEFR_B2 = "cefrB2",
    CEFR_C1 = "cefrC1",
    CEFR_C2 = "cefrC2",
    Dreyfus_advanced_beginner = "dreyfusAdvancedBeginner",
    Dreyfus_competent = "dreyfusCompetent",
    Dreyfus_expert = "dreyfusExpert",
    Dreyfus_novice = "dreyfusNovice",
    Dreyfus_proficient = "dreyfusProficient",
    heritage = "levelHeritage",
    native = "levelNative",
    NIH_advanced = "nihAdvanced",
    NIH_beginner = "nihBeginner",
    NIH_expert = "nihExpert",
    NIH_intermediate = "nihIntermediate",
};

export enum ProficiencyModalityEnum {

    comprehension = "profModalityComprehension",
    listening = "profModalityListening",
    overall = "profModalityOverall",
    reading = "profModalityReading",
    signing = "profModalitySigning",
    speaking = "profModalitySpeaking",
    writing = "profModalityWriting",
};

export enum ProficiencyScaleEnum {

    ACTFL = "scaleACTFL",
    assessed = "scaleAssessed",
    CEFR = "scaleCEFR",
    Dreyfus = "scaleDreyfus",
    ILR = "scaleILR",
    NIH = "scaleNIH",
    self_reported = "scaleSelfReported",
};

export enum ProfileEnum {

    Reference_Frame_Profile = "profileReferenceFrame",
    Temporal_Frame_Profile = "profileTemporalFrame",
    Temporal_Provenance_Profile_LEFT_PARENTHESISfour_clocksRIGHT_PARENTHESIS = "profileTemporalProvenance",
};

export enum ProjectionContextEnum {

    agent_memory = "consumerAgentMemory",
    FOAF_export = "consumerFoafExport",
    internal_archive = "consumerInternalArchive",
    public_site = "consumerPublicSite",
    research_queue = "consumerResearchQueue",
    schemaFULL_STOPorg_JSON_LD = "consumerSchemaOrgJsonLd",
    Wikidata = "consumerWikidata",
    Wikipedia = "consumerWikipedia",
};

export enum PronounSetEnum {

    aeSOLIDUSaer = "pronounAeAer",
    any_pronouns = "pronounAny",
    ask_me = "pronounAsk",
    coSOLIDUScos = "pronounCoCos",
    eSOLIDUSem_LEFT_PARENTHESISSpivakRIGHT_PARENTHESIS = "pronounEEm",
    eySOLIDUSem_LEFT_PARENTHESISElversonRIGHT_PARENTHESIS = "pronounEyEm",
    faeSOLIDUSfaer = "pronounFaeFaer",
    heSOLIDUShim = "pronounHeHim",
    huSOLIDUShum = "pronounHuHum",
    itSOLIDUSits = "pronounItIts",
    kiSOLIDUSkin = "pronounKiKin",
    use_my_name_LEFT_PARENTHESISno_pronounsRIGHT_PARENTHESIS = "pronounNameOnly",
    neSOLIDUSnem = "pronounNeNem",
    oneSOLIDUSone_LEFT_PARENTHESISgenericRIGHT_PARENTHESIS = "pronounOneOne",
    perSOLIDUSper = "pronounPerPer",
    sheSOLIDUSher = "pronounSheHer",
    theySOLIDUSthem_LEFT_PARENTHESISsingularRIGHT_PARENTHESIS = "pronounTheyThem",
    thonSOLIDUSthon = "pronounThonThon",
    veSOLIDUSver = "pronounVeVer",
    viSOLIDUSvir = "pronounViVir",
    xeSOLIDUSxem = "pronounXeXem",
    zeSOLIDUShir = "pronounZeHir",
    zeSOLIDUSzir = "pronounZeZir",
    zheSOLIDUSzher = "pronounZheZher",
};

export enum QualityDimensionEnum {

    completeness = "qualityDimensionCompleteness",
    lineage = "qualityDimensionLineage",
    logical_consistency = "qualityDimensionLogicalConsistency",
    positional_accuracy = "qualityDimensionPositionalAccuracy",
    temporal_accuracy = "qualityDimensionTemporalAccuracy",
    thematic_accuracy = "qualityDimensionThematicAccuracy",
    topological_consistency = "qualityDimensionTopologicalConsistency",
};

export enum ReferenceFrameEnum {

    Australian_Dollar_Currency_Reference_Frame = "referenceFrameAUD",
    Altitude_Above_Ground_Level_Reference_Frame = "referenceFrameAltitudeAGL",
    Altitude_Above_Mean_Sea_Level_Reference_Frame = "referenceFrameAltitudeMSL",
    Audio_Spectrum_Reference_Frame = "referenceFrameAudioSpectrum",
    BGP_Autonomous_System_Reference_Frame = "referenceFrameBGP",
    Bitcoin_Currency_Reference_Frame = "referenceFrameBTC",
    Canadian_Dollar_Currency_Reference_Frame = "referenceFrameCAD",
    Swiss_Franc_Currency_Reference_Frame = "referenceFrameCHF",
    CIE_LASTERISKaASTERISKbASTERISK_Perceptually_Uniform_Reference_Frame = "referenceFrameCIELAB",
    CIE_1931_XYZ_Tristimulus_Reference_Frame = "referenceFrameCIEXYZ",
    CMYK_Colourspace_Reference_Frame = "referenceFrameCMYK",
    Chinese_Yuan_Currency_Reference_Frame = "referenceFrameCNY",
    Celestial_Equatorial_Reference_Frame = "referenceFrameCelestialEquatorial",
    DNS_Name_Space_Reference_Frame = "referenceFrameDNS",
    Depth_Below_Chart_Datum_Reference_Frame = "referenceFrameDepthBelowChartDatum",
    Depth_Below_Mean_Sea_Level_Reference_Frame = "referenceFrameDepthBelowSeaLevel",
    Ethereum_Currency_Reference_Frame = "referenceFrameETH",
    Euro_Currency_Reference_Frame = "referenceFrameEUR",
    English_Language_Reference_Frame = "referenceFrameEnglish",
    FK5_Equatorial_Reference_Frame = "referenceFrameFK5",
    ICAO_Flight_Level_Reference_Frame = "referenceFrameFlightLevel",
    British_Pound_Currency_Reference_Frame = "referenceFrameGBP",
    GRCh38_Human_Reference_Assembly = "referenceFrameGRCh38",
    Galactic_Coordinate_Reference_Frame = "referenceFrameGalactic",
    Geohash_Reference_Frame = "referenceFrameGeohash",
    Gregorian_Calendar_Reference_Frame = "referenceFrameGregorian",
    Hilbert_Space_Reference_Frame = "referenceFrameHilbertSpace",
    ICRS_Celestial_Reference_Frame = "referenceFrameICRS",
    IPv4_Address_Space_Reference_Frame = "referenceFrameIPv4",
    IPv6_Address_Space_Reference_Frame = "referenceFrameIPv6",
    Internet_Topology_Reference_Frame = "referenceFrameInternet",
    Japanese_Yen_Currency_Reference_Frame = "referenceFrameJPY",
    Latent_Vector_Space_Reference_Frame = "referenceFrameLatentVectorSpace",
    Local_Grid_Cartesian_Reference_Frame = "referenceFrameLocalGrid",
    MAC_Address_Space_Reference_Frame = "referenceFrameMAC",
    MGRS_Reference_Frame = "referenceFrameMGRS",
    Linear_Referencing_LEFT_PARENTHESISMile_MarkerRIGHT_PARENTHESIS_Reference_Frame = "referenceFrameMileMarker",
    Network_Graph_Reference_Frame = "referenceFrameNetworkGraph",
    Abstract_Phase_Space_Reference_Frame_LEFT_PARENTHESISqp_axesRIGHT_PARENTHESIS = "referenceFramePhaseSpace3DOF",
    Plus_Code_LEFT_PARENTHESISOpen_Location_CodeRIGHT_PARENTHESIS_Reference_Frame = "referenceFramePlusCode",
    Port_Number_Space_Reference_Frame = "referenceFramePort",
    Postal_Address_Reference_Frame = "referenceFramePostalAddress",
    number_6_DOF_Robot_Arm_Configuration_Space = "referenceFrameRobotArm6DOF",
    Robot_Base_Cartesian_Reference_Frame = "referenceFrameRobotBase",
    number_6_DOF_Robot_Configuration_Space_Reference_Frame = "referenceFrameRobotCspace6DOF",
    Robot_SLAM_Occupancy_Grid_Reference_Frame = "referenceFrameRobotSLAM",
    Robot_End_Effector_Task_Space_Reference_Frame = "referenceFrameRobotTaskSpace",
    Robot_Velocity_Reference_Frame = "referenceFrameRobotVelocity",
    SI_Measurement_Reference_Frame = "referenceFrameSI",
    sRGB_Colourspace_Reference_Frame = "referenceFrameSRGB",
    UNSOLIDUSLOCODE_Reference_Frame = "referenceFrameUNLocode",
    URL_Space_Reference_Frame = "referenceFrameURL",
    US_Dollar_Currency_Reference_Frame = "referenceFrameUSD",
    Unix_Epoch_Timestamp_Reference_Frame = "referenceFrameUnixEpoch",
    Virtual_Platform_Reference_Frame = "referenceFrameVirtualPlatform",
    WGS_84_Geodetic_Reference_Frame = "referenceFrameWGS84",
    what3words_Reference_Frame = "referenceFrameWhat3Words",
};

export enum RegulatoryOverlayTypeEnum {

    aerodrome_traffic_zone_LEFT_PARENTHESISATZRIGHT_PARENTHESIS = "overlayTypeAerodromeTrafficZone",
    airway = "overlayTypeAirway",
    alert_area = "overlayTypeAlertArea",
    civil_time_zone = "overlayTypeCivilTimeZone",
    contiguous_zone = "overlayTypeContiguousZone",
    continental_shelf = "overlayTypeContinentalShelf",
    control_zone_LEFT_PARENTHESISCTRRIGHT_PARENTHESIS = "overlayTypeControlZone",
    customs_zone = "overlayTypeCustomsZone",
    electoral_district = "overlayTypeElectoralDistrict",
    fishing_zone_SOLIDUS_EEZ = "overlayTypeFishingZone",
    flight_information_region_LEFT_PARENTHESISFIRRIGHT_PARENTHESIS = "overlayTypeFlightInformationRegion",
    high_seas = "overlayTypeHighSeas",
    marine_protected_area = "overlayTypeMarineProtectedArea",
    military_operations_area_LEFT_PARENTHESISMOARIGHT_PARENTHESIS = "overlayTypeMilitaryOperationsArea",
    notice_to_air_missions_LEFT_PARENTHESISNOTAMRIGHT_PARENTHESIS = "overlayTypeNOTAM",
    postal_zone = "overlayTypePostalZone",
    protected_area = "overlayTypeProtectedArea",
    restricted_airspace = "overlayTypeRestrictedAirspace",
    sanctions_SOLIDUS_embargo = "overlayTypeSanctions",
    tax_district = "overlayTypeTaxDistrict",
    terminal_control_area_LEFT_PARENTHESISTMASOLIDUSTCARIGHT_PARENTHESIS = "overlayTypeTerminalControlArea",
    territorial_sea = "overlayTypeTerritorialSea",
    warning_area = "overlayTypeWarningArea",
    zoning_SOLIDUS_land_use_regulation = "overlayTypeZoning",
};

export enum ReminderActionEnum {

    audio = "reminderActionAudio",
    display = "reminderActionDisplay",
    email = "reminderActionEmail",
};

export enum RepositoryTypeEnum {

    fossil = "repoTypeFossil",
    git = "repoTypeGit",
    mercurial = "repoTypeHg",
    jujutsu = "repoTypeJJ",
    pijul = "repoTypePijul",
    subversion = "repoTypeSVN",
};

export enum RightsActionEnum {

    accept_tracking = "actionAcceptTracking",
    aggregate = "actionAggregate",
    annotate = "actionAnnotate",
    anonymize = "actionAnonymize",
    archive = "actionArchive",
    attribute = "actionAttribute",
    commercialize = "actionCommercialize",
    compensate = "actionCompensate",
    concurrent_use = "actionConcurrentUse",
    delete = "actionDelete",
    derive_SOLIDUS_modify = "actionDerive",
    digitize = "actionDigitize",
    display = "actionDisplay",
    distribute = "actionDistribute",
    ensure_exclusivity = "actionEnsureExclusivity",
    execute = "actionExecute",
    extract = "actionExtract",
    give = "actionGive",
    grant_use = "actionGrantUse",
    include = "actionInclude",
    index = "actionIndex",
    inform = "actionInform",
    install = "actionInstall",
    lease = "actionLease",
    lend = "actionLend",
    modify = "actionModify",
    move = "actionMove",
    next_policy = "actionNextPolicy",
    obtain_consent = "actionObtainConsent",
    play = "actionPlay",
    present_SOLIDUS_display = "actionPresent",
    print = "actionPrint",
    process_personal_data = "actionProcessPersonalData",
    read = "actionRead",
    reproduce = "actionReproduce",
    retain_notice = "actionRetainNotice",
    review_policy = "actionReviewPolicy",
    sell = "actionSell",
    share_alike = "actionShareAlike",
    stream = "actionStream",
    synchronize = "actionSynchronize",
    text_to_speech = "actionTextToSpeech",
    transfer = "actionTransfer",
    transform = "actionTransform",
    translate = "actionTranslate",
    uninstall = "actionUninstall",
    use = "actionUse",
    watermark = "actionWatermark",
};

export enum RightsTypeEnum {

    copyright = "rightsTypeCopyright",
    database_right = "rightsTypeDatabaseRight",
    industrial_design_right = "rightsTypeIndustrialDesign",
    moral_rights = "rightsTypeMoralRights",
    patent = "rightsTypePatent",
    plant_breedersAPOSTROPHE_rights = "rightsTypePlantBreedersRights",
    related_rights = "rightsTypeRelatedRights",
    trade_secret = "rightsTypeTradeSecret",
    trademark = "rightsTypeTrademark",
};

export enum RomanticOrientationValueEnum {

    aromantic = "romanticAromantic",
    biromantic = "romanticBiromantic",
    demiromantic = "romanticDemiromantic",
    heteroromantic = "romanticHeteroromantic",
    homoromantic = "romanticHomoromantic",
    panromantic = "romanticPanromantic",
    queerplatonic_SOLIDUS_queer_romantic = "romanticQueerromantic",
    questioning = "romanticQuestioning",
};

export enum RouteKindEnum {

    accessible_route = "routeKindAccessible",
    citation_chain = "routeKindCitation",
    dependency_chain = "routeKindDependency",
    flight_path = "routeKindFlight",
    network_path = "routeKindNetwork",
    social_path = "routeKindSocial",
    transit_route = "routeKindTransit",
    walking_route = "routeKindWalking",
};

export enum RsvpStatusEnum {

    accepted = "rsvpStatusAccepted",
    declined = "rsvpStatusDeclined",
    needs_action = "rsvpStatusNeedsAction",
    tentative = "rsvpStatusTentative",
};

export enum SLSALevelEnum {

    SLSA_Level_1 = "slsaLevel1",
    SLSA_Level_2 = "slsaLevel2",
    SLSA_Level_3 = "slsaLevel3",
    SLSA_Level_4 = "slsaLevel4",
};

export enum SceneRelationTypeEnum {

    above = "sceneRelationAbove",
    below = "sceneRelationBelow",
    eating = "sceneRelationEating",
    far_from = "sceneRelationFarFrom",
    holding = "sceneRelationHolding",
    inside = "sceneRelationInside",
    left_of = "sceneRelationLeftOf",
    near = "sceneRelationNear",
    part_of = "sceneRelationPartOf",
    playing = "sceneRelationPlaying",
    riding = "sceneRelationRiding",
    right_of = "sceneRelationRightOf",
    same_as = "sceneRelationSameAs",
    touching = "sceneRelationTouching",
    wearing = "sceneRelationWearing",
};

export enum ScriptRoleEnum {

    decorative = "scriptRoleDecorative",
    historical_SOLIDUS_superseded = "scriptRoleHistorical",
    liturgical = "scriptRoleLiturgical",
    loanword_SOLIDUS_foreign_term = "scriptRoleLoanword",
    logographic_content = "scriptRoleLogographicContent",
    primary = "scriptRolePrimary",
    syllabic_grammar_SOLIDUS_inflection = "scriptRoleSyllabicGrammar",
    transliteration_SOLIDUS_romanization = "scriptRoleTransliteration",
};

export enum SelectorTypeEnum {

    COCO_RLE_mask = "selectorTypeCocoRleMask",
    DICOM_SEG_mask = "selectorTypeDicomSegMask",
    fractional_rectangle = "selectorTypeFractionalRectangle",
    pixel_mask = "selectorTypePixelMask",
    pixel_rectangle = "selectorTypePixelRectangle",
    polygon_path = "selectorTypePolygonPath",
    run_length_encoded = "selectorTypeRunLengthEncoded",
    SVG_path = "selectorTypeSvgPath",
    Web_Annotation_fragment = "selectorTypeWebAnnotationFragment",
};

export enum SeniorityLevelEnum {

    entry_level = "seniorityEntry",
    executive = "seniorityExecutive",
    lead = "seniorityLead",
    mid_level = "seniorityMid",
    senior = "senioritySenior",
};

export enum SensitivityLevelEnum {

    confidential = "sensitivityConfidential",
    internal = "sensitivityInternal",
    public = "sensitivityPublic",
    restricted = "sensitivityRestricted",
    sensitive_personal = "sensitivitySensitivePersonal",
};

export enum SensoryModalityEnum {

    air_quality = "sensoryModalityAirQuality",
    auditory = "sensoryModalityAuditory",
    gustatory = "sensoryModalityGustatory",
    olfactory = "sensoryModalityOlfactory",
    tactile = "sensoryModalityTactile",
    thermal = "sensoryModalityThermal",
    visual = "sensoryModalityVisual",
};

export enum SequenceFeatureTypeEnum {

    coding_sequence_LEFT_PARENTHESISCDSRIGHT_PARENTHESIS = "sequenceFeatureTypeCDS",
    chromosome = "sequenceFeatureTypeChromosome",
    exon = "sequenceFeatureTypeExon",
    gene = "sequenceFeatureTypeGene",
    intron = "sequenceFeatureTypeIntron",
    single_nucleotide_polymorphism_LEFT_PARENTHESISSNPRIGHT_PARENTHESIS = "sequenceFeatureTypeSNP",
};

export enum SexAssignedAtBirthEnum {

    assigned_female = "saabFemale",
    intersex = "saabIntersex",
    assigned_male = "saabMale",
    unknown_SOLIDUS_not_recorded = "saabUnknown",
};

export enum SexualOrientationValueEnum {

    asexual = "orientAsexual",
    bisexual = "orientBisexual",
    demisexual = "orientDemisexual",
    heterosexual = "orientHeterosexual",
    homosexual_SOLIDUS_gay_SOLIDUS_lesbian = "orientHomosexual",
    pansexual = "orientPansexual",
    queer = "orientQueer",
    questioning = "orientQuestioning",
};

export enum SignatureSchemeEnum {

    BLS12_381 = "signatureSchemeBLS12-381",
    ECDSA_P256 = "signatureSchemeECDSAP256",
    ECDSA_secp256k1 = "signatureSchemeECDSASecp256k1",
    Ed25519 = "signatureSchemeEd25519",
    RSA_SHA256 = "signatureSchemeRSASHA256",
};

export enum SiteTypeEnum {

    branch = "siteTypeBranch",
    headquarters = "siteTypeHeadquarters",
    registered = "siteTypeRegistered",
};

export enum SourceIndependenceEnum {

    independent = "sourceIndependenceIndependent",
    self_or_issuer_originated = "sourceIndependenceSelfOrIssuerOriginated",
};

export enum SourceTierEnum {

    primary = "sourceTierPrimary",
    secondary = "sourceTierSecondary",
    tertiary = "sourceTierTertiary",
};

export enum StandpointEnum {

    universal_standpoint_LEFT_PARENTHESISASTERISKRIGHT_PARENTHESIS = "universalStandpoint",
};

export enum StandpointModalityEnum {

    bullshit_LEFT_PARENTHESISindifference_to_truthRIGHT_PARENTHESIS = "bullshit",
    conceivable_LEFT_PARENTHESISLOZENGE_possibleRIGHT_PARENTHESIS = "conceivable",
    probable = "probable",
    refuted_LEFT_PARENTHESISWHITE_SQUARENOT_SIGN_falseRIGHT_PARENTHESIS = "refuted",
    unequivocal_LEFT_PARENTHESISWHITE_SQUARE_trueRIGHT_PARENTHESIS = "unequivocal",
};

export enum StepTypeEnum {

    atomic_step = "stepTypeAtomic",
    branch_step = "stepTypeBranch",
    end_step = "stepTypeEnd",
    parallel_step = "stepTypeParallel",
    start_step = "stepTypeStart",
    subprocess_step = "stepTypeSubprocess",
};

export enum StorageMediumEnum {

    cloud_service = "storageMediumCloudService",
    content_addressed_store = "storageMediumContentAddressed",
    local_filesystem = "storageMediumLocalFilesystem",
    object_store = "storageMediumObjectStore",
    physical_disk = "storageMediumPhysicalDisk",
    removable_media = "storageMediumRemovableMedia",
};

export enum StrandOrientationEnum {

    both_strands = "strandBoth",
    forward_SOLIDUS_Watson_strand = "strandForward",
    reverse_SOLIDUS_Crick_strand = "strandReverse",
};

export enum SymbolicSystemKindEnum {

    communication_convention = "symbolicKindCommunicationConvention",
    cryptographic = "symbolicKindCryptographic",
    emoji = "symbolicKindEmoji",
    encoding = "symbolicKindEncoding",
    gesture = "symbolicKindGesture",
    mathematical = "symbolicKindMathematical",
    musical = "symbolicKindMusical",
    platform_convention = "symbolicKindPlatformConvention",
    stenographic = "symbolicKindStenographic",
    transcription = "symbolicKindTranscription",
};

export enum TagEnum {

    review = "tagReview",
    todo = "tagTodo",
    urgent = "tagUrgent",
};

export enum TaskStatusEnum {

    cancelled = "taskStatusCancelled",
    completed = "taskStatusCompleted",
    in_progress = "taskStatusInProgress",
    not_started = "taskStatusNotStarted",
};

export enum TemporalFrameEnum {

    TAI_LEFT_PARENTHESISatomic_no_calendarRIGHT_PARENTHESIS = "temporalFrameTAI",
    TDB_Gregorian_LEFT_PARENTHESISbarycentricRIGHT_PARENTHESIS = "temporalFrameTDBGregorian",
    TT_Gregorian_LEFT_PARENTHESISdynamicalRIGHT_PARENTHESIS = "temporalFrameTTGregorian",
    UTC_Gregorian_LEFT_PARENTHESIScivil_timeRIGHT_PARENTHESIS = "temporalFrameUTCGregorian",
};

export enum TemporalMeasurementEnum {

    Cenozoic_start_age = "measurementCenozoicStart",
    Holocene_start_age = "measurementHoloceneStart",
    Phanerozoic_start_age = "measurementPhanerozoicStart",
    Quaternary_start_age = "measurementQuaternaryStart",
};

export enum TemporalPrecisionEnum {

    circa = "precisionCirca",
    day = "precisionDay",
    decade = "precisionDecade",
    month = "precisionMonth",
    year = "precisionYear",
};

export enum TextDirectionEnum {

    boustrophedon = "directionBoustrophedon",
    contextual_SOLIDUS_bidirectional = "directionContextual",
    left_to_right = "directionLtr",
    non_linear = "directionNonLinear",
    right_to_left = "directionRtl",
    vertical_columns_left_to_right = "directionVerticalLtr",
    vertical_columns_right_to_left = "directionVerticalRtl",
};

export enum TimeScaleEnum {

    GPS_Time = "timeScaleGPS",
    International_Atomic_Time_LEFT_PARENTHESISTAIRIGHT_PARENTHESIS = "timeScaleTAI",
    Barycentric_Dynamical_Time_LEFT_PARENTHESISTDBRIGHT_PARENTHESIS = "timeScaleTDB",
    Terrestrial_Time_LEFT_PARENTHESISTTRIGHT_PARENTHESIS = "timeScaleTT",
    Universal_Time_1_LEFT_PARENTHESISUT1RIGHT_PARENTHESIS = "timeScaleUT1",
    Coordinated_Universal_Time_LEFT_PARENTHESISUTCRIGHT_PARENTHESIS = "timeScaleUTC",
};

export enum TrademarkStatusEnum {

    cancelled = "trademarkStatusCancelled",
    expired = "trademarkStatusExpired",
    pending = "trademarkStatusPending",
    registered_LEFT_PARENTHESISREGISTERED_SIGNRIGHT_PARENTHESIS = "trademarkStatusRegistered",
    unregistered_LEFT_PARENTHESISTRADE_MARK_SIGNRIGHT_PARENTHESIS = "trademarkStatusUnregistered",
};

export enum TransactionStatusEnum {

    completed = "transactionStatusCompleted",
    failed = "transactionStatusFailed",
    pending = "transactionStatusPending",
    reversed = "transactionStatusReversed",
};

export enum TransactionTypeEnum {

    deposit = "transactionTypeDeposit",
    fee = "transactionTypeFee",
    interest = "transactionTypeInterest",
    payment = "transactionTypePayment",
    refund = "transactionTypeRefund",
    transfer = "transactionTypeTransfer",
    withdrawal = "transactionTypeWithdrawal",
};

export enum TransliterationSchemeEnum {

    BGNSOLIDUSPCGN_romanization = "schemeBGNPCGN",
    Hepburn_LEFT_PARENTHESISJapanese_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeHepburn",
    IAST_LEFT_PARENTHESISSanskritSOLIDUSIndic_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeIAST",
    IPA_phonetic_transcription = "schemeIPA",
    ISO_15919_LEFT_PARENTHESISIndic_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeISO15919",
    ISO_233_LEFT_PARENTHESISArabic_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeISO233",
    Kunrei_shiki_LEFT_PARENTHESISJapanese_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeKunreiShiki",
    McCune_Reischauer_LEFT_PARENTHESISKorean_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeMcCuneReischauer",
    Nihon_shiki_LEFT_PARENTHESISJapanese_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeNihonShiki",
    Hanyu_Pinyin_LEFT_PARENTHESISMandarin_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemePinyin",
    Revised_Romanization_LEFT_PARENTHESISKorean_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeRevisedRomanization",
    Wade_Giles_LEFT_PARENTHESISMandarin_RIGHTWARDS_ARROW_LatinRIGHT_PARENTHESIS = "schemeWadeGiles",
};

export enum VerificationStatusEnum {

    expired = "verificationStatusExpired",
    failed = "verificationStatusFailed",
    finality_pending = "verificationStatusFinalityPending",
    policy_failed = "verificationStatusPolicyFailed",
    revoked = "verificationStatusRevoked",
    unverified = "verificationStatusUnverified",
    verified = "verificationStatusVerified",
};

export enum VersionRoleEnum {

    canonical = "roleCanonical",
    collected = "roleCollected",
    deprecated = "roleDeprecated",
    draft = "roleDraft",
    long_term_support_LEFT_PARENTHESISLTSRIGHT_PARENTHESIS = "roleLTS",
    latest = "roleLatest",
    published = "rolePublished",
    revised = "roleRevised",
    stable = "roleStable",
    variant = "roleVariant",
    withdrawn = "roleWithdrawn",
    yanked = "roleYanked",
};

export enum VersionScaleEnum {

    major = "scaleMajor",
    minor = "scaleMinor",
    trivial = "scaleTrivial",
};

export enum VirtualLocationTypeEnum {

    chat_space = "virtualLocationTypeChatSpace",
    metaverse_room = "virtualLocationTypeMetaverseRoom",
    online_forum = "virtualLocationTypeOnlineForum",
    social_media_page = "virtualLocationTypeSocialMediaPage",
    streaming_channel = "virtualLocationTypeStreamingChannel",
    video_conference = "virtualLocationTypeVideoConference",
    virtual_event_space = "virtualLocationTypeVirtualEventSpace",
    website = "virtualLocationTypeWebsite",
};

export enum WalletSchemeEnum {

    Bitcoin = "walletSchemeBTC",
    Ethereum = "walletSchemeETH",
    Solana = "walletSchemeSOL",
    Monero = "walletSchemeXMR",
};

export enum WritingSystemTypeEnum {

    abjad = "wsTypeAbjad",
    abugida = "wsTypeAbugida",
    alphabet = "wsTypeAlphabet",
    featural = "wsTypeFeatural",
    ideographic = "wsTypeIdeographic",
    logographic = "wsTypeLogographic",
    mixed = "wsTypeMixed",
    non_linear_LEFT_PARENTHESISeFULL_STOPgFULL_STOP_IthkuilRIGHT_PARENTHESIS = "wsTypeNonLinear",
    pictographic = "wsTypePictographic",
    syllabary = "wsTypeSyllabary",
};



export interface AccessibilityAssertion {
    assertionFacet?: AccessibilityFacet,
    assertionPolarity?: AccessibilityPolarity,
    assertionSubject?: Entity,
}



export interface AccessibilityFacet {
}



export interface AccessibilityPolarity {
}



export interface AcquaintanceRelationship extends InterpersonalRelationship {
}



export interface Activity extends Event {
    wasAssociatedWith?: Agent[],
}



export interface AddressTenure extends TimeScopedRelation {
    addressHolder?: Agent,
    tenuredContactPoint?: ContactPoint,
}



export interface AdoptiveParentChild extends ParentChildRelationship {
}



export interface Agent {
    email?: string,
    endorses?: Agent[],
    hasAgreement?: Agreement[],
    hasContactPoint?: ContactPoint[],
    hasMet?: Agent[],
    hasSkill?: Skill[],
    hasUsed?: Entity[],
    hasWorkedWith?: Agent[],
    holdsAccount?: OnlineAccount[],
    holdsCredential?: Credential[],
    holdsKey?: CryptographicKey[],
    knowsLanguage?: Language[],
    mailmapEntry?: string,
    memberOf?: Organization[],
    nativeLanguage?: Language[],
    telephone?: string,
}



export interface AggregationFunction {
}



export interface Agreement {
    hasAgreementName?: AgreementName[],
    hasParty?: Agent[],
}



export interface AgreementName extends Appellation {
}



export interface Annotation {
    annotationBody?: Note,
    annotationMotivation?: AnnotationMotivation,
    annotationTarget?: Entity,
    annotationTargetSpan?: EvidenceSpan[],
}



export interface AnnotationMotivation {
}



export interface Appellation {
    conferredByEvent?: LifeEvent[],
    fullName?: string,
    hasNamePart?: NamePart[],
    nameLanguage?: Language,
    namePurpose?: NamePurpose[],
    nameScript?: string,
    romanization?: string,
    transliterationScheme?: TransliterationScheme[],
}



export interface ArcType {
}



export interface ArchaeologicalFindContext {
    findContextDating?: TemporalMeasurement[],
    findContextEvent?: Event[],
    findContextPlace?: Place[],
    findContextStratigraphy?: Entity[],
    findContextTarget?: PhysicalObject,
}



export interface Article extends Work {
}



export interface Asset extends Entity {
    assetIdentifier?: string,
    assetType?: AssetType,
}



export interface AssetType {
}



export interface AtomicConstraint extends Constraint {
    constraintOperator?: ConstraintOperator,
    leftOperand?: LeftOperand,
    rightOperand?: string,
    rightOperandReference?: Entity[],
}



export interface Attachment extends BodyPart {
    filename?: string,
}



export interface Attestation {
    attestationArtifact?: AttestationArtifact[],
    attestationPolicy?: AttestationPolicy[],
    attestationType?: AttestationType[],
    attestedClaim?: Observation[],
    attestedSubject?: Entity[],
    attester?: Agent,
    hasSLSALevel?: SLSALevel[],
    issuedAt?: string,
    transparencyLogEntry?: TransparencyLogEntry[],
    verificationActivity?: VerificationActivity[],
    verificationResult?: VerificationResult[],
}



export interface AttestationArtifact extends InformationObject {
    artifactMediaType?: string,
}



export interface AttestationPolicy {
}



export interface AttestationType {
}



export interface AuthenticationResult extends InformationObject {
    authMethod?: string,
    authResult?: string,
    authServer?: string,
}



export interface AuthorIdentity extends InformationObject {
    authorIdentityString?: string,
    canonicalizedIdentity?: Agent[],
}



export interface Availability {
    availabilityAgent?: Agent[],
    availabilitySlot?: TimeInterval,
    availabilityStatus?: AvailabilityStatus[],
}



export interface AvailabilityStatus {
}



export interface Axis {
}



export interface BiologicalParentChild extends ParentChildRelationship {
}



export interface BiologicalSequenceLocation extends Location {
    hasSequenceFeature?: SequenceFeature[],
}



export interface Blob extends SourceNode {
}



export interface Block extends InformationObject {
    blockHash?: string,
    blockNumber?: number,
}



export interface BlockchainAccount extends Entity {
}



export interface BlockchainNetwork extends Entity {
    chainId?: string,
}



export interface BodyPart extends InformationObject {
    charset?: string,
    contentId?: string,
    hasContentDisposition?: ContentDisposition,
    hasContentTransferEncoding?: ContentTransferEncoding,
    mediaType?: string,
    partId?: string,
}



export interface BookRelease extends Manifestation {
}



export interface Bookmark extends Annotation {
}



export interface Branch extends InformationObject {
}



export interface BranchConditionType {
}



export interface BuildActivity extends Activity {
    buildConfigUri?: string,
    buildOutput?: Distribution[],
    buildSource?: string[],
}



export interface Builder extends SoftwareAgent {
}



export interface CadastralReference extends InformationObject {
    referenceAuthority?: Agent,
    referenceJurisdiction?: Place,
    referenceType?: CadastralReferenceType[],
    referenceValue?: string,
}



export interface CadastralReferenceType {
}



export interface Calendar {
    calendarMember?: Event[],
    calendarTimeZone?: TimeZone,
}



export interface CalendarMethod {
}



export interface CalendarSystem {
}



export interface Capacity extends Measurement {
    capacityOf?: Location,
}



export interface CarrierMedium {
}



export interface CelestialCoordinates {
    celestialEpoch?: string,
    declination?: string,
    rightAscension?: string,
}



export interface CelestialLocation extends Location {
    celestialObjectType?: CelestialObjectType[],
    hasCelestialCoordinates?: CelestialCoordinates[],
}



export interface CelestialObjectType {
}



export interface CelestialReferenceOrigin {
}



export interface Certification {
    certificationLevel?: string,
    certifiedIdentity?: Agent,
    certifiedKey?: CryptographicKey,
    certifier?: Agent,
}



export interface CharacterArc extends InformationObject {
    arcEvidence?: InformationObject[],
    arcFrame?: NarrativeReferenceFrame,
    arcSubject?: Entity,
    arcType?: ArcType,
}



export interface CitationAct {
    citationIntent?: CitationIntent,
    citedEntity?: CreativeWork,
    citingEntity?: Entity,
    coverageDepth?: CoverageDepth[],
    hasEvidenceClass?: EvidenceClass[],
    sourceIndependence?: SourceIndependence[],
    sourceTier?: SourceTier[],
    supportsNotability?: boolean,
    viaSelector?: Selector[],
}



export interface CitationIntent {
}



export interface ClaimVeridicality {
}



export interface CodeReview extends Event {
    reviewCommit?: Commit[],
    reviewOf?: MergeRequest[],
}



export interface Collection extends Work {
}



export interface Comment extends Note {
    commentParent?: Entity[],
}



export interface Commit extends Activity {
    authorTime?: string,
    authoredBy?: Agent[],
    commitAncestor?: Commit[],
    commitAuthorIdentity?: AuthorIdentity,
    commitCommitterIdentity?: AuthorIdentity,
    commitDescendant?: Commit[],
    commitInRepository?: Repository,
    commitTree?: SourceTree,
    committedBy?: Agent[],
    committerTime?: string,
    parentCommit?: Commit[],
}



export interface ConflictStrategy {
}



export interface Connection {
    connectionSource?: string,
    connectionTarget?: string,
}



export interface Constraint {
}



export interface ConstraintLogic {
}



export interface ConstraintOperator {
}



export interface ContactPoint {
}



export interface ContainmentTenure extends TimeScopedRelation {
    containmentChild?: Place,
    containmentParent?: Place,
}



export interface ContentDisposition {
}



export interface ContentSegment extends InformationObject {
    segmentIndex?: number,
    segmentOf?: Entity[],
    segmentType?: ContentSegmentType,
}



export interface ContentSegmentType {
}



export interface ContentTransferEncoding {
}



export interface Contract extends Agreement {
}



export interface Contribution {
    contributionDegree?: ContributionDegree,
    contributionRole?: ContributionRole,
    contributionTarget?: CreativeWork,
    contributor?: Agent,
}



export interface ContributionDegree {
}



export interface ContributionRole {
}



export interface ControlFlow {
    flowGuard?: BranchConditionType[],
    flowOrder?: number,
    flowSource?: ProcedureStep[],
    flowTarget?: ProcedureStep[],
}



export interface CoordinateMatrix {
    coordinateMatrixFrame?: ReferenceFrame,
    matrixShape?: string,
    matrixValue?: string,
}



export interface CoordinateObservation extends SpatialMeasurement {
    coordinateObservationOf?: Place[],
    coordinateResult?: GeoCoordinates[],
    geometryResult?: Geometry[],
}



export interface Copyright {
    copyrightHolder?: Agent[],
    copyrightNotice?: string,
    copyrightStatus?: CopyrightStatus,
    copyrightWork?: InformationObject,
    copyrightYear?: string,
}



export interface CopyrightStatus {
}



export interface CoupleRelationship extends KinRelationship {
    hasPartner?: Person[],
}



export interface CoverageDepth {
}



export interface CreativeWork extends InformationObject {
    abstract?: string,
    audience?: Agent[],
    bibliographicCitation?: string,
    conformsTo?: Entity[],
    dateAccepted?: string,
    dateAvailable?: string,
    dateCreated?: string,
    dateModified?: string,
    datePublished?: string,
    dateSubmitted?: string,
    editionOf?: CreativeWork,
    extent?: string,
    hasAuthor?: Agent[],
    hasContributor?: Agent[],
    hasEditor?: Agent[],
    hasIllustrator?: Agent[],
    hasNarrator?: Agent[],
    hasTitle?: CreativeWorkTitle[],
    hasTranslator?: Agent[],
    identifier?: string,
    isRequiredBy?: CreativeWork[],
    medium?: CarrierMedium[],
    propagatesFrom?: CreativeWork[],
    requires?: CreativeWork[],
    sourceFor?: NarrativeReferenceFrame[],
    sourceLocation?: string,
    sourceModifiedAt?: string,
    spatialCoverage?: Place[],
    tableOfContents?: string,
    temporalCoverage?: TimeInterval[],
    title?: string,
}



export interface CreativeWorkTitle extends Appellation {
}



export interface CreativeWorkType {
}



export interface Credential extends Entity {
    credentialFor?: string[],
    credentialIssuer?: Organization,
}



export interface CryptoWallet extends FinancialAccount {
    walletAddress?: string,
    walletKey?: CryptographicKey[],
    walletScheme?: WalletScheme,
}



export interface CryptographicKey extends InformationObject {
    fingerprint?: string,
    keyAlgorithm?: string,
    keyExpiresAt?: string,
    keyId?: string,
    keyMaterial?: string,
    keyScheme?: KeyScheme,
}



export interface CryptographicSignature extends InformationObject {
    signatureAlgorithm?: string,
    signatureOf?: string[],
    signedBy?: Agent[],
    signingDomain?: string,
    signingKey?: CryptographicKey,
    verificationStatus?: string,
}



export interface DKIMSignature extends CryptographicSignature {
}



export interface DataFlow {
    dataFlowEntity?: Entity[],
    dataFlowSource?: ProcedureStep[],
    dataFlowTarget?: ProcedureStep[],
}



export interface Dataset extends Work {
}



export interface DatingMethod {
}



export interface DepictionContext {
}



export interface DepictionUsage {
    depictionAudience?: Entity[],
    depictionAuthority?: Agent[],
    depictionContext?: DepictionContext,
    depictionImage?: MediaObject,
    depictionInterval?: TimeInterval[],
    depictionSubject?: Entity,
}



export interface DerivationKind {
}



export interface Determinacy {
}



export interface Diff extends InformationObject {
    diffFrom?: Commit,
    diffTo?: Commit,
}



export interface DisclosurePolicy {
}



export interface Distribution extends InformationObject {
    distributionFormat?: string,
}



export interface Document extends Work {
}



export interface Duration extends Entity {
    durationValue?: string,
}



export interface Duty extends Rule {
}



export interface EmailAddress extends ContactPoint {
    addressValue?: string,
    deliversToAccount?: OnlineAccount[],
    domainPart?: string,
    localPart?: string,
}



export interface EmailMessage extends Message {
    analysisInputBodyLine?: string,
    analysisScope?: string,
    autoSubmitted?: string,
    bcc?: EmailAddress[],
    bodyLineFingerprint?: string,
    calendarAttachment?: Attachment[],
    calendarUid?: string,
    canonicalFingerprint?: string,
    cc?: EmailAddress[],
    describesEvent?: Event[],
    dispositionNotificationTo?: EmailAddress[],
    from?: EmailAddress[],
    hasCalendarMethod?: CalendarMethod[],
    hasMessageParticipant?: MessageParticipant[],
    hasPatchDiff?: EmailPatchDiff[],
    importance?: string,
    messageIdCollision?: boolean,
    messageIdGenerated?: boolean,
    precedence?: string,
    priority?: string,
    readReceiptRequested?: boolean,
    replyTo?: EmailAddress[],
    sender?: EmailAddress[],
    sentBySoftware?: SoftwareAgent[],
    subjectPrefix?: string,
    to?: EmailAddress[],
    userAgent?: string,
}



export interface EmailPatchDiff extends BodyPart {
}



export interface Employment extends Membership {
    employmentCompensation?: MonetaryAmount[],
    employmentInterval?: TimeInterval[],
    employmentOccupation?: Occupation,
    employmentRole?: Role,
    employmentSeniority?: SeniorityLevel,
    employmentType?: EmploymentType,
}



export interface EmploymentType {
}



export interface Entity {
    acquireLicensePage?: string,
    attributionText?: string,
    attributionUrl?: string,
    authorityLink?: string[],
    cites?: CreativeWork[],
    conditionsOfAccess?: string,
    counterpartOf?: Entity[],
    depictedIn?: MediaObject[],
    description?: string,
    existenceInterval?: TimeInterval[],
    hasAccessibilityNeed?: AccessibilityFacet[],
    hasAppellation?: Appellation[],
    hasAttestation?: Attestation[],
    hasCopyright?: Copyright[],
    hasCreationEvent?: Event[],
    hasDestructionEvent?: Event[],
    hasDirectReply?: Comment[],
    hasLicense?: License[],
    hasPose?: Pose[],
    hasReply?: Comment[],
    hasRightsStatement?: RightsStatement[],
    hasSensoryObservation?: SensoryObservation[],
    hasSensoryQuantity?: SensoryQuantity[],
    hasSpatialMeasurement?: SpatialMeasurement[],
    hasStream?: Stream[],
    hasTag?: Tag[],
    hasTrademark?: Trademark[],
    hasVersion?: Entity[],
    hasWebPage?: WebPage[],
    isAbout?: Entity[],
    isAccessibleForFree?: boolean,
    isReferencedBy?: Entity[],
    isResultOf?: Observation[],
    locatedAt?: Location[],
    mentionedIn?: Note[],
    name?: string,
    provenance?: string,
    proximity?: ProximityMeasurement[],
    references?: Entity[],
    storedIn?: StorageLocation[],
    supersededBy?: Entity[],
    supersedes?: Entity[],
    usageInfo?: string,
    versionFingerprint?: string,
    versionLabel?: string,
    versionOf?: Entity,
    wasAttributedTo?: Agent[],
    wasGeneratedBy?: Activity[],
}



export interface EntityExistence extends TimeScopedRelation {
    existenceCreationEvent?: Event[],
    existenceDestructionEvent?: Event[],
    existenceEntity?: Entity,
}



export interface EtymologicalDerivation {
    derivationEvidence?: Entity[],
    derivationKind?: DerivationKind[],
    derivationSource?: InformationObject,
    derivationTarget?: InformationObject,
}



export interface Event {
    after?: Event[],
    before?: Event[],
    coincidesWith?: Event[],
    contains?: Event[],
    deceptionCue?: Observation[],
    deceptiveIntentClaim?: StandpointClaim[],
    during?: Event[],
    earliestStart?: string,
    eventAspect?: GrammaticalAspect[],
    eventDescribedBy?: EmailMessage[],
    eventInterval?: TimeInterval[],
    eventLocation?: Location[],
    eventObservation?: Observation[],
    eventSpacetime?: LocationState[],
    eventTemporalFrame?: TemporalFrame,
    eventTense?: GrammaticalTense[],
    eventTime?: string,
    eventTimeZone?: TimeZone,
    eventTrajectory?: Trajectory[],
    eventType?: EventType[],
    finishedBy?: Event[],
    finishes?: Event[],
    hasDuration?: Duration[],
    hasParticipant?: Agent[],
    hasSubEvent?: Event[],
    heldStandpoint?: StandpointClaim[],
    implicates?: Entity[],
    latestEnd?: string,
    maximViolationType?: MaximViolationType[],
    meets?: Event[],
    metBy?: Event[],
    occurrenceOfSeries?: EventSeries[],
    overlappedBy?: Event[],
    overlaps?: Event[],
    predecessorOrganization?: Organization[],
    projectedStandpoint?: StandpointClaim[],
    propagationMutationDistance?: number,
    startedBy?: Event[],
    starts?: Event[],
    subEventOf?: Event[],
    successorOrganization?: Organization[],
    temporalPrecision?: TemporalPrecision[],
}



export interface EventInvitation extends Agreement {
    invitationEvent?: Event,
    invitationInvitee?: Agent[],
    invitationStatus?: InvitationStatus[],
    rsvpStatus?: RsvpStatus[],
}



export interface EventSchedule {
    scheduleOccurrence?: Event[],
    scheduleRecurrenceRule?: RecurrenceRule[],
    scheduleTemplateEvent?: Event,
    scheduleTimeZone?: TimeZone,
}



export interface EventSeries extends Entity {
    hasRecurrenceRule?: RecurrenceRule[],
    seriesOccurrence?: Event[],
}



export interface EventType {
}



export interface EvidenceClass {
}



export interface EvidenceSpan extends InformationObject {
}



export interface ExceptionType {
}



export interface Execution extends Event {
    executesProcedure?: Procedure[],
    executesStep?: ProcedureStep[],
    executionParticipant?: Agent[],
    executionStatus?: ExecutionStatus[],
}



export interface ExecutionStatus {
}



export interface Expression extends CreativeWork {
    embodiedIn?: Manifestation[],
    realizes?: Work[],
}



export interface Family extends Group {
}



export interface Filename extends Appellation {
    claimedMediaType?: string,
}



export interface FinancialAccount extends InformationObject {
    accountBalance?: MonetaryAmount[],
    accountCurrency?: ReferenceFrame[],
    accountHolder?: Agent[],
    accountNumber?: string,
    accountType?: FinancialAccountType,
    bic?: string,
    iban?: string,
}



export interface FinancialAccountType {
}



export interface FinancialTransaction extends Event {
    transactionAmount?: MonetaryAmount,
    transactionStatus?: TransactionStatus[],
    transactionType?: TransactionType[],
}



export interface ForgePlatform extends Entity {
}



export interface FormalLanguage extends Language {
}



export interface FosterParentChild extends ParentChildRelationship {
}



export interface FrameKind {
}



export interface FrameRealm {
}



export interface Gender {
}



export interface GenderExpression extends IdentityFacet {
    expressionValue?: GenderExpressionStyle,
}



export interface GenderExpressionStyle {
}



export interface GenderIdentity extends IdentityFacet {
    genderValue?: Gender,
}



export interface GeoCoordinates extends Entity {
    elevation?: string,
    latitude?: string,
    longitude?: string,
}



export interface Geocode extends Entity {
    geocodeFrame?: ReferenceFrame,
    geocodeValue?: string,
    geohash?: string,
    mgrs?: string,
    mileMarker?: string,
    plusCode?: string,
    unLocode?: string,
    what3words?: string,
}



export interface Geometry extends Entity {
    asGeoJSON?: string,
    asWKT?: string,
    geometryDeterminacy?: Determinacy[],
    geometryType?: GeometryType[],
}



export interface GeometryType {
}



export interface GovernanceModel {
}



export interface GrammaticalAspect {
}



export interface GrammaticalTense {
}



export interface GranularityLevel {
    coarserThan?: GranularityLevel[],
}



export interface Group extends Entity {
}



export interface Highlight extends Annotation {
}



export interface Holding {
    holdingAgent?: Agent,
    holdingAsset?: Asset,
    holdingCostBasis?: MonetaryAmount,
    holdingPeriod?: TimeInterval[],
    holdingQuantity?: string,
}



export interface Honorific {
    honorificClass?: HonorificClass[],
    honorificPosition?: HonorificPosition,
}



export interface HonorificClass {
}



export interface HonorificPosition {
}



export interface Identifier {
    identifierScheme?: string,
    identifierValue?: string,
    jurisdiction?: Location[],
}



export interface IdentityFacet {
    selfAsserted?: boolean,
}



export interface ImageRegion extends InformationObject {
    regionLabel?: string,
    regionOf?: MediaObject,
    regionSelector?: RegionSelector,
}



export interface ImportActivity extends Activity {
    ingestedAt?: string,
}



export interface InformationObject extends Entity {
    contributesToFrame?: NarrativeReferenceFrame[],
    detectedMediaType?: string,
}



export interface InlinePart extends BodyPart {
}



export interface Inscription extends InformationObject {
    inscriptionCarrier?: PhysicalObject,
}



export interface InscriptionReading {
    readingOf?: Inscription,
    readingResult?: LexicalForm[],
}



export interface InscriptionTranslation {
    translationOf?: Inscription,
    translationResult?: LexicalForm[],
}



export interface InscriptionTransliteration {
    transliterationOf?: Inscription,
    transliterationResult?: LexicalForm[],
}



export interface Instant {
    inTemporalFrame?: TemporalFrame[],
    instantValue?: string,
}



export interface InterpersonalRelationship {
    relationshipInterval?: TimeInterval[],
    relationshipParty?: Agent[],
}



export interface InvitationStatus {
}



export interface Invoice extends Document {
    invoiceAmount?: MonetaryAmount,
    invoiceDueDate?: string,
    invoiceIssuer?: Agent[],
    invoiceRecipient?: Agent[],
    invoiceStatus?: InvoiceStatus[],
}



export interface InvoiceStatus {
}



export interface Issue extends InformationObject {
}



export interface Item extends CreativeWork {
    exemplifies?: Manifestation[],
    hasCarrier?: PhysicalObject[],
}



export interface JournalEntry extends Event {
    journalEntryPostings?: Posting[],
}



export interface JurisdictionTenure extends TimeScopedRelation {
    jurisdictionDeterminacy?: Determinacy[],
    jurisdictionPlace?: Place,
    jurisdictionPolity?: Agent,
}



export interface KeyScheme {
}



export interface KinRelationship {
}



export interface LandTenure extends TimeScopedRelation {
    tenureDeterminacy?: Determinacy[],
    tenureParty?: Agent,
    tenurePlace?: Place,
    tenureRights?: RightsStatement[],
    tenureType?: LandTenureType[],
}



export interface LandTenureType {
}



export interface Language extends InformationObject {
    bcp47Tag?: string,
    designGoal?: string,
    hasNotationSystem?: NotationSystem[],
    languageCode?: string,
    languageModality?: LanguageModality[],
    languageOrigin?: LanguageOrigin[],
    languageStatus?: LanguageStatus[],
    languageTag?: string,
    usesWritingSystem?: WritingSystem[],
}



export interface LanguageChangeEvent extends Activity {
    affectedLanguage?: Language[],
    changeType?: LanguageChangeType[],
}



export interface LanguageChangeType {
}



export interface LanguageCreation extends Activity {
}



export interface LanguageModality {
}



export interface LanguageOrigin {
}



export interface LanguageProficiency {
    proficiencyAgent?: Agent,
    proficiencyInterval?: TimeInterval[],
    proficiencyLanguage?: Language,
    proficiencyLevel?: ProficiencyLevel,
    proficiencyModality?: ProficiencyModality,
    proficiencyScale?: ProficiencyScale,
}



export interface LanguageState {
    stateAuthority?: Agent[],
    stateInterval?: TimeInterval[],
    stateLanguage?: Language,
    stateStatusValue?: LanguageStatus[],
}



export interface LanguageStatus {
}



export interface LanguageVariety extends Language {
    varietyKind?: LanguageVarietyKind[],
    varietyOf?: Language[],
}



export interface LanguageVarietyKind {
}



export interface LanguageVersion extends Language {
}



export interface LedgerAccount extends InformationObject {
    ledgerAccountCurrency?: ReferenceFrame[],
    ledgerAccountHolder?: Agent[],
    ledgerAccountType?: LedgerAccountType,
}



export interface LedgerAccountType {
}



export interface LedgerEvent extends Event {
    logIndex?: number,
}



export interface LedgerFinalityStatus {
}



export interface LedgerTransaction extends InformationObject {
    transactionHash?: string,
}



export interface LeftOperand {
}



export interface LexicalForm extends InformationObject {
    formOf?: LexicalItem[],
    formRepresentation?: string,
    formTransliterationScheme?: TransliterationScheme[],
    formType?: LexicalFormType[],
}



export interface LexicalFormType {
}



export interface LexicalItem extends InformationObject {
    hasLexicalForm?: LexicalForm[],
    lexicalItemLanguage?: Language,
}



export interface License extends Agreement {
    isOsiApproved?: boolean,
    licenseFamily?: LicenseFamily,
    licenseText?: string,
    licensedWork?: InformationObject,
    licensee?: Agent[],
    licensor?: Agent[],
    spdxLicenseId?: string,
    spdxLicenseName?: string,
}



export interface LicenseFamily {
}



export interface LifeEvent extends Event {
}



export interface LiteraryWork extends Work {
}



export interface Location extends Entity {
    adjacentTo?: Location[],
    containedInLocation?: Location[],
    hasAccessibilityFeature?: AccessibilityFacet[],
    hasBarrier?: AccessibilityFacet[],
    hasCapacity?: Capacity[],
    hasOccupancy?: Occupancy[],
    hasUtilization?: Utilization[],
    rcc8dc?: Location[],
    rcc8ec?: Location[],
    rcc8eq?: Location[],
    rcc8ntpp?: Location[],
    rcc8ntppi?: Location[],
    rcc8po?: Location[],
    rcc8tpp?: Location[],
    rcc8tppi?: Location[],
    siteType?: SiteType[],
    spatiallyConnectsTo?: Location[],
    timezone?: string,
}



export interface LocationState extends Entity {
    stateAtInstant?: Instant[],
    stateDuringInterval?: TimeInterval[],
    stateHasAngularVelocity?: ScalarQuantity[],
    stateHasVelocity?: ScalarQuantity[],
    stateOf?: Entity,
    stateReferenceFrame?: ReferenceFrame,
}



export interface LogicalConstraint extends Constraint {
    constraintLogic?: ConstraintLogic,
    logicConstraintMember?: Constraint[],
}



export interface Mailbox extends InformationObject {
    childMailbox?: Mailbox[],
    mailboxName?: string,
    mailboxOfAccount?: OnlineAccount,
    mailboxPath?: string,
    mailboxRole?: string,
    mailboxSortOrder?: number,
    mailboxTotalMessages?: number,
    mailboxUnreadMessages?: number,
    parentMailbox?: Mailbox[],
}



export interface MailboxResidence extends TimeScopedRelation {
    residenceMailbox?: Mailbox,
    residentMessage?: Message,
}



export interface MaintenanceStatus {
}



export interface Manifestation extends CreativeWork {
    embodies?: Expression[],
    exemplifiedBy?: Item[],
    hasManifestationFormat?: ManifestationFormat,
}



export interface ManifestationFormat {
}



export interface Mark extends InformationObject {
    markText?: string,
}



export interface MaximViolationType {
}



export interface MeasuredValue extends Entity {
}



export interface Measurement extends Observation {
}



export interface MediaObject extends Manifestation {
    captureDevice?: PhysicalObject[],
    captureTime?: string,
    colourspace?: ReferenceFrame[],
    depicts?: Entity[],
    hasRegion?: ImageRegion[],
    imageOrientation?: string,
    pixelHeight?: string,
    pixelWidth?: string,
}



export interface Membership {
    fillsPost?: Post[],
    hasRole?: Role[],
    membershipMember?: Agent,
    membershipOrganization?: Organization,
}



export interface MentalReferenceFrame extends ReferenceFrame {
}



export interface Merge extends Activity {
    mergeBase?: Commit[],
    mergeSource?: Ref[],
    mergeTarget?: Ref,
}



export interface MergeRequest extends InformationObject {
}



export interface Message extends InformationObject {
    hasAttachment?: Attachment[],
    hasAuthenticationResult?: AuthenticationResult[],
    hasBodyPart?: BodyPart[],
    hasHeader?: MessageHeader[],
    hasInlinePart?: InlinePart[],
    hasKeyword?: MessageKeyword[],
    hasMessageKind?: MessageKind[],
    hasRelayHop?: RelayHop[],
    inReplyTo?: Message[],
    listId?: string,
    partOfThread?: Thread[],
    preview?: string,
    receivedAt?: string,
    residesIn?: Mailbox[],
    rfcMessageId?: string,
    sentAt?: string,
    sizeEstimate?: number,
    subject?: string,
}



export interface MessageHeader extends InformationObject {
    headerName?: string,
    headerValue?: string,
}



export interface MessageKeyword {
}



export interface MessageKind {
}



export interface MessageParticipant {
    displayName?: string,
    participantAddress?: EmailAddress,
    participantGroup?: string,
    participantHeader?: MessageHeader,
    participantMessage?: EmailMessage,
    participantOrdinal?: string,
    participantRole?: MessageParticipantRole,
    rawAddressValue?: string,
}



export interface MessageParticipantRole {
}



export interface MetricKind {
}



export interface MonetaryAmount extends Entity {
    currency?: ReferenceFrame,
    monetaryValue?: string,
}



export interface MultipartBodyPart extends BodyPart {
    hasMultipartType?: MultipartType,
}



export interface MultipartType {
}



export interface Myth extends SocialObject {
    affectedConsumerSurface?: ProjectionContext[],
    hasMythTelling?: CreativeWork[],
    mythFrame?: NarrativeReferenceFrame,
    recurringRisk?: boolean,
}



export interface NamePart extends InformationObject {
    namePartType?: NamePartType,
    partExpansion?: string,
    partOrder?: string,
    partText?: string,
}



export interface NamePartType {
}



export interface NamePurpose {
}



export interface NameRegister {
}



export interface NameUsage {
    usageAppellation?: Appellation,
    usageAudience?: Entity[],
    usageAuthority?: Agent[],
    usageInterval?: TimeInterval[],
    usageNamed?: Entity,
    usageNamer?: Agent[],
    usageRegister?: NameRegister,
    usageRelationshipScope?: InterpersonalRelationship,
}



export interface NamedPeriod extends Entity {
    periodContainsPeriod?: NamedPeriod[],
    periodEnd?: Instant,
    periodPartOf?: NamedPeriod[],
    periodStart?: Instant[],
    periodType?: PeriodType[],
}



export interface NarrativeFrameLink {
    narrativeFrameLinkRelation?: NarrativeFrameRelation,
    narrativeFrameLinkSource?: NarrativeReferenceFrame,
    narrativeFrameLinkTarget?: NarrativeReferenceFrame,
}



export interface NarrativeFrameRelation {
}



export interface NarrativeReferenceFrame extends ReferenceFrame {
    hasNarrativeFrameRelation?: NarrativeFrameRelation[],
    relatesToFrame?: NarrativeReferenceFrame[],
}



export interface NetworkAddress extends Entity {
    networkAddressFrame?: ReferenceFrame,
    networkAddressType?: NetworkAddressType[],
    networkAddressValue?: string,
}



export interface NetworkAddressType {
}



export interface NotationSystem extends SymbolicSystem {
    notationSystemFor?: Language[],
    notationSystemKind?: SymbolicSystemKind[],
}



export interface NotationSystemUsage {
    notationUsageInterval?: TimeInterval,
    notationUsageNotationSystem?: NotationSystem,
    notationUsageRole?: NotationUsageRole,
    notationUsageTarget?: Entity,
}



export interface NotationUsageRole {
}



export interface Note extends InformationObject {
    hasWikilink?: Note[],
    mentions?: Entity[],
    noteAuthor?: Agent[],
    noteContent?: string,
    noteCreatedAt?: string,
    noteModifiedAt?: string,
    relatedNote?: Note[],
}



export interface ObservableProperty {
}



export interface Observation {
    credibilityScore?: string,
    facetSubject?: Person[],
    facetVantage?: Agent[],
    observationEvent?: Event[],
    observationMethod?: ObservationMethod,
    observationResult?: Entity[],
    observationType?: ObservationType[],
    observedFeature?: string[],
    perceptionEnvironment?: SensoryEnvironment,
    vantage?: Entity[],
}



export interface ObservationMethod {
}



export interface ObservationType {
}



export interface ObservationalActivity extends Activity {
    generatedObservation?: Observation[],
}



export interface Occupancy extends Measurement {
    occupancyOf?: Location,
}



export interface Occupation extends Entity {
    occupationClassification?: string,
}



export interface OnlineAccount extends InformationObject {
    accountKey?: CryptographicKey[],
    accountName?: string,
    activityPubActor?: string,
    nip05?: string,
    nostrPubkey?: string,
}



export interface Order extends Agreement {
    orderAmount?: MonetaryAmount,
    orderBuyer?: Agent[],
    orderSeller?: Agent[],
    orderStatus?: OrderStatus[],
}



export interface OrderStatus {
}



export interface Organization extends Agent {
    hasIdentifier?: Identifier[],
    hasMember?: Agent[],
    hasOrganizationName?: OrganizationName[],
    hasSite?: Location[],
    industryClassification?: Identifier[],
    legalIdentifier?: Identifier[],
    organizationPurpose?: string,
    organizationType?: OrganizationType[],
    subOrganizationOf?: Organization[],
}



export interface OrganizationName extends Appellation {
}



export interface OrganizationType {
}



export interface Orientation extends Entity {
    bearing?: number,
    eulerOrder?: string,
    heading?: number,
    pitch?: number,
    quaternionW?: number,
    quaternionX?: number,
    quaternionY?: number,
    quaternionZ?: number,
    roll?: number,
    yaw?: number,
}



export interface PGPSignature extends CryptographicSignature {
}



export interface Package extends InformationObject {
    hasDistribution?: Distribution[],
    packageOf?: SoftwareProduct[],
}



export interface ParentChildRelationship extends KinRelationship {
    relationshipChild?: Person,
    relationshipParent?: Person,
}



export interface ParticipantRole {
}



export interface Participation {
    participationEvent?: Event,
    participationInterval?: TimeInterval[],
    participationParticipant?: Entity[],
    participationRole?: ParticipantRole[],
}



export interface Patent extends Work {
}



export interface Payment extends FinancialTransaction {
    paymentMethod?: PaymentMethod[],
}



export interface PaymentMethod {
}



export interface PeriodType {
}



export interface Permission extends Rule {
}



export interface Person extends Agent {
    hasAncestor?: Person[],
    hasChild?: Person[],
    hasDescendant?: Person[],
    hasFather?: Person[],
    hasGenderExpression?: GenderExpression[],
    hasGenderIdentity?: GenderIdentity[],
    hasMother?: Person[],
    hasName?: PersonName[],
    hasOccupation?: Occupation[],
    hasParent?: Person[],
    hasPronounSet?: PronounSet[],
    hasRomanticOrientation?: RomanticOrientation[],
    hasSexualOrientation?: SexualOrientation[],
    hasSibling?: Person[],
    hasSpouse?: Person[],
    honorific?: Honorific[],
    intersexVariation?: string,
    sexAssignedAtBirth?: SexAssignedAtBirth[],
}



export interface PersonName extends Appellation {
}



export interface PhysicalCarrierType {
}



export interface PhysicalObject extends Entity {
    carrierInscription?: Inscription[],
    carrierType?: PhysicalCarrierType[],
}



export interface Place extends Location {
    containedInPlace?: Place[],
    containsPlace?: Place[],
    hasCadastralReference?: CadastralReference[],
    hasCentroid?: Geometry[],
    hasCoordinateObservation?: CoordinateObservation[],
    hasCoordinates?: GeoCoordinates[],
    hasGeocode?: Geocode[],
    hasGeometry?: Geometry[],
    hasPlaceName?: PlaceName[],
    placeDeterminacy?: Determinacy[],
    placeSupersededBy?: Place[],
    placeSupersedes?: Place[],
    placeType?: PlaceType[],
}



export interface PlaceName extends Appellation {
}



export interface PlaceNaming extends NameUsage {
}



export interface PlaceType {
}



export interface Pose extends Entity {
    hasPoseOrientation?: Orientation[],
    hasPosePosition?: SpatialCoordinates[],
    poseFrame?: ReferenceFrame[],
}



export interface Post {
    postIn?: Organization,
}



export interface PostalAddress extends ContactPoint {
    addressLocality?: string,
    addressPlace?: Place[],
    addressRegion?: string,
    countryCode?: string,
    extendedAddress?: string,
    postOfficeBox?: string,
    postalAddressFrame?: ReferenceFrame,
    postalCode?: string,
    streetAddress?: string,
}



export interface Posting {
    postingAccount?: LedgerAccount,
    postingAmount?: MonetaryAmount,
    postingDirection?: PostingDirection,
    postingJournalEntry?: JournalEntry,
}



export interface PostingDirection {
}



export interface PrivacyNotice extends InformationObject {
}



export interface Procedure extends InformationObject {
    hasProcedureStep?: ProcedureStep[],
    hasSubProcedure?: Procedure[],
    inquiryPriority?: number,
    inquirySource?: Entity[],
    inquiryStatus?: ExecutionStatus[],
    inquiryTheme?: string,
    procedureType?: ProcedureType[],
    resolvedByArtifact?: Entity[],
    subProcedureOf?: Procedure[],
}



export interface ProcedureStep extends InformationObject {
    procedureStepOf?: Procedure[],
    procedureStepType?: StepType[],
    stepEnactsProcedure?: Procedure[],
    stepInput?: string[],
    stepOutput?: string[],
    stepParameter?: string[],
}



export interface ProcedureType {
}



export interface ProfessionalRelationship extends InterpersonalRelationship {
}



export interface ProficiencyLevel {
    levelScale?: ProficiencyScale,
}



export interface ProficiencyModality {
}



export interface ProficiencyScale {
}



export interface Profile extends InformationObject {
    profileAppliesTo?: string[],
    profileDescriptor?: string,
    profileOpenValue?: string[],
}



export interface ProgrammingLanguage extends FormalLanguage {
}



export interface Prohibition extends Rule {
}



export interface Project extends Entity {
    governanceModel?: GovernanceModel[],
    hasRelease?: Release[],
    hasRepository?: Repository[],
    maintenanceStatus?: MaintenanceStatus[],
    projectIdentifier?: string,
    projectLicense?: License[],
}



export interface ProjectionContext {
}



export interface PronounSet extends InformationObject {
    pronounObject?: string,
    pronounPossessive?: string,
    pronounPossessiveDeterminer?: string,
    pronounReflexive?: string,
    pronounSubject?: string,
}



export interface ProximityMeasurement extends Measurement {
    proximityTo?: Entity,
}



export interface Push extends Activity {
    pushTarget?: string[],
}



export interface QualityAssessment extends Observation {
    assessedEntity?: Entity[],
    qualityDimension?: QualityDimension[],
}



export interface QualityDimension {
}



export interface Quantity extends Entity {
}



export interface ReadingOrder extends Standpoint {
}



export interface RecurrenceRule extends InformationObject {
    recurrenceRuleText?: string,
}



export interface Ref extends InformationObject {
    pointsToCommit?: Commit[],
}



export interface ReferenceFrame extends Entity {
    determinacyModel?: Determinacy,
    dimensionCount?: string,
    frameKind?: FrameKind,
    frameRealm?: FrameRealm,
    frameSolver?: string,
    hasAxis?: Axis[],
    hasMetricKind?: MetricKind,
    hasReferencePosition?: CelestialReferenceOrigin,
    hasTimeScale?: TimeScale,
    isHostedBy?: Entity[],
    parentFrame?: ReferenceFrame,
    requiresHost?: boolean,
    transformsTo?: ReferenceFrame[],
}



export interface ReferencePosition extends Entity {
}



export interface RegionSelector extends InformationObject {
    selectorType?: SelectorType,
    selectorValue?: string,
}



export interface RegulatoryOverlay extends TimeScopedRelation {
    overlayAuthority?: Agent,
    overlayDesignator?: string,
    overlayDeterminacy?: Determinacy[],
    overlayLowerBound?: ScalarQuantity[],
    overlayPlace?: Place,
    overlayRegulation?: RightsStatement[],
    overlayType?: RegulatoryOverlayType[],
    overlayUpperBound?: ScalarQuantity[],
}



export interface RegulatoryOverlayType {
}



export interface RelayHop {
    hopOrdinal?: number,
    relayAt?: string,
    relayBy?: string,
    relayFrom?: string,
    relayProtocol?: string,
}



export interface Release extends Event {
    releaseDoi?: string,
    releaseOf?: Project,
    releaseTag?: Tag[],
    releaseVersion?: string,
}



export interface Reminder extends Entity {
    reminderAction?: ReminderAction,
    reminderTarget?: Event,
    reminderTrigger?: string,
}



export interface ReminderAction {
}



export interface Repository extends InformationObject {
    cloneUrl?: string,
    hostedAt?: ForgePlatform[],
    materializationDepth?: string,
    repositoryType?: RepositoryType,
    webUrl?: string,
}



export interface RepositoryType {
}



export interface Review extends InformationObject {
}



export interface RightsAction {
}



export interface RightsStatement extends Observation {
    conflictStrategy?: ConflictStrategy,
    hasDataController?: Agent[],
    hasDataSubject?: Agent[],
    hasPermission?: Permission[],
    hasProhibition?: Prohibition[],
    rightsType?: RightsType[],
    statementAbout?: Entity[],
}



export interface RightsType {
}



export interface Role {
}



export interface RomanticOrientation extends IdentityFacet {
    romanticOrientationValue?: RomanticOrientationValue,
}



export interface RomanticOrientationValue {
}



export interface Route extends Entity {
    hasRouteSegment?: Route[],
    routeEnd?: string,
    routeKind?: RouteKind,
    routeStart?: string,
    routeVia?: string[],
}



export interface RouteKind {
}



export interface RsvpStatus {
}



export interface Rule {
    ruleAction?: RightsAction,
    ruleAssignee?: Agent[],
    ruleConsequence?: Duty[],
    ruleConstraint?: Constraint[],
    ruleTarget?: Entity,
}



export interface SLSALevel {
}



export interface SMIMESignature extends CryptographicSignature {
}



export interface ScalarQuantity extends Entity {
    quantityUncertainty?: string,
    quantityValue?: string,
}



export interface SceneGraphEdge {
    sceneConfidence?: string,
    sceneObject?: ImageRegion,
    sceneRelation?: SceneRelationType,
    sceneSubject?: ImageRegion,
}



export interface SceneRelationType {
}



export interface ScheduleException {
    exceptionOriginalDate?: string,
    exceptionReplacementEvent?: Event,
    exceptionSchedule?: EventSchedule,
    exceptionType?: ExceptionType,
}



export interface ScriptLanguageAttribution extends Observation {
    attributedLanguage?: Language[],
    attributedNotation?: NotationSystem[],
    attributedScript?: WritingSystem[],
    attributionTarget?: Inscription,
}



export interface ScriptRole {
}



export interface Selector extends EvidenceSpan {
    selectorLocator?: string,
    selectorPage?: string,
    selectorTextPosition?: string,
    selectorTextQuote?: string,
}



export interface SelectorType {
}



export interface SeniorityLevel {
}



export interface SensitivityLevel {
}



export interface Sensor extends Agent {
}



export interface SensorPlatform extends Entity {
    platformLocation?: Place[],
}



export interface SensoryEnvironment extends Entity {
    environmentAtInstant?: Instant[],
    environmentAtLocation?: Location,
    environmentDuringInterval?: TimeInterval[],
    hasMeasuredCondition?: CoordinateMatrix[],
    hasPerceivedCondition?: SensoryPerception[],
    sensoryModality?: SensoryModality[],
}



export interface SensoryModality {
}



export interface SensoryObservation extends Observation {
    sensoryObservationOf?: Entity[],
    sensoryProperty?: ObservableProperty[],
    sensoryResult?: SensoryQuantity[],
}



export interface SensoryPerception extends StandpointClaim {
    perceptionModality?: SensoryModality,
}



export interface SensoryQuantity extends Entity {
}



export interface SequenceCoordinates extends Entity {
    inReferenceAssembly?: ReferenceFrame,
    sequenceEnd?: string,
    sequenceStart?: string,
    sequenceStrand?: StrandOrientation,
}



export interface SequenceFeature extends Entity {
    hasSequenceCoordinates?: SequenceCoordinates[],
    sequenceFeatureType?: SequenceFeatureType[],
}



export interface SequenceFeatureType {
}



export interface SerialInstallment extends Manifestation {
}



export interface SerialWork extends Work {
}



export interface Service extends Work {
}



export interface SexAssignedAtBirth {
}



export interface SexualOrientation extends IdentityFacet {
    sexualOrientationValue?: SexualOrientationValue,
}



export interface SexualOrientationValue {
}



export interface SignatureScheme {
}



export interface SiteType {
}



export interface Skill extends Entity {
}



export interface SkillProficiency {
    skillProficiencyAgent?: Agent,
    skillProficiencyInterval?: TimeInterval[],
    skillProficiencyLevel?: ProficiencyLevel,
    skillProficiencyOf?: Skill,
    skillProficiencyScale?: ProficiencyScale,
}



export interface SmartContract extends Entity {
    contractAddress?: string,
}



export interface SocialObject extends Entity {
}



export interface SoftwareAgent extends Agent {
}



export interface SoftwareName extends Appellation {
}



export interface SoftwareProduct extends Work {
}



export interface SoftwareProject extends Project {
    writtenInLanguage?: ProgrammingLanguage[],
}



export interface SourceDirectory extends SourceTree {
}



export interface SourceFile extends SourceNode {
}



export interface SourceIndependence {
}



export interface SourceNode extends InformationObject {
}



export interface SourceRole extends CreativeWork {
}



export interface SourceTier {
}



export interface SourceTree extends SourceNode {
}



export interface SpatialAggregation extends Measurement {
    aggregationFunction?: AggregationFunction,
    hasBin?: SpatialBin[],
    minimumPopulation?: string,
}



export interface SpatialBin extends Place {
}



export interface SpatialCoordinates extends Entity {
    coordinateFrame?: ReferenceFrame,
}



export interface SpatialMeasurement extends Measurement {
    spatialMeasurementOf?: Entity[],
}



export interface Standpoint extends Entity {
    sharpens?: Standpoint[],
}



export interface StandpointClaim extends Observation {
    argumentAcceptability?: string,
    claimModality?: StandpointModality,
    claimVeridicality?: ClaimVeridicality[],
}



export interface StandpointModality {
}



export interface StandpointTenure extends TimeScopedRelation {
    standpointClaim?: StandpointClaim,
    tenurePosition?: string[],
    tenureStandpoint?: Standpoint,
}



export interface StepParentChild extends ParentChildRelationship {
}



export interface StepType {
}



export interface StorageLocation extends Location {
    physicalPlace?: Place[],
    storageMedium?: StorageMedium,
    storagePath?: string,
    storageService?: string,
}



export interface StorageMedium {
}



export interface StrandOrientation {
}



export interface Stream extends Entity {
    streamInterval?: TimeInterval,
    streamOf?: Entity,
    streamPlatform?: Agent[],
    streamSample?: Entity[],
    streamSensor?: Agent[],
}



export interface Summary extends InformationObject {
}



export interface SymbolicSystem extends InformationObject {
    symbolicSystemKind?: SymbolicSystemKind[],
}



export interface SymbolicSystemKind {
}



export interface Tag extends InformationObject {
    broaderTag?: Tag[],
    narrowerTag?: Tag[],
    relatedTag?: Tag[],
    tagInScheme?: TagScheme[],
}



export interface TagScheme extends InformationObject {
}



export interface Tagging {
    taggingInterval?: TimeInterval[],
    taggingScheme?: TagScheme,
    taggingTag?: Tag,
    taggingTagged?: Entity,
    taggingTagger?: Agent[],
}



export interface Task extends Event {
    taskDueDate?: string,
    taskPriority?: number,
    taskRecurrenceUntilDone?: boolean,
    taskStatus?: TaskStatus[],
}



export interface TaskStatus {
}



export interface TelephoneNumber extends ContactPoint {
}



export interface TemporalFrame extends ReferenceFrame {
    frameCalendarSystem?: CalendarSystem,
    frameReferencePosition?: ReferencePosition,
    frameTimeScale?: TimeScale,
}



export interface TemporalMeasurement extends Measurement {
    measuredAge?: string,
    measuredDate?: Instant,
    measurementDeterminacy?: Determinacy[],
    measurementMethod?: DatingMethod,
    measurementUncertainty?: string,
}



export interface TemporalPrecision {
}



export interface TextDirection {
}



export interface TextExtraction extends Document {
}



export interface Thread extends InformationObject {
    threadSubject?: string,
}



export interface TimeInterval {
    endedAtTime?: string,
    hasEndInstant?: Instant,
    hasStartInstant?: Instant,
    hasTemporalFrame?: TemporalFrame[],
    intervalAfter?: TimeInterval[],
    intervalBefore?: TimeInterval[],
    intervalCoincidesWith?: TimeInterval[],
    intervalContains?: TimeInterval[],
    intervalDuring?: TimeInterval[],
    intervalFinishedBy?: TimeInterval[],
    intervalFinishes?: TimeInterval[],
    intervalMeets?: TimeInterval[],
    intervalMetBy?: TimeInterval[],
    intervalOverlappedBy?: TimeInterval[],
    intervalOverlaps?: TimeInterval[],
    intervalStartedBy?: TimeInterval[],
    intervalStarts?: TimeInterval[],
    startedAtTime?: string,
}



export interface TimeScale extends Entity {
}



export interface TimeScopedRelation {
    duringInterval?: TimeInterval[],
}



export interface TimeZone extends Entity {
    timeZoneIanaId?: string,
}



export interface Trademark {
    registrationNumber?: string,
    trademarkHolder?: Agent[],
    trademarkMark?: Mark,
    trademarkStatus?: TrademarkStatus,
}



export interface TrademarkStatus {
}



export interface Trajectory extends Entity {
    hasTrajectorySample?: LocationState[],
    trajectoryOf?: Entity,
    trajectoryReferenceFrame?: ReferenceFrame,
}



export interface TransactionStatus {
}



export interface TransactionType {
}



export interface TransliterationScheme {
}



export interface TransparencyLogEntry extends InformationObject {
    logEntryIndex?: number,
    logEntryUrl?: string,
}



export interface TreeEntry extends InformationObject {
    treeEntryMode?: string,
    treeEntryName?: string,
    treeEntryObject?: SourceNode,
}



export interface TrustAssertion {
    introducerAmount?: number,
    introducerDepth?: number,
    trustLevel?: string,
    trustee?: Agent,
    trustor?: Agent,
}



export interface UsageAttestation extends Observation {
    attestationInterval?: TimeInterval,
    attestedForm?: LexicalForm,
    attestedInContext?: Entity[],
    attestedInLanguage?: Language[],
    attestedInSource?: CreativeWork[],
    attestedOnCarrier?: PhysicalObject[],
}



export interface Utilization extends Measurement {
    utilizationOf?: Location,
}



export interface VerificationActivity extends Activity {
}



export interface VerificationResult extends InformationObject {
    hasVerificationStatus?: VerificationStatus[],
    verifiedBy?: Agent[],
}



export interface VerificationStatus {
}



export interface VersionMembership extends Observation {
    membershipAuthority?: Agent[],
    membershipInterval?: TimeInterval[],
    versionMember?: Entity,
    versionRole?: VersionRole[],
    versionScale?: VersionScale[],
    versionSet?: VersionSet,
}



export interface VersionRole {
}



export interface VersionScale {
}



export interface VersionSet extends InformationObject {
}



export interface VirtualLocation extends Location {
    accessUrl?: string,
    hasNetworkAddress?: NetworkAddress[],
    virtualLocationType?: VirtualLocationType[],
    virtualPlatform?: string,
}



export interface VirtualLocationType {
}



export interface WalletScheme {
}



export interface WebPage extends Manifestation {
}



export interface Work extends CreativeWork {
    realizedThrough?: Expression[],
}



export interface WritingSystem extends InformationObject {
    scriptCode?: string,
    textDirection?: TextDirection[],
    writingSystemAsNotation?: NotationSystem[],
    writingSystemType?: WritingSystemType[],
}



export interface WritingSystemType {
}



export interface WritingSystemUsage {
    scriptRole?: ScriptRole,
    scriptUsageInterval?: TimeInterval[],
    usageLanguage?: Language,
    usageWritingSystem?: WritingSystem,
}
