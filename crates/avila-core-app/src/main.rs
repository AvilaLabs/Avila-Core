#![forbid(unsafe_code)]

//! Thin egui client over the semantic compiler.
//!
//! The application compiles the embedded specimen through the same compiler
//! the CLI uses and renders the report: the question, the contract, every
//! finding with its owner, pointer, and repair candidates, and the compiled
//! snapshot when one exists. It performs no calculation and holds no
//! scientific state of its own.

use avila_core_compiler::{
    CompilationStatus, CompileReport, ContractSource, CoreDiagnostic, FindingClass,
    RepairApplicability, compile_documents, explain,
};
use avila_core_kernel::VerdictStatus;
use eframe::egui;

const CORE_ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 140, 0);
const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(168, 173, 184);
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/branding/Avila_Core_Logo.png");
const CONTRACT_JSON: &[u8] =
    include_bytes!("../../../examples/contracts/shutdown-dose-specimen.json");
const REGISTRY_JSON: &[u8] =
    include_bytes!("../../../examples/registry/shutdown-dose-specimen.registry.json");

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_260.0, 800.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Avila Core",
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
    Findings,
    Compiled,
    Results,
    Evidence,
}

impl Workspace {
    const ALL: [Self; 6] = [
        Self::Overview,
        Self::Contract,
        Self::Findings,
        Self::Compiled,
        Self::Results,
        Self::Evidence,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Contract => "Contract",
            Self::Findings => "Findings",
            Self::Compiled => "Compiled snapshot",
            Self::Results => "Results",
            Self::Evidence => "Evidence",
        }
    }
}

struct Specimen {
    contract: ContractSource,
    report: CompileReport,
}

struct CoreApp {
    workspace: Workspace,
    logo: Option<egui::TextureHandle>,
    specimen: Result<Specimen, String>,
}

impl CoreApp {
    fn new(context: &egui::Context) -> Self {
        Self {
            workspace: Workspace::Overview,
            logo: load_logo_texture(context).ok(),
            specimen: load_specimen(),
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

        let specimen = match &self.specimen {
            Ok(specimen) => specimen,
            Err(error) => {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("Embedded specimen could not be read: {error}"),
                );
                return;
            }
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.workspace {
                Workspace::Overview => show_overview(ui, specimen),
                Workspace::Contract => show_contract(ui, &specimen.contract),
                Workspace::Findings => show_findings(ui, &specimen.report),
                Workspace::Compiled => show_compiled(ui, &specimen.report),
                Workspace::Results => show_results(ui, &specimen.contract),
                Workspace::Evidence => show_evidence(ui),
            });
    }
}

fn load_specimen() -> Result<Specimen, String> {
    let contract: ContractSource =
        serde_json::from_slice(CONTRACT_JSON).map_err(|error| error.to_string())?;
    let report =
        compile_documents(CONTRACT_JSON, REGISTRY_JSON).map_err(|error| error.to_string())?;
    Ok(Specimen { contract, report })
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
                        egui::RichText::new("SEMANTIC COMPILER")
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
                            "Contract-first compilation for portable computational evidence",
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
            "The included contract and registry are unqualified software specimens. Compilation establishes composability only; it cannot produce a technical or safety conclusion.",
        );
    });
}

fn show_overview(ui: &mut egui::Ui, specimen: &Specimen) {
    section_heading(
        ui,
        "What do you need to establish?",
        "Begin with a bounded question and its acceptance requirements, not a solver or a blank workflow.",
    );
    let contract = &specimen.contract;
    let report = &specimen.report;

    card(ui, |ui| {
        ui.label(
            egui::RichText::new("CURRENT DRAFT CONTRACT")
                .small()
                .strong(),
        );
        ui.heading(&contract.contract_id);
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
                &format!("{} STEPS", contract.workflow.len()),
                TEXT_MUTED,
            );
            badge(ui, &format!("{} INPUTS", contract.inputs.len()), TEXT_MUTED);
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
            "2  Compile",
            "Core resolves typed dataflow, checks every rule, and reports every finding with its owner and repair.",
        );
        overview_stage(
            &mut columns[2],
            "3  Review",
            "Receive PASS, FAIL, or INCONCLUSIVE with portable lineage and explicit limitations.",
        );
    });

    ui.add_space(10.0);
    card(ui, |ui| {
        ui.label(egui::RichText::new("NEXT ACTION").small().strong());
        let blocking: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.blocks_compilation())
            .collect();
        match (report.status, blocking.first()) {
            (CompilationStatus::Compiled, _) => {
                ui.label(
                    "The contract compiled. Package binding, execution, admission, and verdicts are not implemented, so nothing can run yet.",
                );
            }
            (CompilationStatus::Rejected, Some(first)) => {
                ui.colored_label(
                    CORE_ORANGE,
                    format!(
                        "{} blocking finding{} to resolve. First: {} at {} ({})",
                        blocking.len(),
                        if blocking.len() == 1 { "" } else { "s" },
                        first.code,
                        first.primary.pointer,
                        first.owner
                    ),
                );
                ui.label(&first.message);
            }
            (CompilationStatus::Rejected, None) => {
                ui.colored_label(CORE_ORANGE, "The contract was rejected.");
            }
        }
    });
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

fn show_contract(ui: &mut egui::Ui, contract: &ContractSource) {
    section_heading(
        ui,
        "Evidence contract",
        "The contract states the question, inputs, workflow, and requirements before anything executes.",
    );
    card(ui, |ui| {
        key_value(ui, "Contract ID", &contract.contract_id);
        key_value(ui, "Revision", &contract.revision.to_string());
        key_value(ui, "Status", "DRAFT / NOT APPROVED");
        key_value(ui, "Question", &contract.question);
        for assumption in &contract.assumptions {
            ui.colored_label(TEXT_MUTED, format!("Assumes: {assumption}"));
        }
    });
    ui.add_space(10.0);
    ui.columns(2, |columns| {
        card(&mut columns[0], |ui| {
            ui.label(egui::RichText::new("INPUTS").small().strong());
            for input in &contract.inputs {
                ui.separator();
                ui.label(egui::RichText::new(&input.input_id).strong());
                ui.colored_label(
                    TEXT_MUTED,
                    format!(
                        "{}@{}  ·  {}",
                        input.role.id, input.role.major, input.media_type
                    ),
                );
            }
        });
        card(&mut columns[1], |ui| {
            ui.label(egui::RichText::new("WORKFLOW").small().strong());
            for step in &contract.workflow {
                ui.separator();
                ui.label(egui::RichText::new(&step.step_id).strong());
                ui.colored_label(
                    TEXT_MUTED,
                    format!("{}@{}", step.capability_type.id, step.capability_type.major),
                );
                for (parameter, value) in &step.parameters {
                    ui.colored_label(TEXT_MUTED, format!("{parameter} = {value}"));
                }
            }
        });
    });
    ui.add_space(10.0);
    card(ui, |ui| {
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
                format!(
                    "Limit: {} {} ({})  ·  purpose {}@{}",
                    requirement.limit.value,
                    requirement.limit.unit,
                    requirement.limit.kind,
                    requirement.purpose.id,
                    requirement.purpose.major
                ),
            );
        }
    });
}

fn show_findings(ui: &mut egui::Ui, report: &CompileReport) {
    section_heading(
        ui,
        "Findings",
        "Every finding carries a stable code, a class, an accountable owner, a JSON Pointer, and typed repair candidates where a bounded repair exists.",
    );
    if report.findings.is_empty() {
        card(ui, |ui| {
            ui.label("No findings. The contract compiled cleanly under the draft profile.");
        });
        return;
    }
    for finding in &report.findings {
        show_finding(ui, finding);
        ui.add_space(7.0);
    }
}

fn show_finding(ui: &mut egui::Ui, finding: &CoreDiagnostic) {
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            class_badge(ui, finding.class);
            ui.label(
                egui::RichText::new(&finding.code)
                    .strong()
                    .color(CORE_ORANGE),
            );
            ui.colored_label(TEXT_MUTED, format!("owner: {}", finding.owner));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(format!(
                    "{}:{}",
                    finding.primary.document, finding.primary.pointer
                ));
            });
        });
        ui.label(&finding.message);
        for related in &finding.related {
            ui.colored_label(
                TEXT_MUTED,
                format!("related: {}:{}", related.document, related.pointer),
            );
        }
        for repair in &finding.repairs {
            let label = match repair.applicability {
                RepairApplicability::MechanicallySafe => "mechanically safe repair",
                RepairApplicability::ConstrainedChoice => "choose one",
                RepairApplicability::MethodOwnerJudgment => "method owner judgment",
            };
            ui.horizontal_wrapped(|ui| {
                badge(ui, label, egui::Color32::from_rgb(120, 164, 210));
                ui.monospace(repair.candidates.join("  |  "));
            });
        }
        if let Some(entry) = explain(&finding.code) {
            ui.colored_label(TEXT_MUTED, format!("Next action: {}", entry.next_action));
        }
    });
}

fn show_compiled(ui: &mut egui::Ui, report: &CompileReport) {
    section_heading(
        ui,
        "Compiled snapshot",
        "A content-identified description of the campaign. It establishes composability under the draft profile, nothing more.",
    );
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("COMPILATION").small().strong());
            match report.status {
                CompilationStatus::Compiled => badge(ui, "COMPILED", egui::Color32::LIGHT_GREEN),
                CompilationStatus::Rejected => badge(ui, "REJECTED", CORE_ORANGE),
            }
        });
        for identity in &report.source_identities {
            key_value(ui, &identity.document, &identity.sha256);
        }
        ui.colored_label(TEXT_MUTED, &report.notice);
    });
    let Some(compiled) = &report.compiled else {
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(
                "No snapshot exists because the contract was rejected. Resolve the findings first.",
            );
        });
        return;
    };
    ui.add_space(10.0);
    card(ui, |ui| {
        key_value(ui, "Snapshot", &compiled.snapshot_sha256);
        key_value(ui, "Compiler", &compiled.compiler);
        key_value(ui, "Semantic profile", &compiled.semantic_profile);
    });
    for (index, step) in compiled.workflow.iter().enumerate() {
        ui.add_space(7.0);
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
                    ui.colored_label(
                        TEXT_MUTED,
                        format!(
                            "{}@{}  ·  {:?}",
                            step.capability_type.id,
                            step.capability_type.major,
                            step.reproducibility.determinism
                        ),
                    );
                });
            });
            for binding in &step.bindings {
                ui.colored_label(
                    TEXT_MUTED,
                    format!("{} ← {}", binding.input_slot, binding.source.label()),
                );
            }
            if step.review_obligation.is_some() {
                badge(
                    ui,
                    "PENDING EXTERNAL REVIEW",
                    egui::Color32::from_rgb(120, 164, 210),
                );
            }
        });
    }
}

fn show_results(ui: &mut egui::Ui, contract: &ContractSource) {
    section_heading(
        ui,
        "Results",
        "A requirement receives a verdict only after admitted evidence exists. Compilation never produces one.",
    );
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
            "When implemented, an export will contain the compiled snapshot, hashed inputs, selected capability packages, execution receipts, outputs, lineage, verdicts, limitations, and reviews.",
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

fn class_badge(ui: &mut egui::Ui, class: FindingClass) {
    let (label, color) = match class {
        FindingClass::Missing => ("MISSING", egui::Color32::from_rgb(120, 164, 210)),
        FindingClass::Invalid => ("INVALID", egui::Color32::from_rgb(232, 102, 102)),
        FindingClass::Unsatisfied => ("UNSATISFIED", CORE_ORANGE),
        FindingClass::Inadmissible => ("INADMISSIBLE", egui::Color32::from_rgb(190, 120, 220)),
        FindingClass::Notice => ("NOTICE", TEXT_MUTED),
    };
    badge(ui, label, color);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_specimen_is_an_honest_draft() {
        let specimen = load_specimen().expect("embedded specimen should load");
        assert_eq!(specimen.report.status, CompilationStatus::Rejected);
        assert!(!specimen.report.findings.is_empty());
        for finding in &specimen.report.findings {
            assert_eq!(finding.class, FindingClass::Missing, "{finding:?}");
            assert_eq!(finding.owner, "requester", "{finding:?}");
            assert!(
                matches!(finding.code.as_str(), "CORE-S1301" | "CORE-T2501"),
                "only declared placeholders may block the specimen: {finding:?}"
            );
        }
    }

    #[test]
    fn provisional_logo_decodes() {
        let image = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
            .expect("logo should be a PNG");
        assert_eq!((image.width(), image.height()), (2000, 2000));
    }
}
