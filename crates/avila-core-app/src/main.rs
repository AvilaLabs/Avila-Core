#![forbid(unsafe_code)]

//! Thin egui client over the semantic compiler and the case runner.
//!
//! The case workbench runs a composed case through the same runner the CLI
//! uses and renders its report stage by stage; the specimen view compiles the
//! embedded specimen through the same compiler and renders its findings. The
//! application performs no calculation and holds no scientific state of its
//! own.

mod case_view;
mod help;

use avila_core_compiler::{
    CompilationStatus, CompileReport, ContractSource, CoreDiagnostic, FindingClass,
    RepairApplicability, compile_documents, explain,
};
use avila_core_kernel::VerdictStatus;
use eframe::egui;
use help::{GuidedHelp, HelpView, TourTarget, TourTargets};

const CORE_ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 140, 0);

/// Muted text for the active theme.
fn muted(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(168, 173, 184)
    } else {
        egui::Color32::from_rgb(96, 100, 110)
    }
}
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/branding/Avila_Core_Logo.png");
const CONTRACT_JSON: &[u8] =
    include_bytes!("../../../examples/contracts/shutdown-dose-specimen.json");
const REGISTRY_JSON: &[u8] =
    include_bytes!("../../../examples/registry/shutdown-dose-specimen.registry.json");

fn main() -> eframe::Result {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let setup = match case_view::CaseSetup::from_arguments(&arguments) {
        Ok(setup) => setup,
        Err(error) => {
            eprintln!(
                "error: {error}\nusage: avila-core-app [--case DIR] [--source-root NAME=PATH]... [--capability NAME=PATH]... [--workspace DIR] [--no-reuse] [--auto-run | --auto-plan] [--screenshot PNG] [--tab NAME]"
            );
            std::process::exit(2);
        }
    };
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
            Ok(Box::new(CoreApp::new(&creation_context.egui_ctx, setup)))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Case,
    Specimen,
}

struct CoreApp {
    mode: Mode,
    workspace: Workspace,
    logo: Option<egui::TextureHandle>,
    specimen: Result<Specimen, String>,
    case: case_view::CaseView,
    help: GuidedHelp,
}

impl CoreApp {
    fn new(context: &egui::Context, setup: case_view::CaseSetup) -> Self {
        if setup.light {
            context.set_theme(egui::Theme::Light);
        }
        let mut help = GuidedHelp::default();
        if let Some(guide) = setup.tour.as_deref().and_then(help::GuideKind::by_name) {
            help.start_tour(guide);
        }
        Self {
            mode: Mode::Case,
            workspace: Workspace::Overview,
            logo: load_logo_texture(context).ok(),
            specimen: load_specimen(),
            case: case_view::CaseView::new(setup),
            help,
        }
    }

    fn current_view(&self) -> HelpView {
        match self.mode {
            Mode::Case => HelpView::Case(self.case.help_tab()),
            Mode::Specimen => HelpView::Specimen,
        }
    }

    /// A tour step may ask for a mode and tab so its target is on screen.
    fn apply_requested_view(&mut self) {
        match self.help.requested_view() {
            Some(HelpView::Case(tab)) => {
                self.mode = Mode::Case;
                self.case.show_help_tab(tab);
            }
            Some(HelpView::Specimen) => self.mode = Mode::Specimen,
            None => {}
        }
    }

    fn specimen_ui(&mut self, ui: &mut egui::Ui, targets: &mut TourTargets) {
        let notice = ui.scope(show_scaffold_notice);
        targets.set(TourTarget::SpecimenNotice, notice.response.rect);
        ui.add_space(8.0);

        let navigation = ui.horizontal_wrapped(|ui| {
            for workspace in Workspace::ALL {
                if ui
                    .selectable_label(self.workspace == workspace, workspace.label())
                    .clicked()
                {
                    self.workspace = workspace;
                }
            }
        });
        targets.set(TourTarget::SpecimenNavigation, navigation.response.rect);
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

impl eframe::App for CoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input(|input| input.key_pressed(egui::Key::F1)) {
            self.help.toggle_center();
        }
        self.apply_requested_view();
        let mut targets = TourTargets::default();

        // Paint the root background from the active theme; the window's clear
        // color does not follow a theme switch.
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        show_header(ui, self.logo.as_ref(), &mut self.help, &mut targets);
        ui.add_space(8.0);
        let switch = ui.horizontal(|ui| {
            for (mode, label) in [
                (Mode::Case, "Case workbench"),
                (Mode::Specimen, "Specimen compiler"),
            ] {
                if ui.selectable_label(self.mode == mode, label).clicked() {
                    self.mode = mode;
                }
            }
        });
        targets.set(TourTarget::ModeSwitch, switch.response.rect);
        ui.separator();
        if self.mode == Mode::Case {
            self.case.ui(ui, &mut targets);
        } else {
            self.specimen_ui(ui, &mut targets);
        }
        let view = self.current_view();
        self.help.show_center(ui.ctx(), view);
        self.help.show_tour(ui.ctx(), &targets);
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
    let mut dark = egui::Visuals::dark();
    dark.panel_fill = egui::Color32::from_rgb(18, 18, 18);
    dark.window_fill = egui::Color32::from_rgb(24, 24, 24);
    dark.extreme_bg_color = egui::Color32::from_rgb(12, 12, 12);
    dark.faint_bg_color = egui::Color32::from_rgb(31, 31, 31);
    dark.selection.bg_fill = egui::Color32::from_rgb(126, 70, 4);
    dark.selection.stroke = egui::Stroke::new(1.0, CORE_ORANGE);
    context.set_visuals_of(egui::Theme::Dark, dark);
    let mut light = egui::Visuals::light();
    light.panel_fill = egui::Color32::from_rgb(246, 246, 247);
    light.window_fill = egui::Color32::WHITE;
    light.extreme_bg_color = egui::Color32::WHITE;
    light.faint_bg_color = egui::Color32::from_rgb(236, 236, 238);
    light.selection.bg_fill = egui::Color32::from_rgb(255, 214, 160);
    light.selection.stroke = egui::Stroke::new(1.0, CORE_ORANGE);
    context.set_visuals_of(egui::Theme::Light, light);
    context.set_theme(egui::Theme::Dark);
    context.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style.spacing.button_padding = egui::vec2(13.0, 7.0);
    });
}

fn show_header(
    ui: &mut egui::Ui,
    logo: Option<&egui::TextureHandle>,
    help: &mut GuidedHelp,
    targets: &mut TourTargets,
) {
    let dark = ui.visuals().dark_mode;
    egui::Frame::new()
        .fill(if dark {
            egui::Color32::from_rgb(27, 27, 27)
        } else {
            egui::Color32::from_rgb(235, 235, 237)
        })
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            let (brand_rect, (theme_rect, help_rect)) = egui::Sides::new().show(
                ui,
                |ui| {
                    let brand = ui.horizontal(|ui| {
                        match logo {
                            Some(logo) => {
                                // The asset is a square canvas with the wordmark in
                                // its middle band: crop to the brackets and show the
                                // crop at its own proportions, not the canvas's.
                                ui.add(
                                    egui::Image::from_texture(logo)
                                        .uv(egui::Rect::from_min_max(
                                            egui::pos2(0.04, 0.32),
                                            egui::pos2(0.96, 0.70),
                                        ))
                                        .fit_to_exact_size(egui::vec2(218.0, 90.0))
                                        .maintain_aspect_ratio(false)
                                        .corner_radius(6),
                                );
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new("[ Avila Core ]")
                                        .size(27.0)
                                        .strong()
                                        .color(CORE_ORANGE),
                                );
                            }
                        }
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("SEMANTIC COMPILER AND CASE RUNNER")
                                    .size(10.0)
                                    .strong()
                                    .color(muted(ui)),
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
                                .color(muted(ui)),
                            );
                        });
                    });
                    brand.response.rect
                },
                |ui| {
                    let row = ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Min),
                        |ui| {
                            badge(ui, "SCAFFOLD", CORE_ORANGE);
                            let help_button = ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("?").size(16.0).strong(),
                                    )
                                    .min_size(egui::vec2(34.0, 30.0)),
                                )
                                .on_hover_text("Help, walkthroughs, and bundled answers (F1)");
                            if help_button.clicked() {
                                help.toggle_center();
                            }
                            let theme = ui
                                .button(if dark { "Light mode" } else { "Dark mode" })
                                .on_hover_text("Switch the interface theme");
                            if theme.clicked() {
                                ui.ctx().set_theme(if dark {
                                    egui::Theme::Light
                                } else {
                                    egui::Theme::Dark
                                });
                            }
                            (theme.rect, help_button.rect)
                        },
                    );
                    row.inner
                },
            );
            targets.set(TourTarget::Brand, brand_rect);
            targets.set(TourTarget::ThemeButton, theme_rect);
            targets.set(TourTarget::HelpButton, help_rect);
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
                muted(ui),
            );
            badge(ui, &format!("{} STEPS", contract.workflow.len()), muted(ui));
            badge(ui, &format!("{} INPUTS", contract.inputs.len()), muted(ui));
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
            ui.colored_label(muted(ui), format!("Assumes: {assumption}"));
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
                    muted(ui),
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
                    muted(ui),
                    format!("{}@{}", step.capability_type.id, step.capability_type.major),
                );
                for (parameter, value) in &step.parameters {
                    ui.colored_label(muted(ui), format!("{parameter} = {value}"));
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
                muted(ui),
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
        show_finding(
            ui,
            finding,
            &[("contract", CONTRACT_JSON), ("registry", REGISTRY_JSON)],
        );
        ui.add_space(7.0);
    }
}

/// The source line a finding points at, when its document's bytes are known:
/// `document:line:column`, the pointer, and the line with the value marked.
fn finding_location(finding: &CoreDiagnostic, sources: &[(&str, &[u8])]) -> Option<String> {
    let (_, bytes) = sources
        .iter()
        .find(|(document, _)| *document == finding.primary.document)?;
    let span = avila_core_compiler::locate(bytes, &finding.primary.pointer)?;
    let text = std::str::from_utf8(bytes).ok()?;
    let line_start = text[..span.start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[span.start..]
        .find('\n')
        .map_or(text.len(), |index| span.start + index);
    let line = text[line_start..line_end].trim_end();
    let mut rendered = format!(
        "{}:{}:{}  {}",
        finding.primary.document, span.line, span.column, finding.primary.pointer
    );
    if !span.is_exact(&finding.primary.pointer) {
        rendered.push_str(&format!(
            "  (not present; nearest is {})",
            span.resolved_pointer
        ));
    }
    rendered.push('\n');
    rendered.push_str(line.trim_start());
    Some(rendered)
}

fn show_finding(ui: &mut egui::Ui, finding: &CoreDiagnostic, sources: &[(&str, &[u8])]) {
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            class_badge(ui, finding.class);
            ui.label(
                egui::RichText::new(&finding.code)
                    .strong()
                    .color(CORE_ORANGE),
            );
            ui.colored_label(muted(ui), format!("owner: {}", finding.owner));
            if let Some(location) = finding_location(finding, sources) {
                ui.add(
                    egui::Label::new(egui::RichText::new(location).monospace().size(11.0)).wrap(),
                );
            }
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
                muted(ui),
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
            ui.colored_label(muted(ui), format!("Next action: {}", entry.next_action));
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
        ui.colored_label(muted(ui), &report.notice);
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
                        muted(ui),
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
                    muted(ui),
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
                muted(ui),
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
                badge(ui, "NOT CREATED", muted(ui));
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
    ui.label(egui::RichText::new(subtitle).color(muted(ui)));
    ui.add_space(10.0);
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    let dark = ui.visuals().dark_mode;
    egui::Frame::new()
        .fill(if dark {
            egui::Color32::from_rgb(29, 29, 29)
        } else {
            egui::Color32::WHITE
        })
        .stroke(egui::Stroke::new(
            1.0,
            if dark {
                egui::Color32::from_rgb(52, 52, 52)
            } else {
                egui::Color32::from_rgb(214, 214, 218)
            },
        ))
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
        FindingClass::Notice => ("NOTICE", muted(ui)),
    };
    badge(ui, label, color);
}

fn verdict_badge(ui: &mut egui::Ui, verdict: VerdictStatus) {
    let (label, color) = match verdict {
        VerdictStatus::Pass => ("PASS", egui::Color32::from_rgb(95, 197, 128)),
        VerdictStatus::Fail => ("FAIL", egui::Color32::from_rgb(232, 102, 102)),
        VerdictStatus::Inconclusive => ("INCONCLUSIVE", CORE_ORANGE),
        VerdictStatus::NotEvaluated => ("NOT EVALUATED", muted(ui)),
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
