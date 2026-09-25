//! Replaying a recorded `VerdictDerivation` against fresh evaluation
//! (ADR-0026).
//!
//! `verify_derivation` never trusts the serialized record: it re-runs the
//! whole evaluation from the supplied contract, registry, claims, and
//! artifact observations, then compares the fresh derivation with the
//! recorded one application by application. A forged conclusion is a
//! `mismatch` even when the forger honestly recomputed the outer digest —
//! the digest proves the file, the replay proves the inference.
//!
//! Outcomes are per check: `verified`, `mismatch`, or `not_checked`
//! (material unavailable or the record's shape unsupported). A
//! `not_checked` never reads as success.

use serde::Serialize;

use super::context::{ArtifactObservations, CONTEXT_SCHEMA_VERSION};
use super::derivation::{DERIVATION_SCHEMA_VERSION, VerdictDerivation};
use super::{CampaignStatus, EVALUATOR_ID, evaluate_campaign_in_context};
use crate::CompilerError;
use crate::compile::prefixed_sha256;
use avila_core_kernel::{SEMANTIC_PROFILE, canonicalize_json};

/// The state one replayed check reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivationCheckState {
    /// The recorded value equals the recomputed one.
    Verified,
    /// The recorded value differs from the recomputed one — the inference
    /// does not replay.
    Mismatch,
    /// The material needed for this check was unavailable or its shape is
    /// not supported by this verifier.
    NotChecked,
}

/// One check the replay performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivationCheck {
    /// `identity` | `context` | `application` | `campaign` | `schema`
    pub check: String,
    /// What the check covers: the document, the context, or
    /// `rule`/`subject` of one application.
    pub subject: String,
    pub state: DerivationCheckState,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// The replay verdict for one recorded derivation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivationVerification {
    /// `verified` only when every performed check verified.
    pub status: String,
    pub checks: Vec<DerivationCheck>,
    pub notice: String,
}

impl DerivationVerification {
    /// Whether every performed check verified — the only state a caller
    /// may treat as "the record replays".
    pub fn is_verified(&self) -> bool {
        self.status == "verified"
    }
}

/// Replay `derivation_bytes` against a fresh evaluation of the supplied
/// material. `Err` means the recorded document could not even be read;
/// a readable but wrong record is an `Ok` with `mismatch` checks.
pub fn verify_derivation(
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    claims_bytes: &[u8],
    observations: ArtifactObservations,
    derivation_bytes: &[u8],
) -> Result<DerivationVerification, CompilerError> {
    let mut checks = Vec::new();
    let recorded: VerdictDerivation =
        serde_json::from_slice(derivation_bytes).map_err(|error| {
            CompilerError::Serialization(format!("derivation is not a readable document: {error}"))
        })?;

    if recorded.schema_version != DERIVATION_SCHEMA_VERSION
        || recorded.semantic_profile != SEMANTIC_PROFILE
    {
        checks.push(DerivationCheck {
            check: "schema".into(),
            subject: "derivation".into(),
            state: DerivationCheckState::NotChecked,
            detail: format!(
                "record declares `{}` under `{}`; this verifier supports `{DERIVATION_SCHEMA_VERSION}` under `{SEMANTIC_PROFILE}`",
                recorded.schema_version, recorded.semantic_profile
            ),
        });
        return Ok(finish(checks));
    }

    match &recorded.derivation_sha256 {
        Some(recorded_identity) => match recorded.recompute_identity() {
            Ok(recomputed) => checks.push(DerivationCheck {
                check: "identity".into(),
                subject: "derivation_sha256".into(),
                state: if recomputed == *recorded_identity {
                    DerivationCheckState::Verified
                } else {
                    DerivationCheckState::Mismatch
                },
                detail: if recomputed == *recorded_identity {
                    String::new()
                } else {
                    format!("recorded `{recorded_identity}` but the body hashes to `{recomputed}`")
                },
            }),
            Err(error) => checks.push(DerivationCheck {
                check: "identity".into(),
                subject: "derivation_sha256".into(),
                state: DerivationCheckState::NotChecked,
                detail: format!("the recorded body could not be canonicalized: {error}"),
            }),
        },
        None => checks.push(DerivationCheck {
            check: "identity".into(),
            subject: "derivation_sha256".into(),
            state: DerivationCheckState::NotChecked,
            detail: "the record carries no derivation_sha256 to recompute".into(),
        }),
    }

    // The embedded context record must itself hash to `context_sha256` —
    // an attacker who edits bound identities inside `context` and
    // recomputes only the outer digest fails here.
    match serde_json::to_vec(&recorded.context)
        .map_err(|error| error.to_string())
        .and_then(|bytes| canonicalize_json(&bytes).map_err(|error| error.to_string()))
    {
        Ok(canonical) => {
            let embedded = prefixed_sha256(&canonical);
            checks.push(DerivationCheck {
                check: "context".into(),
                subject: "context_record".into(),
                state: if embedded == recorded.context_sha256 {
                    DerivationCheckState::Verified
                } else {
                    DerivationCheckState::Mismatch
                },
                detail: if embedded == recorded.context_sha256 {
                    String::new()
                } else {
                    format!(
                        "the embedded context hashes to `{embedded}`, not the recorded `{}`",
                        recorded.context_sha256
                    )
                },
            });
        }
        Err(error) => checks.push(DerivationCheck {
            check: "context".into(),
            subject: "context_record".into(),
            state: DerivationCheckState::NotChecked,
            detail: format!("the embedded context could not be canonicalized: {error}"),
        }),
    }

    // The evaluator metadata must name this verifier — a record produced
    // by different inference rules is unsupported material, not an
    // independently verified derivation.
    if recorded.evaluator == EVALUATOR_ID
        && recorded.context.evaluator == EVALUATOR_ID
        && recorded.context.semantic_profile == SEMANTIC_PROFILE
        && recorded.context.schema_version == CONTEXT_SCHEMA_VERSION
    {
        checks.push(DerivationCheck {
            check: "context".into(),
            subject: "evaluator".into(),
            state: DerivationCheckState::Verified,
            detail: format!("{EVALUATOR_ID} under {SEMANTIC_PROFILE}"),
        });
    } else {
        checks.push(DerivationCheck {
            check: "context".into(),
            subject: "evaluator".into(),
            state: DerivationCheckState::NotChecked,
            detail: format!(
                "the record names evaluator `{recorded_evaluator}`/`{context_evaluator}` under `{profile}`/`{schema}`; this verifier supports `{EVALUATOR_ID}` under `{SEMANTIC_PROFILE}`/`{CONTEXT_SCHEMA_VERSION}`",
                recorded_evaluator = recorded.evaluator,
                context_evaluator = recorded.context.evaluator,
                profile = recorded.context.semantic_profile,
                schema = recorded.context.schema_version,
            ),
        });
    }

    let evaluation =
        evaluate_campaign_in_context(contract_bytes, registry_bytes, claims_bytes, observations)?;
    let Some(fresh) = evaluation.derivation() else {
        // The evaluation was refused before a context existed (compile or
        // bind failure) — the report's findings explain why; nothing about
        // the recorded derivation can be checked against it.
        checks.push(DerivationCheck {
            check: "context".into(),
            subject: "context_sha256".into(),
            state: DerivationCheckState::NotChecked,
            detail: format!(
                "the supplied material evaluates to `{}` without a bound context",
                status_label(evaluation.report().status)
            ),
        });
        return Ok(finish(checks));
    };

    checks.push(DerivationCheck {
        check: "context".into(),
        subject: "context_sha256".into(),
        state: if fresh.context_sha256 == recorded.context_sha256 {
            DerivationCheckState::Verified
        } else {
            DerivationCheckState::Mismatch
        },
        detail: if fresh.context_sha256 == recorded.context_sha256 {
            String::new()
        } else {
            format!(
                "the supplied material binds context `{}`, not the recorded `{}`",
                fresh.context_sha256, recorded.context_sha256
            )
        },
    });

    // The embedded record must equal what the supplied material binds —
    // field-for-field, not merely a colliding hash.
    checks.push(DerivationCheck {
        check: "context".into(),
        subject: "bound_material".into(),
        state: if fresh.context == recorded.context {
            DerivationCheckState::Verified
        } else {
            DerivationCheckState::Mismatch
        },
        detail: if fresh.context == recorded.context {
            String::new()
        } else {
            "the embedded context record differs from the material the supplied documents bind"
                .into()
        },
    });

    let shared = fresh.applications.len().min(recorded.applications.len());
    for index in 0..shared {
        let fresh_application = &fresh.applications[index];
        let recorded_application = &recorded.applications[index];
        let subject = format!(
            "{}:{}",
            recorded_application.rule, recorded_application.subject
        );
        let equal = fresh_application == recorded_application;
        checks.push(DerivationCheck {
            check: "application".into(),
            subject: subject.clone(),
            state: if equal {
                DerivationCheckState::Verified
            } else {
                DerivationCheckState::Mismatch
            },
            detail: if equal {
                String::new()
            } else {
                application_diff_detail(recorded_application, fresh_application)
            },
        });
    }
    for application in recorded.applications.iter().skip(shared) {
        checks.push(DerivationCheck {
            check: "application".into(),
            subject: format!("{}:{}", application.rule, application.subject),
            state: DerivationCheckState::Mismatch,
            detail: "the replay produced no such application".into(),
        });
    }
    for application in fresh.applications.iter().skip(shared) {
        checks.push(DerivationCheck {
            check: "application".into(),
            subject: format!("{}:{}", application.rule, application.subject),
            state: DerivationCheckState::Mismatch,
            detail: "the record omits this replayed application".into(),
        });
    }

    match (&recorded.campaign_sha256, &evaluation.report().campaign_sha256) {
        (Some(recorded_identity), Some(fresh_identity)) => checks.push(DerivationCheck {
            check: "campaign".into(),
            subject: "campaign_sha256".into(),
            state: if recorded_identity == fresh_identity {
                DerivationCheckState::Verified
            } else {
                DerivationCheckState::Mismatch
            },
            detail: if recorded_identity == fresh_identity {
                String::new()
            } else {
                format!(
                    "the fresh report is `{fresh_identity}`, not the recorded `{recorded_identity}`"
                )
            },
        }),
        // A record that omits the campaign identity it claims to derive
        // is never silently verified — the omission is reported.
        (None, Some(_)) => checks.push(DerivationCheck {
            check: "campaign".into(),
            subject: "campaign_sha256".into(),
            state: DerivationCheckState::NotChecked,
            detail:
                "the record omits the campaign identity although the replayed evaluation produced one"
                    .into(),
        }),
        (Some(recorded_identity), None) => checks.push(DerivationCheck {
            check: "campaign".into(),
            subject: "campaign_sha256".into(),
            state: DerivationCheckState::Mismatch,
            detail: format!(
                "the record names campaign `{recorded_identity}` but the replayed evaluation produced none"
            ),
        }),
        // Refused evaluations legitimately bind no report — nothing to
        // compare on either side.
        (None, None) => {}
    }

    Ok(finish(checks))
}

/// What differs between the recorded and replayed application, in words.
fn application_diff_detail(
    recorded: &super::derivation::RuleApplication,
    fresh: &super::derivation::RuleApplication,
) -> String {
    if recorded.rule != fresh.rule || recorded.subject != fresh.subject {
        return format!(
            "the replay ran `{}` on `{}` at this position",
            fresh.rule, fresh.subject
        );
    }
    if recorded.conclusion != fresh.conclusion {
        return format!(
            "the conclusion replays as `{}`, not the recorded `{}`",
            fresh.conclusion, recorded.conclusion
        );
    }
    if recorded.premises != fresh.premises {
        return "the premises differ from the replayed check".into();
    }
    "the recorded reasons differ from the replayed check".into()
}

fn finish(checks: Vec<DerivationCheck>) -> DerivationVerification {
    let status = if checks
        .iter()
        .all(|check| check.state == DerivationCheckState::Verified)
    {
        "verified"
    } else if checks
        .iter()
        .any(|check| check.state == DerivationCheckState::Mismatch)
    {
        "mismatch"
    } else {
        "not_checked"
    };
    DerivationVerification {
        status: status.into(),
        checks,
        notice: super::DERIVATION_NOTICE.into(),
    }
}

fn status_label(status: CampaignStatus) -> &'static str {
    match status {
        CampaignStatus::Evaluated => "evaluated",
        CampaignStatus::Rejected => "rejected",
    }
}
