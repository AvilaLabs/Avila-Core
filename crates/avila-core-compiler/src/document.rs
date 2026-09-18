use std::collections::BTreeMap;

use avila_core_kernel::{ExactNumber, SEMANTIC_PROFILE, VerdictStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONTRACT_SCHEMA_VERSION: &str = "avila.core/evidence-contract/v0.2-draft";
pub const REGISTRY_SCHEMA_VERSION: &str = "avila.core/registry-snapshot/v0.2-draft";
pub const COMPILE_REPORT_SCHEMA_VERSION: &str = "avila.core/compile-report/v0.2-draft";
pub const COMPILED_CONTRACT_SCHEMA_VERSION: &str = "avila.core/compiled-contract/v0.2-draft";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedRef {
    pub id: String,
    pub major: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    InReview,
    Approved,
    Retired,
}

/// SC-9 clause 6: which verdict states fulfill delivery and which named
/// inconclusive reasons are permitted. The block is a delivery statement,
/// not a verdict input — it never changes a derived verdict, only whether
/// the campaign's verdicts complete the contract. `not_evaluated` never
/// completes a substantive contract, so declaring it here is refused
/// (`CORE-A4701`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionBlock {
    /// The verdict states that fulfill delivery — some subset of `pass`,
    /// `fail`, `inconclusive`. `not_evaluated` is refused at compile.
    pub fulfilling_verdicts: Vec<VerdictStatus>,
    /// The inconclusive verdict rules an `inconclusive` verdict may carry
    /// and still fulfill delivery; any other inconclusive reason does not
    /// complete. Meaningless unless `inconclusive` is declared fulfilling.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permitted_inconclusive_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    #[serde(default)]
    pub permitted_nondeterministic_roles: Vec<VersionedRef>,
    /// Permits requirements whose basis is `nominal`, which compare a nominal
    /// value and use no uncertainty. The weakening must be explicit here.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permit_nominal_basis: bool,
    /// Requires a qualification assessment on the admitted evidence of every
    /// `bounded` or `enclosure` requirement. When true, evidence with no
    /// qualification assessment leaves the requirement `NOT_EVALUATED`
    /// (`CORE-A4402`) instead of being evaluated as if it were qualified.
    /// When false or absent, such evidence is still evaluated, but the
    /// verdict carries an informational reason (`CORE-A4403`) naming the
    /// gap. Nominal-basis requirements are unaffected either way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_qualification: bool,
    /// Issuers whose qualification records this organization recognizes:
    /// the record's `owner` mapped to the issuer's Ed25519 public key
    /// (hex). When non-empty, a claim qualified by a record whose owner is
    /// not listed cannot establish a bounded or enclosure requirement
    /// (`CORE-A4601`), and the runner applies a listed owner's record only
    /// when a signature document over its bound bytes verifies under the
    /// declared key — an unsigned or unverifiable record is refused at
    /// binding rather than attached. An empty map imposes no recognition
    /// constraint. Key rotation is a contract revision.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub recognized_qualification_owners: std::collections::BTreeMap<String, String>,
    /// Requires the case runner to refuse an unsigned package and unsigned
    /// SC-12 reuse (ADR-0015): a run must be given `--trust-root`, the
    /// package manifest's requester signature must verify, and every
    /// declared execution step's operative receipt must carry a signature
    /// verified against a listed runner key, or the run is refused. The
    /// compiler itself only carries this flag through to the compiled
    /// snapshot and notes it visibly (`CORE-A4404`); the refusal itself is
    /// the case runner's, because only it can see receipts and signatures.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_signatures: bool,
    /// Capability-type owners (the registry's provider identities) whose
    /// implementations may not be selected for any step (`CORE-P5301`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_providers: Vec<String>,
    /// When non-empty, the closed set of capability-type owners a selected
    /// implementation may come from (`CORE-P5301`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_providers: Vec<String>,
    /// No two steps' selected capability types may share an owner
    /// (`CORE-P5302`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_provider_independence: bool,
    /// No two steps' selected executable digests may be identical
    /// (`CORE-P5304`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_diverse_implementations: bool,
    /// The minimum provider-declared maturity a selected capability type
    /// must carry (`CORE-P5303`); maturity is a policy fact, never a
    /// quality score. A type with no declared maturity fails any declared
    /// floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity_floor: Option<CapabilityMaturity>,
    /// An `avila_provided` selection must carry a recorded
    /// `self_preference_check` (`CORE-P5501`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forbid_self_preference: bool,
    /// A selected cost estimate above the cap must carry a recorded
    /// confirmation (`CORE-P5401`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_cap: Option<PolicyCostCap>,
}

/// A provider's declared implementation maturity — a policy fact per
/// ADR-0020 clause 2, ordered for `maturity_floor` comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMaturity {
    Prototype,
    Development,
    Qualified,
    Production,
}

/// A cost ceiling in a named currency; estimates are exact-number strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyCostCap {
    pub value: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSource {
    pub schema_version: String,
    pub semantic_profile: String,
    pub contract_id: String,
    pub revision: u64,
    pub status: ContractStatus,
    /// The bounded question the contract exists to resolve. Prose the
    /// compiler carries into the compiled boundary and never interprets.
    pub question: String,
    /// Conditions accepted without being established by the campaign. Prose,
    /// never interpreted; typed facts with provenance are a separate record.
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub execution_policy: ExecutionPolicy,
    /// SC-9 clause 6 delivery statement. Absent means the contract declares
    /// no completion rule — the campaign still derives verdicts but no
    /// completion assessment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<CompletionBlock>,
    #[serde(default)]
    pub inputs: Vec<ContractInput>,
    pub workflow: Vec<WorkflowStep>,
    #[serde(default)]
    pub requirements: Vec<RequirementSource>,
    /// Closed-vocabulary requirements over non-quantity evidence. Kept
    /// separate from quantitative requirements so neither kind needs dummy
    /// fields from the other semantic domain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categorical_requirements: Vec<CategoricalRequirementSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractInput {
    pub input_id: String,
    pub role: VersionedRef,
    pub media_type: String,
    pub claim_model: ClaimModelDeclaration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub step_id: String,
    pub capability_type: VersionedRef,
    #[serde(default)]
    pub bindings: Vec<AuthoredBinding>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    #[serde(default)]
    pub reproducibility: ReproducibilityBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewPolicyBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default)]
    pub material_factors: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredBinding {
    pub input_slot: String,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceRef {
    ContractInput {
        input_id: String,
    },
    StepOutput {
        step_id: String,
        output_slot: String,
    },
}

impl SourceRef {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::ContractInput { input_id } => format!("input:{input_id}"),
            Self::StepOutput {
                step_id,
                output_slot,
            } => format!("step:{step_id}/{output_slot}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementSource {
    pub requirement_id: String,
    pub statement: String,
    pub purpose: VersionedRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<SourceRef>,
    pub comparison: Comparison,
    pub limit: TypedQuantity,
    /// Required by, and only meaningful for, an `equal` comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<TypedQuantity>,
    pub basis: RequirementBasis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoricalRequirementSource {
    pub requirement_id: String,
    pub statement: String,
    pub purpose: VersionedRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<SourceRef>,
    pub predicate: CategoricalPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operator", rename_all = "snake_case", deny_unknown_fields)]
pub enum CategoricalPredicate {
    Equals { value: String },
    InSet { values: Vec<String> },
}

impl CategoricalPredicate {
    #[must_use]
    pub fn accepted_values(&self) -> &[String] {
        match self {
            Self::Equals { value } => std::slice::from_ref(value),
            Self::InSet { values } => values,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedQuantity {
    pub kind: String,
    pub value: ExactNumber,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementBasis {
    pub kind: BasisKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    Bounded,
    Enclosure,
    Nominal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshot {
    pub schema_version: String,
    pub semantic_profile: String,
    pub registry_id: String,
    pub revision: u64,
    pub kinds: Vec<KindRecord>,
    pub purposes: Vec<PurposeDefinition>,
    pub roles: Vec<RoleDefinition>,
    pub capability_types: Vec<CapabilityTypeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurposeDefinition {
    pub purpose: VersionedRef,
    pub owner: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindRecord {
    pub kind_id: String,
    pub canonical_unit: String,
    pub unit_class: String,
    pub owner: String,
    pub units: Vec<UnitRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRecord {
    pub symbol: String,
    pub factor: ExactNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinition {
    pub role: VersionedRef,
    pub owner: String,
    pub validator: String,
    /// An embedded JSON Schema for the documents that fill this role as a
    /// free input, in the restricted subset the compiler's embedded shape
    /// validator supports (`type`, `properties`, `required`,
    /// `additionalProperties`, `items`, `enum`, `const`, `oneOf`, the
    /// canonical-decimal `pattern`, and `description`). `validator` stays a
    /// free-text identifier of the role's owner-named checker; this is the
    /// separate, structural, compiler-checkable half. The runner refuses a
    /// supplied free input that fails it before staging or execution
    /// (CORE-X1301); the compiler refuses a schema outside the subset at
    /// registry compile time (CORE-R3501).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_class: Option<String>,
    pub accepted_media_types: Vec<String>,
    pub permitted_claim_models: Vec<ClaimModelDeclaration>,
    /// The complete vocabulary for categorical values carried by this role.
    /// An empty list means the role is not a closed-set categorical role.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categorical_values: Vec<String>,
    #[serde(default)]
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityTypeDefinition {
    pub capability_type: VersionedRef,
    pub owner: String,
    /// The provider's declared implementation maturity — a policy fact per
    /// ADR-0020, used only by `execution_policy.maturity_floor`; it is not
    /// a scientific quality score and never enters a verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity: Option<CapabilityMaturity>,
    pub reproducibility: ReproducibilityDeclaration,
    #[serde(default)]
    pub inputs: Vec<InputSlotDefinition>,
    #[serde(default)]
    pub outputs: Vec<OutputSlotDefinition>,
    #[serde(default)]
    pub parameters: Vec<ParameterDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ReviewDeclaration>,
    /// ADR-0006 clause 8: present only when this type is a decision-rounding
    /// capability — the rounding transformation is a separately typed and
    /// qualified capability, never an implicit comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounding: Option<RoundingDeclaration>,
    #[serde(default)]
    pub non_claims: Vec<String>,
}

/// ADR-0006 clause 8: a decision-rounding capability's declared
/// transformation — the capability declares the rounding it performs, so a
/// requirement's comparison over its output is exact over declared
/// semantics, never over an implicit rounding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundingDeclaration {
    /// The exact rounding quantum, as a `Quantity`.
    pub quantum: QuantityValue,
    /// The rounding mode the capability applies.
    pub mode: RoundingMode,
    /// The authority — regulation or method — requiring the rounding.
    pub authority: String,
    /// The input slot carrying the raw, unrounded value — the raw-input
    /// edge the claim lineage keeps linked.
    pub raw_input_edge: String,
}

/// ADR-0006 clause 8: the closed rounding-mode vocabulary a
/// decision-rounding capability may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Toward negative infinity.
    Floor,
    /// Toward positive infinity.
    Ceiling,
    /// Discard the fraction — toward zero.
    TowardZero,
    /// Increase the magnitude — away from zero.
    AwayFromZero,
    /// Nearest quantum; ties move away from zero.
    HalfUp,
    /// Nearest quantum; ties move to the even multiple.
    HalfEven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDeclaration {
    pub reviewer_role: ReviewerRole,
    pub presented_input_slots: Vec<String>,
    pub decision_output_slot: String,
    pub allowed_dispositions: Vec<ReviewDisposition>,
}

/// Practical review is performed by a connected agent after Core has produced
/// its technical verdicts. The whole stage is optional; when present it gates
/// presentation to the user, never compilation or a technical verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerRole {
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDisposition {
    PresentToUser,
    RequestChanges,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicyBinding {
    pub reviewer_eligibility_policy: ImmutablePolicyRef,
    pub independence: ReviewIndependence,
    /// Practical instructions shown to this reviewer. The compiler binds the
    /// exact text but does not interpret whether the reviewer followed it.
    #[serde(default)]
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImmutablePolicyRef {
    pub policy_id: String,
    pub revision: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewIndependence {
    None,
    Constraints {
        requirements: Vec<IndependenceRequirement>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceRequirement {
    pub separated_from: ReviewParty,
    pub minimum_separation: SeparationLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewParty {
    Requester,
    MethodOwner,
    CapabilityProvider,
    Executor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeparationLevel {
    DifferentPerson,
    DifferentOrganization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityDeclaration {
    pub determinism: DeterminismClass,
    #[serde(default)]
    pub material_factors: Vec<ExecutionFactorDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    Deterministic,
    SeededStochastic,
    Nondeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFactorDefinition {
    pub factor_id: String,
    pub value_type: ParameterType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDefinition {
    pub parameter_id: String,
    pub required: bool,
    pub value_type: ParameterType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParameterType {
    Boolean,
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<IntegerBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<IntegerBound>,
    },
    ExactNumber {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<ExactBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<ExactBound>,
    },
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        allowed_values: Option<Vec<String>>,
    },
    Quantity {
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<QuantityBound>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<QuantityBound>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegerBound {
    pub value: i64,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactBound {
    pub value: ExactNumber,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityBound {
    pub value: QuantityValue,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityValue {
    pub value: ExactNumber,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSlotDefinition {
    pub slot_id: String,
    pub role: VersionedRef,
    pub accepted_media_types: Vec<String>,
    #[serde(default = "default_true")]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSlotDefinition {
    pub slot_id: String,
    pub role: VersionedRef,
    pub media_type: String,
    pub permitted_claim_models: Vec<ClaimModelDeclaration>,
    /// SC-5 clause 6: whether a declared partial result is admitted for this
    /// slot. Absent means partial results are never admitted here — a claim
    /// recorded `partial` on this slot quarantines with `CORE-E7201`.
    #[serde(default)]
    pub permits_partial: bool,
    #[serde(default)]
    pub excluded_purposes: Vec<VersionedRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClaimModelDeclaration {
    Exact,
    Interval {
        #[serde(default)]
        nominal: bool,
    },
    CoverageInterval,
    WorstCase {
        side: BoundSide,
        #[serde(default)]
        nominal: bool,
    },
    StandardUncertainty,
    Samples,
    Distribution,
    Unquantified,
}

impl ClaimModelDeclaration {
    #[must_use]
    pub const fn is_kernel_irreducible(&self) -> bool {
        matches!(
            self,
            Self::StandardUncertainty | Self::Samples | Self::Distribution
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundSide {
    Lower,
    Upper,
}

const fn default_true() -> bool {
    true
}

pub fn current_profile_contract(contract_id: impl Into<String>) -> ContractSource {
    ContractSource {
        schema_version: CONTRACT_SCHEMA_VERSION.into(),
        semantic_profile: SEMANTIC_PROFILE.into(),
        contract_id: contract_id.into(),
        revision: 1,
        status: ContractStatus::Draft,
        completion: None,
        question: String::new(),
        assumptions: Vec::new(),
        execution_policy: ExecutionPolicy::default(),
        inputs: Vec::new(),
        workflow: Vec::new(),
        requirements: Vec::new(),
        categorical_requirements: Vec::new(),
    }
}
