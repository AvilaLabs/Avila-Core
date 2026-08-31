#![forbid(unsafe_code)]

use avila_core_model::{CapabilityManifest, EvidenceContract, VerdictStatus};
use avila_core_runtime::{CampaignPlan, CampaignPlanner, PlanStatus, StepState};
use eframe::egui;

const CORE_ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 140, 0);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(168, 173, 184);
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/branding/Avila_Core_Logo.png");
const CONTRACT_JSON: &str = include_str!("../../../examples/contracts/shutdown-dose-specimen.json");
const CAPABILITY_JSON: [&str; 5] = [
    include_str!("../../../examples/capabilities/openmc-transport.specimen.json"),
    include_str!("../../../examples/capabilities/actinv-activation.specimen.json"),
    include_str!("../../../examples/capabilities/avila-dose.specimen.json"),
    include_str!("../../../examples/capabilities/avify-bounds.specimen.json"),
    include_str!("../../../examples/capabilities/core-requirement.specimen.json"),
];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_260.0, 800.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Avila Core — Project North Star",
        options,
        Box::new(|creation_context| {
            configure_style(&creation_context.egui_ctx);
            Ok(Box::new(CoreApp::new(&creation_context.egui_ctx)))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Workspace {
    #[default]
    Overview,
    Contract,
    Plan,
    Results,
    Evidence,
}

impl Workspace {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Contract,
        Self::Plan,
        Self::Results,
        Self::Evidence,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Contract => "Contract",
            Self::Plan => "Execution plan",
            Self::Results => "Results",
            Self::Evidence => "Evidence",
        }
    }
}

struct CoreApp {
    workspace: Workspace,
    logo: Option<egui::TextureHandle>,
    contract: Option<EvidenceContract>,
    plan: Option<CampaignPlan>,
    startup_error: Option<String>,
}

impl CoreApp {
    fn new(context: &egui::Context) -> Self {
        let logo = load_logo_texture(context).ok();
        let loaded = load_specimen();
        match loaded {
            Ok((contract, plan)) => Self {
                workspace: Workspace::Overview,
                logo,
                contract: Some(contract),
                plan: Some(plan),
                startup_error: None,
            },
            Err(error) => Self {
                workspace: Workspace::Overview,
                logo,
                contract: None,
                plan: None,
                startup_error: Some(error),
            },
        }
    }
}

impl eframe::App for CoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        show_header(ui, self.logo.as_ref());
        ui.add_space(8.0);
        show_scaffold_notice(ui);
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            for workspace in Workspace::ALL {
                if ui
                    .selectable_label(self.workspace == workspace, workspace.label())
                    .clicked()
                {
                    self.workspace = workspace;
                }
            }
        });
        ui.separator();

        if let Some(error) = &self.startup_error {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Embedded specimen rejected: {error}"),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.workspace {
                Workspace::Overview => {
                    show_overview(ui, self.contract.as_ref(), self.plan.as_ref())
                }
                Workspace::Contract => show_contract(ui, self.contract.as_ref()),
                Workspace::Plan => show_plan(ui, self.plan.as_ref()),
                Workspace::Results => show_results(ui, self.contract.as_ref()),
                Workspace::Evidence => show_evidence(ui),
            });
    }
}

fn load_specimen() -> Result<(EvidenceContract, CampaignPlan), String> {
    let contract: EvidenceContract =
        serde_json::from_str(CONTRACT_JSON).map_err(|error| error.to_string())?;
    let manifests: Vec<CapabilityManifest> = CAPABILITY_JSON
        .into_iter()
        .map(|json| serde_json::from_str(json).map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;
    let plan = CampaignPlanner
        .plan(&contract, &manifests)
        .map_err(|error| error.to_string())?;
    Ok((contract, plan))
}

fn load_logo_texture(context: &egui::Context) -> Result<egui::TextureHandle, String> {
    let decoded = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
        .map_err(|error| format!("embedded Core logo is not a valid PNG: {error}"))?
        .into_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    Ok(context.load_texture("avila-core-logo", color_image, egui::TextureOptions::LINEAR))
}

fn configure_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(18, 18, 18);
    visuals.window_fill = egui::Color32::from_rgb(24, 24, 24);
    visuals.extreme_bg_color = egui::Color32::from_rgb(12, 12, 12);
    visuals.faint_bg_color = egui::Color32::from_rgb(31, 31, 31);
    visuals.selection.bg_fill = egui::Color32::from_rgb(126, 70, 4);
    visuals.selection.stroke = egui::Stroke::new(1.0, CORE_ORANGE);
    context.set_visuals(visuals);
    context.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style.spacing.button_padding = egui::vec2(13.0, 7.0);
    });
}

fn show_header(ui: &mut egui::Ui, logo: Option<&egui::TextureHandle>) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(27, 27, 27))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(logo) = logo {
                    ui.add(
                        egui::Image::from_texture(logo)
                            .uv(egui::Rect::from_min_max(
                                egui::pos2(0.02, 0.25),
                                egui::pos2(0.98, 0.62),
                            ))
                            .fit_to_exact_size(egui::vec2(260.0, 100.0)),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("[ Avila Core ]")
                            .size(27.0)
                            .strong()
                            .color(CORE_ORANGE),
                    );
                }
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("PROJECT NORTH STAR")
                            .size(10.0)
                            .strong()
                            .color(TEXT_MUTED),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Resolve a technical question into reviewable evidence",
                        )
                        .size(18.0)
                        .strong(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Contract-first planning for portable computational evidence",
                        )
                        .color(TEXT_MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    badge(ui, "SCAFFOLD", CORE_ORANGE);
                });
            });
        });
}

fn show_scaffold_notice(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            CORE_ORANGE,
            egui::RichText::new("NO CALCULATION PERFORMED").strong(),
        );
        ui.label(
            "The included workflow is an unqualified software specimen. It cannot produce a technical or safety conclusion.",
        );
    });
}

fn show_overview(
    ui: &mut egui::Ui,
    contract: Option<&EvidenceContract>,
    plan: Option<&CampaignPlan>,
) {
    section_heading(
        ui,
        "What do you need to establish?",
        "Begin with a bounded question and its acceptance requirements—not a solver or a blank workflow.",
    );
    let Some(contract) = contract else {
        ui.label("No contract loaded.");
        return;
    };

    card(ui, |ui| {
        ui.label(
            egui::RichText::new("CURRENT DRAFT CONTRACT")
                .small()
                .strong(),
        );
        ui.heading(&contract.title);
        ui.label(&contract.question);
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            badge(ui, "DRAFT", egui::Color32::from_rgb(120, 164, 210));
            badge(
                ui,
                &format!("{} REQUIREMENT", contract.requirements.len()),
                TEXT_MUTED,
            );
            badge(
                ui,
                &format!("{} CAPABILITIES", contract.workflow.len()),
                TEXT_MUTED,
            );
        });
    });

    ui.add_space(10.0);
    ui.columns(3, |columns| {
        overview_stage(
            &mut columns[0],
            "1  Define",
            "Question, requirements, inputs, tolerances, assumptions, and review policy.",
        );
        overview_stage(
            &mut columns[1],
            "2  Resolve",
            "Core selects admissible capabilities and records every execution dependency.",
        );
        overview_stage(
            &mut columns[2],
            "3  Review",
            "Receive PASS, FAIL, or INCONCLUSIVE with portable lineage and explicit limitations.",
        );
    });

    if let Some(plan) = plan {
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(egui::RichText::new("NEXT ACTION").small().strong());
            match plan.status {
                PlanStatus::Ready => {
                    ui.label("Review the execution plan before approving a campaign.");
                }
                PlanStatus::Blocked => {
                    ui.colored_label(
                        CORE_ORANGE,
                        "The campaign is blocked. Implement and qualify its required capabilities.",
                    );
                }
            }
        });
    }
}

fn overview_stage(ui: &mut egui::Ui, title: &str, detail: &str) {
    card(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .size(17.0)
                .strong()
                .color(CORE_ORANGE),
        );
        ui.label(detail);
    });
}

fn show_contract(ui: &mut egui::Ui, contract: Option<&EvidenceContract>) {
    section_heading(
        ui,
        "Evidence contract",
        "The contract states the question, boundary, inputs, requirement, and admissibility policy before execution.",
    );
    let Some(contract) = contract else {
        ui.label("No contract loaded.");
        return;
    };

    card(ui, |ui| {
        key_value(ui, "Contract ID", &contract.contract_id);
        key_value(ui, "Status", "DRAFT / NOT APPROVED");
        key_value(ui, "Question", &contract.question);
    });
    ui.add_space(10.0);
    ui.columns(2, |columns| {
        card(&mut columns[0], |ui| {
            ui.label(egui::RichText::new("REQUIRED INPUTS").small().strong());
            for input in &contract.inputs {
                ui.separator();
                ui.label(egui::RichText::new(&input.role).strong());
                ui.colored_label(TEXT_MUTED, &input.uri);
            }
        });
        card(&mut columns[1], |ui| {
            ui.label(
                egui::RichText::new("ACCEPTANCE REQUIREMENTS")
                    .small()
                    .strong(),
            );
            for requirement in &contract.requirements {
                ui.separator();
                ui.label(egui::RichText::new(&requirement.requirement_id).strong());
                ui.label(&requirement.statement);
                ui.colored_label(
                    TEXT_MUTED,
                    format!("Limit: {} {}", requirement.limit, requirement.unit),
                );
            }
        });
    });
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.label(egui::RichText::new("EVIDENCE POLICY").small().strong());
        ui.label(format!(
            "Complete lineage: {}  ·  Content hashes: {}  ·  Review roles: {}",
            yes_no(contract.evidence_policy.require_complete_lineage),
            yes_no(contract.evidence_policy.require_content_hashes),
            contract.evidence_policy.required_review_roles.join(", ")
        ));
        ui.colored_label(
            CORE_ORANGE,
            "Specimen mode permits unqualified manifests for planning only; none may establish a verdict.",
        );
    });
}

fn show_plan(ui: &mut egui::Ui, plan: Option<&CampaignPlan>) {
    section_heading(
        ui,
        "Execution plan",
        "A deterministic dependency plan. Planning does not execute a solver or create evidence.",
    );
    let Some(plan) = plan else {
        ui.label("No plan available.");
        return;
    };

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("CAMPAIGN READINESS").small().strong());
            match plan.status {
                PlanStatus::Ready => badge(ui, "READY FOR REVIEW", egui::Color32::LIGHT_GREEN),
                PlanStatus::Blocked => badge(ui, "BLOCKED", CORE_ORANGE),
            }
        });
        ui.colored_label(TEXT_MUTED, &plan.notice);
    });
    ui.add_space(10.0);
    for (index, step) in plan.steps.iter().enumerate() {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:02}", index + 1))
                        .size(19.0)
                        .strong()
                        .color(CORE_ORANGE),
                );
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&step.step_id).strong());
                    ui.colored_label(TEXT_MUTED, &step.capability_type);
                });
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| match &step.state {
                        StepState::Ready => badge(ui, "READY", egui::Color32::LIGHT_GREEN),
                        StepState::Blocked { .. } => badge(ui, "BLOCKED", CORE_ORANGE),
                    },
                );
            });
            if let Some(capability_id) = &step.capability_id {
                key_value(ui, "Selected manifest", capability_id);
            }
            if let StepState::Blocked { reason } = &step.state {
                ui.colored_label(CORE_ORANGE, reason);
            }
        });
        ui.add_space(7.0);
    }
}

fn show_results(ui: &mut egui::Ui, contract: Option<&EvidenceContract>) {
    section_heading(
        ui,
        "Results",
        "A requirement receives a verdict only after every required capability and evidence gate succeeds.",
    );
    if let Some(contract) = contract {
        for requirement in &contract.requirements {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&requirement.requirement_id).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        verdict_badge(ui, VerdictStatus::NotEvaluated);
                    });
                });
                ui.label(&requirement.statement);
                ui.colored_label(
                    TEXT_MUTED,
                    "No observed value, uncertainty bound, or admissible evidence exists.",
                );
            });
        }
    }
    ui.add_space(10.0);
    card(ui, |ui| {
        ui.label(egui::RichText::new("VERDICT SEMANTICS").small().strong());
        ui.horizontal_wrapped(|ui| {
            verdict_badge(ui, VerdictStatus::Pass);
            ui.label("requirement established within its declared boundary");
        });
        ui.horizontal_wrapped(|ui| {
            verdict_badge(ui, VerdictStatus::Fail);
            ui.label("requirement contradicted within its declared boundary");
        });
        ui.horizontal_wrapped(|ui| {
            verdict_badge(ui, VerdictStatus::Inconclusive);
            ui.label("the available method or evidence cannot decide the requirement");
        });
    });
}

fn show_evidence(ui: &mut egui::Ui) {
    section_heading(
        ui,
        "Portable evidence",
        "Every claim should be traceable to immutable inputs, capability identity, execution receipts, outputs, policy, and review.",
    );
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("EVIDENCE PACKAGE").small().strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                badge(ui, "NOT CREATED", TEXT_MUTED);
            });
        });
        ui.heading("No evidence exists for this specimen campaign");
        ui.label(
            "When implemented, an export will contain the contract, hashed inputs, selected capability manifests, execution receipts, outputs, lineage, verdicts, limitations, and reviews.",
        );
        ui.add_enabled(false, egui::Button::new("Export evidence package"));
    });
}

fn section_heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(8.0);
    ui.heading(egui::RichText::new(title).size(25.0));
    ui.label(egui::RichText::new(subtitle).color(TEXT_MUTED));
    ui.add_space(10.0);
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(29, 29, 29))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(52, 52, 52)))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(14))
        .show(ui, contents);
}

fn key_value(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("{key}:")).strong());
        ui.label(value);
    });
}

fn badge(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.7)))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(10.0).strong().color(color));
        });
}

fn verdict_badge(ui: &mut egui::Ui, verdict: VerdictStatus) {
    let (label, color) = match verdict {
        VerdictStatus::Pass => ("PASS", egui::Color32::from_rgb(95, 197, 128)),
        VerdictStatus::Fail => ("FAIL", egui::Color32::from_rgb(232, 102, 102)),
        VerdictStatus::Inconclusive => ("INCONCLUSIVE", CORE_ORANGE),
        VerdictStatus::NotEvaluated => ("NOT EVALUATED", TEXT_MUTED),
    };
    badge(ui, label, color);
}

const fn yes_no(value: bool) -> &'static str {
    if value { "required" } else { "not required" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_specimen_loads_but_cannot_run() {
        let (_, plan) = load_specimen().expect("embedded specimen should load");
        assert_eq!(plan.status, PlanStatus::Blocked);
    }

    #[test]
    fn provisional_logo_decodes() {
        let image = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
            .expect("logo should be a PNG");
        assert_eq!((image.width(), image.height()), (2000, 2000));
    }
}
