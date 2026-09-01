//! Deterministic contract compilation for Avila Core.
//!
//! Compilation resolves an authored contract against an explicit immutable
//! registry snapshot. It performs no execution, evidence admission, scientific
//! calculation, qualification judgment, or requirement verdict.

#![forbid(unsafe_code)]

mod compile;
mod diagnostic;
mod document;

pub use compile::{
    COMPILE_NOTICE, CompilationStatus, CompileReport, CompiledContract, CompiledRequirement,
    CompiledStep, CompilerError, DocumentIdentity, ResolvedBinding, compile_documents,
};
pub use diagnostic::{
    CORE_R3101, CORE_R3102, CORE_R3201, CORE_R3202, CORE_R3203, CORE_R3301, CORE_R3501, CORE_S1101,
    CORE_S1102, CORE_T2001, CORE_T2101, CORE_T2102, CORE_T2103, CORE_T2201, CORE_T2203, CORE_T2301,
    CoreDiagnostic, DiagnosticRepair, FindingClass, RepairApplicability, SourceLocation,
};
pub use document::{
    AuthoredBinding, BasisKind, BoundSide, COMPILE_REPORT_SCHEMA_VERSION,
    COMPILED_CONTRACT_SCHEMA_VERSION, CONTRACT_SCHEMA_VERSION, CapabilityTypeDefinition,
    ClaimModelDeclaration, Comparison, ContractInput, ContractSource, ContractStatus,
    InputSlotDefinition, KindRecord, OutputSlotDefinition, REGISTRY_SCHEMA_VERSION,
    RegistrySnapshot, RequirementBasis, RequirementSource, RoleDefinition, SourceRef,
    TypedQuantity, UnitRecord, VersionedRef, WorkflowStep, current_profile_contract,
};
