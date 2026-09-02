//! Deterministic contract compilation for Avila Core.
//!
//! Compilation resolves an authored contract against an explicit immutable
//! registry snapshot. It performs no execution, evidence admission, scientific
//! calculation, qualification judgment, or requirement verdict.

#![forbid(unsafe_code)]

mod campaign;
mod catalog;
mod compile;
mod coverage;
mod diagnostic;
mod document;
mod qualification;
mod render;

pub use campaign::{
    AdmissionReason, AdmissionRecord, AdmissionState, ArtifactIdentity, CAMPAIGN_NOTICE,
    CAMPAIGN_REPORT_SCHEMA_VERSION, CLAIMS_SCHEMA_VERSION, CampaignReport, CampaignStatus,
    ClaimValue, ClaimsDocument, InputAttestation, OutputClaim, ProducerIdentity, VerdictBoundary,
    VerdictRecord, evaluate_campaign,
};
pub use catalog::{DIAGNOSTIC_CATALOG, DiagnosticExplanation, explain};
pub use compile::{
    COMPILE_NOTICE, CanonicalTypedQuantity, CompilationStatus, CompileReport, CompiledContract,
    CompiledParameterValue, CompiledPresentationGate, CompiledReproducibility, CompiledRequirement,
    CompiledStep, CompilerError, DocumentIdentity, PresentationGateState, ResolvedBinding,
    compile_documents,
};
pub use coverage::{
    COVERAGE_REPORT_SCHEMA_VERSION, CoverageDeclaration, CoverageEntry, CoverageReport,
    CoverageState, CoverageStatus, CoveringRequirement, DeclaredOmission, OmissionPolicy,
    REQUIREMENT_SET_SCHEMA_VERSION, RequirementSet, SetRequirement, assess_coverage,
    parse_requirement_set,
};
pub use diagnostic::{
    COMPILER_FINDING_CODES, CORE_A4201, CORE_A4301, CORE_E7001, CORE_E7002, CORE_E7101, CORE_E7103,
    CORE_E7201, CORE_E7301, CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301,
    CORE_R3401, CORE_R3501, CORE_R3601, CORE_R3602, CORE_S1101, CORE_S1102, CORE_S1103, CORE_S1301,
    CORE_T2001, CORE_T2101, CORE_T2102, CORE_T2103, CORE_T2104, CORE_T2201, CORE_T2203, CORE_T2301,
    CORE_T2401, CORE_T2402, CORE_T2501, CORE_T2601, CoreDiagnostic, DiagnosticRepair, FindingClass,
    RepairApplicability, RepairEdit, SourceLocation,
};
pub use document::{
    AuthoredBinding, BasisKind, BoundSide, COMPILE_REPORT_SCHEMA_VERSION,
    COMPILED_CONTRACT_SCHEMA_VERSION, CONTRACT_SCHEMA_VERSION, CapabilityTypeDefinition,
    ClaimModelDeclaration, Comparison, ContractInput, ContractSource, ContractStatus,
    DeterminismClass, ExactBound, ExecutionFactorDefinition, ExecutionPolicy, ImmutablePolicyRef,
    IndependenceRequirement, InputSlotDefinition, IntegerBound, KindRecord, OutputSlotDefinition,
    ParameterDefinition, ParameterType, PurposeDefinition, QuantityBound, QuantityValue,
    REGISTRY_SCHEMA_VERSION, RegistrySnapshot, ReproducibilityBinding, ReproducibilityDeclaration,
    RequirementBasis, RequirementSource, ReviewDeclaration, ReviewDisposition, ReviewIndependence,
    ReviewParty, ReviewPolicyBinding, ReviewerRole, RoleDefinition, SeparationLevel, SourceRef,
    TypedQuantity, UnitRecord, VersionedRef, WorkflowStep, current_profile_contract,
};
pub use qualification::{
    ClaimQualification, EnvelopeAssessment, EnvelopeState, EnvelopeTerm,
    QUALIFICATION_SCHEMA_VERSION, QualificationRecord, QualifiedCapability, ValidationEvidence,
    evaluate_envelope, parse_qualification, registry_kinds,
};
pub use render::{
    SourceSpan, locate, render_campaign_report, render_compile_report, render_findings,
};
