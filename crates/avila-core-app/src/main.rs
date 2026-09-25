#![forbid(unsafe_code)]

//! Thin egui client over the semantic compiler and the case runner.
//!
//! The case workbench runs a composed case through the same runner the CLI
//! uses and renders its report stage by stage; the specimen view compiles the
//! embedded specimen through the same compiler and renders its findings. The
//! application performs no calculation and holds no scientific state of its
//! own.

mod case_browser;
mod case_view;
mod help;
mod history_view;
mod tools_view;

use avila_core_compiler::{
    CompilationStatus, CompileReport, ContractSource, CoreDiagnostic, FindingClass,
    RepairApplicability, RepairEdit, compile_documents, explain,
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
                "error: {error}\nusage: avila-core-app [--case DIR] [--source-root NAME=PATH]... [--capability NAME=PATH]... [--input NAME=PATH]... [--env KEY=VALUE]... [--workspace DIR] [--log FILE] [--attempt ID] [--parent-attempt ID] [--candidate-input NAME] [--revision ID] [--amendment ID] [--no-reuse] [--trust-root FILE] [--runner-key FILE] [--auto-run | --auto-plan] [--tools REPORT_OR_LOG] [--tool NAME] [--history CAMPAIGN_LOG] [--history-select ID|line:N] [--screenshot PNG] [--tab NAME]"
            );
            std::process::exit(2);
        }
    };
    let screenshot_state = setup
        .screenshot
        .as_ref()
        .map(|path| std::path::PathBuf::from(format!("{path}.state")));
    let options = eframe::NativeOptions {
        persistence_path: screenshot_state,
        persist_window: setup.screenshot.is_none(),
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
            let mut app = CoreApp::new(&creation_context.egui_ctx, setup);
            if let Some(storage) = creation_context.storage {
                app.browser.local =
                    eframe::get_value(storage, "core-local-cases-v1").unwrap_or_default();
            }
            if let Some(entry) = app.case.recent() {
                app.browser.local.remember(entry);
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Workspace {
    #[default]
    Overview,
    Question,
    Sources,
    Contract,
    Findings,
    Compiled,
    Results,
    Evidence,
}

impl Workspace {
    const ALL: [Self; 8] = [
        Self::Overview,
        Self::Question,
        Self::Sources,
        Self::Contract,
        Self::Findings,
        Self::Compiled,
        Self::Results,
        Self::Evidence,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Question => "Question",
            Self::Sources => "Sources",
            Self::Contract => "Contract",
            Self::Findings => "Findings",
            Self::Compiled => "Compiled snapshot",
            Self::Results => "Results",
            Self::Evidence => "Evidence",
        }
    }

    fn by_name(name: &str) -> Option<Self> {
        let key: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
        Self::ALL.iter().copied().find(|workspace| {
            workspace
                .label()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .eq_ignore_ascii_case(&key)
        })
    }
}

/// The authoring draft behind the specimen compiler: editable source
/// buffers, the paths they were opened from or saved to, and the latest
/// compiler check. Nothing writes a file until **Save** is clicked; the
/// embedded specimen is always available as the starting template.
struct Specimen {
    contract_text: String,
    registry_text: String,
    contract_path: Option<std::path::PathBuf>,
    registry_path: Option<std::path::PathBuf>,
    dirty: bool,
    check: Result<(ContractSource, CompileReport), String>,
    picker: case_browser::FilePicker,
    notice: Option<String>,
}

impl Specimen {
    fn embedded() -> Result<Self, String> {
        let mut draft = Self {
            contract_text: String::from_utf8_lossy(CONTRACT_JSON).into_owned(),
            registry_text: String::from_utf8_lossy(REGISTRY_JSON).into_owned(),
            contract_path: None,
            registry_path: None,
            dirty: false,
            check: Err("not yet compiled".to_string()),
            picker: case_browser::FilePicker::default(),
            notice: None,
        };
        draft.recheck()?;
        Ok(draft)
    }

    /// Compile the current buffers. The read workspaces render only what
    /// the check produced; a parse failure keeps the last sources and shows
    /// the compiler's message.
    fn recheck(&mut self) -> Result<(), String> {
        self.check =
            compile_documents(self.contract_text.as_bytes(), self.registry_text.as_bytes())
                .map_err(|error| error.to_string())
                .and_then(|compilation| {
                    serde_json::from_str::<ContractSource>(&self.contract_text)
                        .map(|contract| (contract, compilation.into_report()))
                        .map_err(|error| error.to_string())
                });
        self.check.as_ref().map(|_| ()).map_err(Clone::clone)
    }

    /// Open a contract file; its registry is `registry.json` beside it.
    fn open(&mut self, path: std::path::PathBuf) {
        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    self.contract_text = text;
                    self.contract_path = Some(path.clone());
                    let registry = path.with_file_name("registry.json");
                    match std::fs::read(&registry)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                    {
                        Some(text) => {
                            self.registry_text = text;
                            self.registry_path = Some(registry);
                        }
                        None => {
                            self.registry_path = None;
                            self.notice = Some(format!(
                                "No readable `registry.json` beside {}; the registry buffer kept its current contents.",
                                path.display()
                            ));
                        }
                    }
                    self.dirty = false;
                    if let Err(error) = self.recheck() {
                        self.notice = Some(format!("Opened but does not compile: {error}"));
                    }
                }
                Err(_) => self.notice = Some(format!("{} is not UTF-8 JSON text", path.display())),
            },
            Err(error) => self.notice = Some(format!("Could not read {}: {error}", path.display())),
        }
    }

    /// Write both buffers back to the paths they came from.
    fn save(&mut self) {
        let (Some(contract), Some(registry)) =
            (self.contract_path.clone(), self.registry_path.clone())
        else {
            self.notice = Some(
                "Nothing to save to: open a contract file first, or the draft stays in memory."
                    .into(),
            );
            return;
        };
        let written = std::fs::write(&contract, &self.contract_text)
            .and_then(|()| std::fs::write(&registry, &self.registry_text));
        match written {
            Ok(()) => {
                self.dirty = false;
                self.notice = Some(format!(
                    "Saved {} and {}",
                    contract.display(),
                    registry.display()
                ));
            }
            Err(error) => {
                self.notice = Some(format!("Save failed: {error}"));
            }
        }
    }

    fn reset(&mut self) {
        self.contract_text = String::from_utf8_lossy(CONTRACT_JSON).into_owned();
        self.registry_text = String::from_utf8_lossy(REGISTRY_JSON).into_owned();
        self.contract_path = None;
        self.registry_path = None;
        self.dirty = false;
        self.notice = Some("Reset to the embedded specimen.".to_string());
        let _ = self.recheck();
    }

    /// Apply one repair alternative — RFC 6902 edits over the buffer of the
    /// document the finding names — then recheck.
    fn apply_repairs(&mut self, document: &str, edits: &[RepairEdit]) {
        let buffer = match document {
            "contract" => &mut self.contract_text,
            "registry" => &mut self.registry_text,
            _ => {
                self.notice = Some(format!("Unknown document `{document}`"));
                return;
            }
        };
        match apply_edits(buffer, edits) {
            Ok(text) => {
                *buffer = text;
                self.dirty = true;
                let _ = self.recheck();
            }
            Err(error) => {
                self.notice = Some(format!("Repair did not apply: {error}"));
            }
        }
    }
}

/// Apply RFC 6902 `replace`, `add`, and `remove` edits to a JSON document
/// and return it pretty-printed.
fn apply_edits(text: &str, edits: &[RepairEdit]) -> Result<String, String> {
    let mut document: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("source is not valid JSON: {error}"))?;
    for edit in edits {
        apply_edit(&mut document, edit).map_err(|error| format!("{edit:?}: {error}"))?;
    }
    serde_json::to_string_pretty(&document).map_err(|error| error.to_string())
}

fn apply_edit(document: &mut serde_json::Value, edit: &RepairEdit) -> Result<(), String> {
    match edit {
        RepairEdit::Replace { path, value } => document
            .pointer_mut(path)
            .map(|slot| *slot = value.clone())
            .ok_or_else(|| format!("nothing exists at {path}")),
        RepairEdit::Add { path, value } => {
            let Some(split) = path.rfind('/') else {
                return Err(format!("{path} is not a JSON Pointer"));
            };
            let (parent, token) = (&path[..split], &path[split + 1..]);
            match document.pointer_mut(parent) {
                Some(serde_json::Value::Array(items)) => {
                    if token == "-" {
                        items.push(value.clone());
                    } else if let Ok(index) = token.parse::<usize>()
                        && index <= items.len()
                    {
                        items.insert(index, value.clone());
                    } else {
                        return Err(format!("{path} does not index the array"));
                    }
                    Ok(())
                }
                Some(serde_json::Value::Object(object)) => {
                    object.insert(token.replace("~1", "/").replace("~0", "~"), value.clone());
                    Ok(())
                }
                _ => Err(format!("nothing exists at {parent}")),
            }
        }
        RepairEdit::Remove { path } => {
            let Some(split) = path.rfind('/') else {
                return Err(format!("{path} is not a JSON Pointer"));
            };
            let (parent, token) = (&path[..split], &path[split + 1..]);
            match document.pointer_mut(parent) {
                Some(serde_json::Value::Array(items)) => match token.parse::<usize>() {
                    Ok(index) if index < items.len() => {
                        items.remove(index);
                        Ok(())
                    }
                    _ => Err(format!("{path} does not index the array")),
                },
                Some(serde_json::Value::Object(object)) => object
                    .remove(&token.replace("~1", "/").replace("~0", "~"))
                    .map(|_| ())
                    .ok_or_else(|| format!("nothing exists at {path}")),
                _ => Err(format!("nothing exists at {parent}")),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Case,
    Cases,
    History,
    Specimen,
    Tools,
}

struct CoreApp {
    mode: Mode,
    workspace: Workspace,
    logo: Option<egui::TextureHandle>,
    specimen: Result<Specimen, String>,
    case: case_view::CaseView,
    help: GuidedHelp,
    tools: tools_view::ToolsView,
    history: history_view::HistoryView,
    browser: case_browser::CaseBrowser,
}

impl CoreApp {
    fn new(context: &egui::Context, setup: case_view::CaseSetup) -> Self {
        if setup.light {
            context.set_theme(egui::Theme::Light);
        }
        let mut help = GuidedHelp::default();
        if setup.show_help {
            help.toggle_center();
        }
        if let Some(guide) = setup.tour.as_deref().and_then(help::GuideKind::by_name) {
            help.start_tour(guide);
        }
        let tools = tools_view::ToolsView::new(setup.tools_path.clone(), setup.tool.as_deref());
        let specimen_workspace = setup
            .specimen_workspace
            .as_deref()
            .and_then(Workspace::by_name);
        let mode = if setup.specimen_workspace.is_some() {
            Mode::Specimen
        } else if setup.history_path.is_some() {
            Mode::History
        } else if setup.tools_path.is_some() || setup.tool.is_some() {
            Mode::Tools
        } else if setup.case_dir.is_empty() {
            Mode::Cases
        } else {
            Mode::Case
        };
        Self {
            mode,
            workspace: specimen_workspace.unwrap_or_default(),
            logo: load_logo_texture(context).ok(),
            specimen: Specimen::embedded(),
            history: history_view::HistoryView::new(
                setup.history_path.clone(),
                setup.history_select.clone(),
            ),
            case: case_view::CaseView::new(setup),
            help,
            tools,
            browser: case_browser::CaseBrowser::new(Default::default()),
        }
    }

    fn current_view(&self) -> HelpView {
        match self.mode {
            Mode::Case => HelpView::Case(self.case.help_tab()),
            Mode::Cases => HelpView::Cases,
            Mode::History => HelpView::History,
            Mode::Specimen => HelpView::Specimen,
            Mode::Tools => HelpView::Tools,
        }
    }

    /// A tour step may ask for a mode and tab so its target is on screen.
    fn apply_requested_view(&mut self) {
        match self.help.requested_view() {
            Some(HelpView::Case(tab)) => {
                self.mode = Mode::Case;
                self.case.show_help_tab(tab);
                self.case.show_setup = true;
            }
            Some(HelpView::Cases) => self.mode = Mode::Cases,
            Some(HelpView::History) => self.mode = Mode::History,
            Some(HelpView::Specimen) => self.mode = Mode::Specimen,
            Some(HelpView::Tools) => self.mode = Mode::Tools,
            None => {}
        }
    }

    fn open_case(&mut self, path: &std::path::Path) {
        match case_browser::CaseInfo::read(path) {
            Ok(info) => {
                if let Some(previous) = self.case.recent()
                    && self
                        .browser
                        .local
                        .recent
                        .iter()
                        .any(|old| old.path == previous.path)
                {
                    self.browser.local.remember(previous);
                }
                let saved = self
                    .browser
                    .local
                    .recent
                    .iter()
                    .find(|entry| entry.path == info.path)
                    .cloned();
                match self.case.select_case(info, saved.as_ref()) {
                    Ok(()) => {
                        if let Some(entry) = self.case.recent() {
                            self.browser.local.remember(entry);
                        }
                        self.browser.error = None;
                        self.mode = Mode::Case;
                    }
                    Err(error) => {
                        self.browser.error = Some(error);
                        self.mode = Mode::Cases;
                    }
                }
            }
            Err(error) => {
                self.browser.error = Some(error);
                self.mode = Mode::Cases;
            }
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

        let specimen = match &mut self.specimen {
            Ok(specimen) => specimen,
            Err(error) => {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("Embedded specimen could not be read: {error}"),
                );
                return;
            }
        };
        if let Some(path) = specimen.picker.poll() {
            specimen.open(path);
        }

        let mut applies: Vec<(String, Vec<RepairEdit>)> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.workspace {
                Workspace::Question => show_question(ui, specimen),
                Workspace::Sources => show_sources(ui, specimen),
                Workspace::Evidence => show_evidence(ui),
                Workspace::Overview => match &specimen.check {
                    Ok((contract, report)) => show_overview(ui, contract, report),
                    Err(error) => show_check_error(ui, error),
                },
                Workspace::Contract => match &specimen.check {
                    Ok((contract, _)) => show_contract(ui, contract),
                    Err(error) => show_check_error(ui, error),
                },
                Workspace::Findings => match &specimen.check {
                    Ok((_, report)) => show_findings(
                        ui,
                        report,
                        specimen.contract_text.as_bytes(),
                        specimen.registry_text.as_bytes(),
                        &mut Some(&mut applies),
                    ),
                    Err(error) => show_check_error(ui, error),
                },
                Workspace::Compiled => match &specimen.check {
                    Ok((_, report)) => show_compiled(ui, report),
                    Err(error) => show_check_error(ui, error),
                },
                Workspace::Results => match &specimen.check {
                    Ok((contract, _)) => show_results(ui, contract),
                    Err(error) => show_check_error(ui, error),
                },
            });
        for (document, edits) in applies {
            specimen.apply_repairs(&document, &edits);
        }
    }
}

fn show_check_error(ui: &mut egui::Ui, error: &str) {
    card(ui, |ui| {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "The sources do not compile — the last check reported:",
        );
        ui.label(egui::RichText::new(error).monospace().size(11.0));
        ui.label("Edit the sources under Sources and press Check again.");
    });
}

impl eframe::App for CoreApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(entry) = self.case.recent() {
            // Forget remains effective until this case is deliberately opened again.
            if self
                .browser
                .local
                .recent
                .iter()
                .any(|old| old.path == entry.path)
            {
                self.browser.local.remember(entry);
            }
        }
        eframe::set_value(storage, "core-local-cases-v1", &self.browser.local);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input(|input| input.key_pressed(egui::Key::F1)) {
            self.help.toggle_center();
        }
        self.apply_requested_view();
        self.case.tick(ui.ctx());
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O))
            && !self.case.busy()
        {
            self.browser.picker.start(
                ui.ctx(),
                true,
                "Open a Core case folder (contains package.json)",
            );
        }
        if let Some(path) = self.browser.picker.poll() {
            self.open_case(&path);
        }
        if let Some(path) = self.browser.report_picker.poll() {
            self.tools =
                tools_view::ToolsView::new(Some(path.display().to_string()), Some("inspect"));
            self.mode = Mode::Tools;
        }
        if self.mode == Mode::Cases {
            let dropped = ui.input(|input| {
                input
                    .raw
                    .dropped_files
                    .first()
                    .map(|file| file.path().to_path_buf())
            });
            if let Some(path) = dropped {
                self.open_case(&path);
            }
        }
        let mut targets = TourTargets::default();

        // Paint the root background from the active theme; the window's clear
        // color does not follow a theme switch.
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, ui.visuals().panel_fill);
        show_header(ui, self.logo.as_ref(), &mut self.help, &mut targets);
        ui.add_space(8.0);
        let switch = ui.horizontal(|ui| {
            for (mode, label) in [
                (Mode::Cases, "Cases"),
                (Mode::Case, "Current case"),
                (Mode::History, "History"),
                (Mode::Specimen, "Specimen compiler"),
                (Mode::Tools, "Tools"),
            ] {
                if ui.selectable_label(self.mode == mode, label).clicked() {
                    if mode == Mode::Cases
                        && let Some(entry) = self.case.recent()
                        && self
                            .browser
                            .local
                            .recent
                            .iter()
                            .any(|old| old.path == entry.path)
                    {
                        self.browser.local.remember(entry);
                    }
                    self.mode = mode;
                }
            }
        });
        targets.set(TourTarget::ModeSwitch, switch.response.rect);
        ui.separator();
        match self.mode {
            Mode::Cases => {
                if let Some(path) = self.browser.ui(ui, self.case.busy()) {
                    self.open_case(&path);
                }
            }
            Mode::Case => self.case.ui(ui, &mut targets),
            Mode::History => {
                let case_inputs = self
                    .case
                    .info
                    .as_ref()
                    .map(|info| info.manifest.free_inputs.as_slice());
                if let Some(history_view::HistoryAction::UseAsParent {
                    attempt_id,
                    candidate_input,
                    log,
                }) = self.history.ui(ui, &self.case.setup.log, case_inputs)
                {
                    // Prefill only the parent relationship; the new attempt's
                    // ID and its candidate file stay the user's to supply.
                    self.case.setup.parent_attempt_id = attempt_id;
                    self.case.setup.candidate_input = candidate_input;
                    if self.case.setup.log.trim().is_empty() {
                        self.case.setup.log = log;
                    }
                    self.case.setup.show_setup = true;
                    self.mode = Mode::Case;
                }
            }
            Mode::Specimen => self.specimen_ui(ui, &mut targets),
            Mode::Tools => self.tools.ui(ui, self.case.report(), &self.case.setup.log),
        }
        let ready = match self.mode {
            Mode::Case => self.case.settled(),
            Mode::Tools => self.tools.settled(),
            Mode::History => self.history.settled(),
            Mode::Specimen | Mode::Cases => true,
        };
        self.case.drive_screenshot(ui.ctx(), ready);
        let view = self.current_view();
        self.help.show_center(ui.ctx(), view);
        if let Some(action) = self.help.take_action() {
            match action {
                help::HelpAction::Cases => self.mode = Mode::Cases,
                help::HelpAction::Setup => {
                    self.mode = if self.case.info.is_some() {
                        Mode::Case
                    } else {
                        Mode::Cases
                    };
                    self.case.show_setup = true;
                }
                help::HelpAction::Results => {
                    self.browser.report_picker.start(
                        ui.ctx(),
                        false,
                        "Open a saved Core run report",
                    );
                }
            }
        }
        self.help.show_tour(ui.ctx(), &targets);
    }
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
    dark.widgets.noninteractive.fg_stroke.color = egui::Color32::from_rgb(211, 214, 220);
    context.set_visuals_of(egui::Theme::Dark, dark);
    let mut light = egui::Visuals::light();
    light.panel_fill = egui::Color32::from_rgb(246, 246, 247);
    light.window_fill = egui::Color32::WHITE;
    light.extreme_bg_color = egui::Color32::WHITE;
    light.faint_bg_color = egui::Color32::from_rgb(236, 236, 238);
    light.selection.bg_fill = egui::Color32::from_rgb(255, 214, 160);
    light.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 62, 0));
    context.set_visuals_of(egui::Theme::Light, light);
    context.set_theme(egui::Theme::Dark);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        context.style_mut_of(theme, |style| {
            style.spacing.item_spacing = egui::vec2(10.0, 9.0);
            style.spacing.button_padding = egui::vec2(13.0, 7.0);
        });
    }
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
                                        .fit_to_exact_size(egui::vec2(135.0, 56.0))
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
                                egui::RichText::new("AVILA CORE")
                                    .size(10.0)
                                    .strong()
                                    .color(muted(ui)),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Resolve a technical question into reviewable evidence",
                                )
                                .size(16.0)
                                .strong(),
                            );
                            ui.label(
                                egui::RichText::new("Cases, evidence, and requirement verdicts")
                                    .color(muted(ui)),
                            );
                        });
                    });
                    brand.response.rect
                },
                |ui| {
                    let row = ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        badge(ui, "SCAFFOLD", CORE_ORANGE);
                        let help_button = ui
                            .add(
                                egui::Button::new(egui::RichText::new("Help").strong())
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
                    });
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

fn show_overview(ui: &mut egui::Ui, contract: &ContractSource, report: &CompileReport) {
    section_heading(
        ui,
        "What do you need to establish?",
        "Begin with a bounded question and its acceptance requirements, not a solver or a blank workflow.",
    );

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
                &format!(
                    "{} REQUIREMENT",
                    contract.requirements.len() + contract.categorical_requirements.len()
                ),
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
            "Question, requirements, inputs, tolerances, assumptions, and optional presentation policy.",
        );
        overview_stage(
            &mut columns[1],
            "2  Compile",
            "Core resolves typed dataflow, checks every rule, and reports every finding with its owner and repair.",
        );
        overview_stage(
            &mut columns[2],
            "3  Understand",
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
        for requirement in &contract.categorical_requirements {
            ui.separator();
            ui.label(egui::RichText::new(&requirement.requirement_id).strong());
            ui.label(&requirement.statement);
            let predicate = match &requirement.predicate {
                avila_core_compiler::CategoricalPredicate::Equals { value } => {
                    format!("Category equals {value}")
                }
                avila_core_compiler::CategoricalPredicate::InSet { values } => {
                    format!("Category in [{}]", values.join(", "))
                }
            };
            ui.colored_label(
                muted(ui),
                format!(
                    "{}  ·  purpose {}@{}",
                    predicate, requirement.purpose.id, requirement.purpose.major
                ),
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Question-first form authoring: the question layer of the contract (question,
// status, assumptions, execution policy, and every requirement) edited as
// structured fields over the same JSON buffer — the compiler remains the only
// check. The workflow, inputs, and bindings stay JSON-level in Sources.
// ---------------------------------------------------------------------------

/// Edit the string at `pointer` in `doc`; `true` when the value changed.
/// The field must already exist and hold a string.
fn form_string(ui: &mut egui::Ui, doc: &mut serde_json::Value, pointer: &str) -> bool {
    let mut text = doc
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let changed = ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(320.0))
        .changed();
    if changed && let Some(slot) = doc.pointer_mut(pointer) {
        *slot = serde_json::Value::String(text);
    }
    changed
}

/// Multiline string field.
fn form_text(ui: &mut egui::Ui, doc: &mut serde_json::Value, pointer: &str, rows: usize) -> bool {
    let mut text = doc
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let changed = ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_width(f32::INFINITY)
                .desired_rows(rows),
        )
        .changed();
    if changed && let Some(slot) = doc.pointer_mut(pointer) {
        *slot = serde_json::Value::String(text);
    }
    changed
}

/// A `u64` field edited as text; an unparseable buffer is left in place until
/// it parses (the compiler reports the type error either way).
fn form_u64(ui: &mut egui::Ui, doc: &mut serde_json::Value, pointer: &str) -> bool {
    let mut text = doc
        .pointer(pointer)
        .and_then(|value| value.as_u64())
        .map_or_else(String::new, |value| value.to_string());
    let changed = ui
        .add(egui::TextEdit::singleline(&mut text).desired_width(70.0))
        .changed();
    if changed
        && let Ok(number) = text.parse::<u64>()
        && let Some(slot) = doc.pointer_mut(pointer)
    {
        *slot = serde_json::Value::from(number);
    }
    changed
}

/// A closed-choice string field as a dropdown.
fn form_choice(
    ui: &mut egui::Ui,
    id: &str,
    doc: &mut serde_json::Value,
    pointer: &str,
    choices: &[&str],
) -> bool {
    let current = doc
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let mut selected = current.clone();
    egui::ComboBox::from_id_salt(id)
        .selected_text(if selected.is_empty() {
            "—"
        } else {
            &selected
        })
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(&mut selected, (*choice).to_string(), *choice);
            }
        });
    if selected != current {
        if let Some(slot) = doc.pointer_mut(pointer) {
            *slot = serde_json::Value::String(selected);
        }
        return true;
    }
    false
}

/// A boolean flag rendered as a checkbox.
fn form_bool(ui: &mut egui::Ui, doc: &mut serde_json::Value, pointer: &str, label: &str) -> bool {
    let mut value = doc
        .pointer(pointer)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let changed = ui.checkbox(&mut value, label).changed();
    if changed && let Some(slot) = doc.pointer_mut(pointer) {
        *slot = serde_json::Value::Bool(value);
    }
    changed
}

/// The registry's `kind_id`s and the unit symbols one kind admits.
fn registry_choices(registry: Option<&serde_json::Value>) -> (Vec<String>, Vec<String>) {
    let kinds: Vec<String> = registry
        .and_then(|doc| doc["kinds"].as_array())
        .map(|kinds| {
            kinds
                .iter()
                .filter_map(|kind| kind["kind_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let purposes: Vec<String> = registry
        .and_then(|doc| doc["purposes"].as_array())
        .map(|purposes| {
            purposes
                .iter()
                .filter_map(|purpose| purpose["purpose"]["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (kinds, purposes)
}

/// Role ids the registry declares, for contract-input dropdowns.
fn registry_roles(registry: Option<&serde_json::Value>) -> Vec<String> {
    registry
        .and_then(|doc| doc["roles"].as_array())
        .map(|roles| {
            roles
                .iter()
                .filter_map(|role| role["role"]["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A registry role entry for `role_id`, when the registry declares it.
fn registry_role<'a>(
    registry: Option<&'a serde_json::Value>,
    role_id: &str,
) -> Option<&'a serde_json::Value> {
    registry
        .and_then(|doc| doc["roles"].as_array())
        .and_then(|roles| {
            roles
                .iter()
                .find(|role| role["role"]["id"].as_str() == Some(role_id))
        })
}

/// Media types the registry admits for `role_id`.
fn role_media_types(registry: Option<&serde_json::Value>, role_id: &str) -> Vec<String> {
    registry_role(registry, role_id)
        .and_then(|role| role["accepted_media_types"].as_array())
        .map(|types| {
            types
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Claim-model names the registry admits for `role_id`.
fn role_claim_models(registry: Option<&serde_json::Value>, role_id: &str) -> Vec<String> {
    registry_role(registry, role_id)
        .and_then(|role| role["permitted_claim_models"].as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model["model"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Capability-type ids the registry declares, for workflow-step dropdowns.
fn registry_capability_types(registry: Option<&serde_json::Value>) -> Vec<String> {
    registry
        .and_then(|doc| doc["capability_types"].as_array())
        .map(|types| {
            types
                .iter()
                .filter_map(|ty| ty["capability_type"]["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A registry capability-type entry for `type_id`, when declared.
fn registry_capability_type<'a>(
    registry: Option<&'a serde_json::Value>,
    type_id: &str,
) -> Option<&'a serde_json::Value> {
    registry
        .and_then(|doc| doc["capability_types"].as_array())
        .and_then(|types| {
            types
                .iter()
                .find(|ty| ty["capability_type"]["id"].as_str() == Some(type_id))
        })
}

/// Unit symbols the registry admits for `kind_id`.
fn registry_units(registry: Option<&serde_json::Value>, kind_id: &str) -> Vec<String> {
    registry
        .and_then(|doc| doc["kinds"].as_array())
        .and_then(|kinds| {
            kinds
                .iter()
                .find(|kind| kind["kind_id"].as_str() == Some(kind_id))
        })
        .and_then(|kind| kind["units"].as_array())
        .map(|units| {
            units
                .iter()
                .filter_map(|unit| unit["symbol"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Workflow step ids declared on the contract.
fn workflow_steps(contract: &serde_json::Value) -> Vec<String> {
    contract["workflow"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["step_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Output slots the registry declares for the capability type `step` uses.
fn step_output_slots(
    contract: &serde_json::Value,
    registry: Option<&serde_json::Value>,
    step_id: &str,
) -> Vec<String> {
    let Some(capability) = contract["workflow"]
        .as_array()
        .and_then(|steps| {
            steps
                .iter()
                .find(|step| step["step_id"].as_str() == Some(step_id))
        })
        .and_then(|step| step["capability_type"]["id"].as_str().map(str::to_string))
    else {
        return Vec::new();
    };
    registry
        .and_then(|doc| doc["capability_types"].as_array())
        .and_then(|types| {
            types
                .iter()
                .find(|cap| cap["capability_type"]["id"].as_str() == Some(capability.as_str()))
        })
        .and_then(|cap| cap["outputs"].as_array())
        .map(|outputs| {
            outputs
                .iter()
                .filter_map(|output| output["slot_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Contract input ids.
fn contract_inputs(contract: &serde_json::Value) -> Vec<String> {
    contract["inputs"]
        .as_array()
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(|input| input["input_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn quantity_form(
    ui: &mut egui::Ui,
    id_prefix: &str,
    doc: &mut serde_json::Value,
    pointer: &str,
    kinds: &[String],
    registry: Option<&serde_json::Value>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("kind");
        let kind_choices: Vec<&str> = kinds.iter().map(String::as_str).collect();
        changed |= form_choice(
            ui,
            &format!("{id_prefix}-kind"),
            doc,
            &format!("{pointer}/kind"),
            &kind_choices,
        );
        let kind = doc
            .pointer(&format!("{pointer}/kind"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        ui.label("value");
        changed |= form_string(ui, doc, &format!("{pointer}/value"));
        ui.label("unit");
        let unit_choices: Vec<String> = registry_units(registry, &kind);
        let unit_refs: Vec<&str> = unit_choices.iter().map(String::as_str).collect();
        changed |= form_choice(
            ui,
            &format!("{id_prefix}-unit"),
            doc,
            &format!("{pointer}/unit"),
            &unit_refs,
        );
    });
    changed
}

/// One requirement's metric: a source-kind dropdown plus the fields the kind
/// names — a contract input or a workflow step's declared output slot.
fn metric_form(
    ui: &mut egui::Ui,
    id_prefix: &str,
    requirement: &mut serde_json::Value,
    contract: &serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("metric");
        let kinds = ["none", "contract_input", "step_output"];
        let current = requirement["metric"]["source"]
            .as_str()
            .unwrap_or("none")
            .to_string();
        let mut selected = current.clone();
        egui::ComboBox::from_id_salt(format!("{id_prefix}-metric-kind"))
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for kind in kinds {
                    ui.selectable_value(&mut selected, kind.to_string(), kind);
                }
            });
        if selected != current {
            requirement["metric"] = match selected.as_str() {
                "contract_input" => serde_json::json!({
                    "source": "contract_input",
                    "input_id": contract_inputs(contract).first().cloned().unwrap_or_default(),
                }),
                "step_output" => serde_json::json!({
                    "source": "step_output",
                    "step_id": workflow_steps(contract).first().cloned().unwrap_or_default(),
                    "output_slot": "",
                }),
                _ => serde_json::Value::Null,
            };
            if selected == "none" {
                requirement.as_object_mut().unwrap().remove("metric");
            }
            changed = true;
        }
        match requirement["metric"]["source"].as_str() {
            Some("contract_input") => {
                let choices: Vec<String> = contract_inputs(contract);
                let refs: Vec<&str> = choices.iter().map(String::as_str).collect();
                ui.label("input");
                changed |= form_choice(
                    ui,
                    &format!("{id_prefix}-metric-input"),
                    requirement,
                    "/metric/input_id",
                    &refs,
                );
            }
            Some("step_output") => {
                let steps = workflow_steps(contract);
                let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
                ui.label("step");
                changed |= form_choice(
                    ui,
                    &format!("{id_prefix}-metric-step"),
                    requirement,
                    "/metric/step_id",
                    &refs,
                );
                let step_id = requirement["metric"]["step_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let slots = step_output_slots(contract, registry, &step_id);
                ui.label("output");
                if slots.is_empty() {
                    changed |= form_string(ui, requirement, "/metric/output_slot");
                } else {
                    let slot_refs: Vec<&str> = slots.iter().map(String::as_str).collect();
                    changed |= form_choice(
                        ui,
                        &format!("{id_prefix}-metric-slot"),
                        requirement,
                        "/metric/output_slot",
                        &slot_refs,
                    );
                }
            }
            _ => {}
        }
    });
    changed
}

/// One requirement card — returns true when the contract was edited; the
/// caller writes the buffer back and marks it dirty.
fn requirement_form(
    ui: &mut egui::Ui,
    index: usize,
    contract: &mut serde_json::Value,
    kinds: &[String],
    purposes: &[String],
    registry: Option<&serde_json::Value>,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let prefix = format!("/requirements/{index}");
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("Requirement {}", index + 1)).strong());
            ui.label("id");
            changed |= form_string(ui, contract, &format!("{prefix}/requirement_id"));
            if ui.small_button("remove").clicked() {
                remove = true;
            }
        });
        ui.label("statement — what the requirement claims, in words");
        changed |= form_text(ui, contract, &format!("{prefix}/statement"), 2);
        ui.horizontal_wrapped(|ui| {
            ui.label("purpose");
            let purpose_refs: Vec<&str> = purposes.iter().map(String::as_str).collect();
            changed |= form_choice(
                ui,
                &format!("req{index}-purpose"),
                contract,
                &format!("{prefix}/purpose/id"),
                &purpose_refs,
            );
            ui.label("major");
            changed |= form_u64(ui, contract, &format!("{prefix}/purpose/major"));
        });
        {
            let contract_snapshot = contract.clone();
            let mut requirement = contract
                .pointer_mut(&prefix)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            changed |= metric_form(
                ui,
                &format!("req{index}"),
                &mut requirement,
                &contract_snapshot,
                registry,
            );
            if let Some(slot) = contract.pointer_mut(&prefix) {
                *slot = requirement;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("comparison");
            changed |= form_choice(
                ui,
                &format!("req{index}-comparison"),
                contract,
                &format!("{prefix}/comparison"),
                &[
                    "less_than",
                    "less_than_or_equal",
                    "greater_than",
                    "greater_than_or_equal",
                    "equal",
                ],
            );
            ui.label("basis");
            changed |= form_choice(
                ui,
                &format!("req{index}-basis"),
                contract,
                &format!("{prefix}/basis/kind"),
                &["bounded", "enclosure", "nominal"],
            );
        });
        ui.label("limit — the quantity the metric is compared against");
        changed |= quantity_form(
            ui,
            &format!("req{index}-limit"),
            contract,
            &format!("{prefix}/limit"),
            kinds,
            registry,
        );
        if contract
            .pointer(&format!("{prefix}/comparison"))
            .and_then(|value| value.as_str())
            == Some("equal")
        {
            if contract.pointer(&format!("{prefix}/tolerance")).is_none()
                && let Some(requirement) = contract.pointer_mut(&prefix)
            {
                requirement.as_object_mut().unwrap().insert(
                    "tolerance".to_string(),
                    serde_json::json!({"kind": "", "value": "0", "unit": ""}),
                );
            }
            ui.label("tolerance — an equal comparison is evaluable only with one");
            changed |= quantity_form(
                ui,
                &format!("req{index}-tolerance"),
                contract,
                &format!("{prefix}/tolerance"),
                kinds,
                registry,
            );
        } else if contract.pointer(&format!("{prefix}/tolerance")).is_some() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    muted(ui),
                    "a tolerance is meaningful only for an equal comparison",
                );
                if ui.small_button("remove tolerance").clicked() {
                    if let Some(requirement) = contract.pointer_mut(&prefix) {
                        requirement.as_object_mut().unwrap().remove("tolerance");
                    }
                    changed = true;
                }
            });
        }
    });
    (changed, remove)
}

/// One contract input card — id, registry role, the media types and claim
/// models that role admits. Returns (edited, remove).
fn input_form(
    ui: &mut egui::Ui,
    index: usize,
    contract: &mut serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let prefix = format!("/inputs/{index}");
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("Input {}", index + 1)).strong());
            ui.label("id");
            changed |= form_string(ui, contract, &format!("{prefix}/input_id"));
            if ui.small_button("remove").clicked() {
                remove = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("role");
            let role_id = contract
                .pointer(&format!("{prefix}/role/id"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let roles = registry_roles(registry);
            let mut selected = role_id.clone();
            egui::ComboBox::from_id_salt(format!("input{index}-role"))
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for role in &roles {
                        ui.selectable_value(&mut selected, role.clone(), role);
                    }
                });
            if selected != role_id
                && let Some(input) = contract.pointer_mut(&prefix)
            {
                let major = registry_role(registry, &selected)
                    .and_then(|role| role["role"]["major"].as_u64())
                    .unwrap_or(1);
                input["role"] = serde_json::json!({"id": selected, "major": major});
                // A changed role admits different media types and models;
                // reset them to the role's first admitted value so the
                // authored input stays admissible while it is edited.
                let media = role_media_types(registry, &selected);
                if let Some(first) = media.first() {
                    input["media_type"] = serde_json::Value::String(first.clone());
                }
                let models = role_claim_models(registry, &selected);
                if let Some(first) = models.first() {
                    input["claim_model"] = serde_json::json!({"model": first});
                }
                changed = true;
            }
            let media = role_media_types(registry, &role_id);
            if media.is_empty() {
                ui.label("media type");
                changed |= form_string(ui, contract, &format!("{prefix}/media_type"));
            } else {
                ui.label("media type");
                let refs: Vec<&str> = media.iter().map(String::as_str).collect();
                changed |= form_choice(
                    ui,
                    &format!("input{index}-media"),
                    contract,
                    &format!("{prefix}/media_type"),
                    &refs,
                );
            }
            let models = role_claim_models(registry, &role_id);
            if models.is_empty() {
                ui.label("claim model");
                changed |= form_string(ui, contract, &format!("{prefix}/claim_model/model"));
            } else {
                ui.label("claim model");
                let refs: Vec<&str> = models.iter().map(String::as_str).collect();
                changed |= form_choice(
                    ui,
                    &format!("input{index}-model"),
                    contract,
                    &format!("{prefix}/claim_model/model"),
                    &refs,
                );
            }
            if contract
                .pointer(&format!("{prefix}/claim_model/model"))
                .and_then(|value| value.as_str())
                == Some("worst_case")
            {
                ui.label("side");
                changed |= form_choice(
                    ui,
                    &format!("input{index}-side"),
                    contract,
                    &format!("{prefix}/claim_model/side"),
                    &["lower", "upper"],
                );
            }
        });
    });
    (changed, remove)
}

/// One categorical requirement card — the same id/statement/purpose/metric
/// surface as a numeric requirement, plus an `equals`/`in_set` predicate.
fn categorical_form(
    ui: &mut egui::Ui,
    index: usize,
    contract: &mut serde_json::Value,
    purposes: &[String],
    registry: Option<&serde_json::Value>,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let prefix = format!("/categorical_requirements/{index}");
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("Categorical requirement {}", index + 1)).strong(),
            );
            ui.label("id");
            changed |= form_string(ui, contract, &format!("{prefix}/requirement_id"));
            if ui.small_button("remove").clicked() {
                remove = true;
            }
        });
        ui.label("statement — what the requirement claims, in words");
        changed |= form_text(ui, contract, &format!("{prefix}/statement"), 2);
        ui.horizontal_wrapped(|ui| {
            ui.label("purpose");
            let purpose_refs: Vec<&str> = purposes.iter().map(String::as_str).collect();
            changed |= form_choice(
                ui,
                &format!("cat{index}-purpose"),
                contract,
                &format!("{prefix}/purpose/id"),
                &purpose_refs,
            );
            ui.label("major");
            changed |= form_u64(ui, contract, &format!("{prefix}/purpose/major"));
        });
        {
            let contract_snapshot = contract.clone();
            let mut requirement = contract
                .pointer_mut(&prefix)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            changed |= metric_form(
                ui,
                &format!("cat{index}"),
                &mut requirement,
                &contract_snapshot,
                registry,
            );
            if let Some(slot) = contract.pointer_mut(&prefix) {
                *slot = requirement;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("predicate");
            let current = contract
                .pointer(&format!("{prefix}/predicate/operator"))
                .and_then(|value| value.as_str())
                .unwrap_or("equals")
                .to_string();
            let mut selected = current.clone();
            egui::ComboBox::from_id_salt(format!("cat{index}-operator"))
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for operator in ["equals", "in_set"] {
                        ui.selectable_value(&mut selected, operator.to_string(), operator);
                    }
                });
            if selected != current
                && let Some(requirement) = contract.pointer_mut(&prefix)
            {
                requirement["predicate"] = if selected == "in_set" {
                    serde_json::json!({"operator": "in_set", "values": []})
                } else {
                    serde_json::json!({"operator": "equals", "value": ""})
                };
                changed = true;
            }
            match contract
                .pointer(&format!("{prefix}/predicate/operator"))
                .and_then(|value| value.as_str())
            {
                Some("in_set") => {
                    ui.label("values (comma-separated)");
                    let joined = contract
                        .pointer(&format!("{prefix}/predicate/values"))
                        .and_then(|value| value.as_array())
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|value| value.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let mut edited = joined.clone();
                    if ui.text_edit_singleline(&mut edited).changed() {
                        let values: Vec<serde_json::Value> = edited
                            .split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(|value| serde_json::Value::String(value.to_string()))
                            .collect();
                        if let Some(requirement) = contract.pointer_mut(&prefix) {
                            requirement["predicate"]["values"] = serde_json::Value::Array(values);
                        }
                        changed = true;
                    }
                }
                _ => {
                    ui.label("value");
                    changed |= form_string(ui, contract, &format!("{prefix}/predicate/value"));
                }
            }
        });
    });
    (changed, remove)
}

/// A `source` reference editor — contract input or another step's output
/// slot — writing the SourceRef shape at `pointer`.
fn source_ref_form(
    ui: &mut egui::Ui,
    id_prefix: &str,
    step: &mut serde_json::Value,
    pointer: &str,
    contract: &serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        let kinds = ["contract_input", "step_output"];
        let current = step
            .pointer(&format!("{pointer}/source"))
            .and_then(|value| value.as_str())
            .unwrap_or("contract_input")
            .to_string();
        let mut selected = current.clone();
        egui::ComboBox::from_id_salt(format!("{id_prefix}-src-kind"))
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for kind in kinds {
                    ui.selectable_value(&mut selected, kind.to_string(), kind);
                }
            });
        if selected != current
            && let Some(source) = step.pointer_mut(pointer)
        {
            *source = if selected == "step_output" {
                serde_json::json!({
                    "source": "step_output",
                    "step_id": workflow_steps(contract).first().cloned().unwrap_or_default(),
                    "output_slot": "",
                })
            } else {
                serde_json::json!({
                    "source": "contract_input",
                    "input_id": contract_inputs(contract).first().cloned().unwrap_or_default(),
                })
            };
            changed = true;
        }
        match step
            .pointer(&format!("{pointer}/source"))
            .and_then(|value| value.as_str())
        {
            Some("step_output") => {
                let steps: Vec<String> = workflow_steps(contract)
                    .into_iter()
                    .filter(|id| {
                        step.pointer("/step_id").and_then(|v| v.as_str()) != Some(id.as_str())
                    })
                    .collect();
                let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
                changed |= form_choice(
                    ui,
                    &format!("{id_prefix}-src-step"),
                    step,
                    &format!("{pointer}/step_id"),
                    &refs,
                );
                let from_step = step
                    .pointer(&format!("{pointer}/step_id"))
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                let slots = step_output_slots(contract, registry, &from_step);
                if slots.is_empty() {
                    changed |= form_string(ui, step, &format!("{pointer}/output_slot"));
                } else {
                    let refs: Vec<&str> = slots.iter().map(String::as_str).collect();
                    changed |= form_choice(
                        ui,
                        &format!("{id_prefix}-src-slot"),
                        step,
                        &format!("{pointer}/output_slot"),
                        &refs,
                    );
                }
            }
            _ => {
                let inputs = contract_inputs(contract);
                let refs: Vec<&str> = inputs.iter().map(String::as_str).collect();
                changed |= form_choice(
                    ui,
                    &format!("{id_prefix}-src-input"),
                    step,
                    &format!("{pointer}/input_id"),
                    &refs,
                );
            }
        }
    });
    changed
}

/// One parameter editor — a `not_defined` toggle plus the typed field the
/// registry's `value_type` declares.
fn parameter_form(
    ui: &mut egui::Ui,
    id_prefix: &str,
    step: &mut serde_json::Value,
    parameter: &serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> bool {
    let mut changed = false;
    let param_id = parameter["parameter_id"].as_str().unwrap_or_default();
    let required = parameter["required"].as_bool().unwrap_or(false);
    let pointer = format!("/parameters/{param_id}");
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "{param_id}{}",
            if required { " *" } else { "" }
        ));
        let undefined = step.pointer(&pointer).is_none_or(|value| {
            value.as_str() == Some("not_defined") || value.is_null()
        });
        let mut undef = undefined;
        if ui
            .checkbox(&mut undef, "not defined")
            .on_hover_text("A draft placeholder; only `draft` contracts may leave a required parameter not defined.")
            .changed()
        {
            step["parameters"][param_id] = if undef {
                serde_json::Value::String("not_defined".to_string())
            } else {
                match parameter["value_type"]["type"].as_str() {
                    Some("boolean") => serde_json::json!(false),
                    Some("integer") | Some("exact_number") => {
                        serde_json::Value::String("0".to_string())
                    }
                    Some("quantity") => serde_json::json!({"kind": "", "value": "0", "unit": ""}),
                    _ => serde_json::Value::String(String::new()),
                }
            };
            changed = true;
        }
        if !undefined {
            match parameter["value_type"]["type"].as_str() {
                Some("boolean") => {
                    let mut value = step
                        .pointer(&pointer)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if ui.checkbox(&mut value, "").changed() {
                        step["parameters"][param_id] = serde_json::json!(value);
                        changed = true;
                    }
                }
                Some("text") => {
                    let allowed = parameter["value_type"]["allowed_values"]
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if allowed.is_empty() {
                        changed |= form_string(ui, step, &pointer);
                    } else {
                        let refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
                        changed |= form_choice(
                            ui,
                            &format!("{id_prefix}-{param_id}"),
                            step,
                            &pointer,
                            &refs,
                        );
                    }
                }
                Some("quantity") => {
                    let kind = parameter["value_type"]["kind"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    changed |= quantity_form(
                        ui,
                        &format!("{id_prefix}-{param_id}"),
                        step,
                        &pointer,
                        &[kind],
                        registry,
                    );
                }
                _ => {
                    changed |= form_string(ui, step, &pointer);
                }
            }
        }
    });
    changed
}

/// One declared material factor bound on the step — same typed-value
/// machinery as parameters, keyed under `reproducibility/material_factors`.
/// An unset factor is left absent, never nulled.
fn factor_form(
    ui: &mut egui::Ui,
    id_prefix: &str,
    step: &mut serde_json::Value,
    factor: &serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> bool {
    let mut changed = false;
    let factor_id = factor["factor_id"].as_str().unwrap_or_default();
    let pointer = format!("/reproducibility/material_factors/{factor_id}");
    ui.horizontal_wrapped(|ui| {
        ui.label(factor_id);
        let set = !step.pointer(&pointer).is_none_or(serde_json::Value::is_null);
        let mut on = set;
        if ui
            .checkbox(&mut on, "set")
            .on_hover_text("Material factors are recorded execution context; an unset factor is simply absent.")
            .changed()
        {
            if step["reproducibility"]["material_factors"].is_null() {
                step["reproducibility"]["material_factors"] = serde_json::json!({});
            }
            if on {
                step["reproducibility"]["material_factors"][factor_id] =
                    match factor["value_type"]["type"].as_str() {
                        Some("boolean") => serde_json::json!(false),
                        Some("integer") | Some("exact_number") => {
                            serde_json::Value::String("0".to_string())
                        }
                        Some("quantity") => {
                            serde_json::json!({"kind": "", "value": "0", "unit": ""})
                        }
                        _ => serde_json::Value::String(String::new()),
                    };
            } else if let Some(map) =
                step["reproducibility"]["material_factors"].as_object_mut()
            {
                map.remove(factor_id);
            }
            changed = true;
        }
        if set {
            match factor["value_type"]["type"].as_str() {
                Some("boolean") => {
                    let mut value = step
                        .pointer(&pointer)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if ui.checkbox(&mut value, "").changed() {
                        step["reproducibility"]["material_factors"][factor_id] =
                            serde_json::json!(value);
                        changed = true;
                    }
                }
                Some("text") => {
                    let allowed = factor["value_type"]["allowed_values"]
                        .as_array()
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if allowed.is_empty() {
                        changed |= form_string(ui, step, &pointer);
                    } else {
                        let refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
                        changed |= form_choice(
                            ui,
                            &format!("{id_prefix}-{factor_id}"),
                            step,
                            &pointer,
                            &refs,
                        );
                    }
                }
                Some("quantity") => {
                    let kind = factor["value_type"]["kind"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    changed |= quantity_form(
                        ui,
                        &format!("{id_prefix}-{factor_id}"),
                        step,
                        &pointer,
                        &[kind],
                        registry,
                    );
                }
                _ => {
                    changed |= form_string(ui, step, &pointer);
                }
            }
        }
    });
    changed
}

/// The step's optional review binding — eligibility policy, independence
/// mode, and the instructions the reviewer is shown. A review never gates
/// a technical verdict; it gates presentation.
fn review_form(ui: &mut egui::Ui, id_prefix: &str, step: &mut serde_json::Value) -> bool {
    let mut changed = false;
    let bound = step["review"].is_object();
    let mut on = bound;
    if ui
        .checkbox(&mut on, "requires review")
        .on_hover_text("An optional presentation gate: a reviewer records a disposition over the realised dossier. Review can never change a technical verdict.")
        .changed()
    {
        step["review"] = if on {
            serde_json::json!({
                "reviewer_eligibility_policy": {
                    "policy_id": "",
                    "revision": 1,
                    "sha256": "sha256:",
                },
                "independence": {"mode": "none"},
                "instructions": [],
            })
        } else {
            serde_json::Value::Null
        };
        changed = true;
    }
    if !bound {
        return changed;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("policy id");
        changed |= form_string(ui, step, "/review/reviewer_eligibility_policy/policy_id");
        ui.label("revision");
        changed |= form_string(ui, step, "/review/reviewer_eligibility_policy/revision");
    });
    changed |= form_string(ui, step, "/review/reviewer_eligibility_policy/sha256");
    ui.horizontal_wrapped(|ui| {
        ui.label("independence");
        let mode = step["review"]["independence"]["mode"]
            .as_str()
            .unwrap_or("none")
            .to_string();
        let mut selected = mode.clone();
        egui::ComboBox::from_id_salt(format!("{id_prefix}-independence"))
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                for option in ["none", "constraints"] {
                    ui.selectable_value(&mut selected, option.to_string(), option);
                }
            });
        if selected != mode {
            step["review"]["independence"] = if selected == "constraints" {
                serde_json::json!({"mode": "constraints", "requirements": []})
            } else {
                serde_json::json!({"mode": "none"})
            };
            changed = true;
        }
        if selected == "constraints" {
            ui.label("each constraint separates the reviewer from a party at a level");
            let mut requirements = step["review"]["independence"]["requirements"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut index = 0;
            while index < requirements.len() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("constraint {}", index + 1));
                    for (key, options) in [
                        (
                            "separated_from",
                            [
                                "requester",
                                "method_owner",
                                "capability_provider",
                                "executor",
                            ]
                            .as_slice(),
                        ),
                        (
                            "minimum_separation",
                            ["different_person", "different_organization"].as_slice(),
                        ),
                    ] {
                        let current = requirements[index][key]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let mut picked = current.clone();
                        egui::ComboBox::from_id_salt(format!("{id_prefix}-req{index}-{key}"))
                            .selected_text(&picked)
                            .show_ui(ui, |ui| {
                                for option in options {
                                    ui.selectable_value(&mut picked, option.to_string(), *option);
                                }
                            });
                        if picked != current {
                            requirements[index][key] = serde_json::json!(picked);
                            changed = true;
                        }
                    }
                    if ui.small_button("remove").clicked() {
                        requirements.remove(index);
                        changed = true;
                    }
                });
                index += 1;
            }
            if ui.small_button("add constraint").clicked() {
                requirements.push(serde_json::json!({
                    "separated_from": "requester",
                    "minimum_separation": "different_person",
                }));
                changed = true;
            }
            if step["review"]["independence"]["requirements"] != serde_json::json!(requirements) {
                step["review"]["independence"]["requirements"] = serde_json::json!(requirements);
            }
        }
    });
    ui.label("instructions — one line each; the compiler binds the text but does not judge whether the reviewer followed it");
    let mut instructions = step["review"]["instructions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut index = 0;
    while index < instructions.len() {
        ui.horizontal_wrapped(|ui| {
            changed |= form_string(ui, &mut instructions[index], "");
            if ui.small_button("remove").clicked() {
                instructions.remove(index);
                changed = true;
            }
        });
        index += 1;
    }
    if ui.small_button("add instruction").clicked() {
        instructions.push(serde_json::Value::String(String::new()));
        changed = true;
    }
    if step["review"]["instructions"] != serde_json::json!(instructions) {
        step["review"]["instructions"] = serde_json::json!(instructions);
    }
    changed
}

/// One workflow-step card — capability type, its declared input slots bound
/// to sources, its declared parameters, and the reproducibility seed.
fn step_form(
    ui: &mut egui::Ui,
    index: usize,
    contract: &mut serde_json::Value,
    registry: Option<&serde_json::Value>,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let prefix = format!("/workflow/{index}");
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("Step {}", index + 1)).strong());
            ui.label("id");
            changed |= form_string(ui, contract, &format!("{prefix}/step_id"));
            if ui.small_button("remove").clicked() {
                remove = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("capability type");
            let type_id = contract
                .pointer(&format!("{prefix}/capability_type/id"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let types = registry_capability_types(registry);
            let mut selected = type_id.clone();
            egui::ComboBox::from_id_salt(format!("step{index}-type"))
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for ty in &types {
                        ui.selectable_value(&mut selected, ty.clone(), ty);
                    }
                });
            if selected != type_id
                && let Some(step) = contract.pointer_mut(&prefix)
            {
                let major = registry_capability_type(registry, &selected)
                    .and_then(|ty| ty["capability_type"]["major"].as_u64())
                    .unwrap_or(1);
                step["capability_type"] = serde_json::json!({"id": selected, "major": major});
                changed = true;
            }
        });
        let type_id = contract
            .pointer(&format!("{prefix}/capability_type/id"))
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let declared = registry_capability_type(registry, &type_id).cloned();
        if let Some(declaration) = declared {
            let slots: Vec<String> = declaration["inputs"]
                .as_array()
                .map(|inputs| {
                    inputs
                        .iter()
                        .filter_map(|input| input["slot_id"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !slots.is_empty() {
                ui.label("inputs — each declared slot bound to a source");
            }
            for slot in slots {
                let contract_snapshot = contract.clone();
                let mut step = contract
                    .pointer_mut(&prefix)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let binding_index = step["bindings"].as_array().and_then(|bindings| {
                    bindings
                        .iter()
                        .position(|binding| binding["input_slot"].as_str() == Some(slot.as_str()))
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(slot.clone());
                    match binding_index {
                        Some(binding) => {
                            changed |= source_ref_form(
                                ui,
                                &format!("step{index}-{slot}"),
                                &mut step,
                                &format!("/bindings/{binding}/source"),
                                &contract_snapshot,
                                registry,
                            );
                            if ui.small_button("unbind").clicked() {
                                step["bindings"].as_array_mut().unwrap().remove(binding);
                                changed = true;
                            }
                        }
                        None => {
                            ui.colored_label(muted(ui), "unbound");
                            if ui.small_button("bind").clicked() {
                                if !step["bindings"].is_array() {
                                    step["bindings"] = serde_json::json!([]);
                                }
                                step["bindings"]
                                    .as_array_mut()
                                    .unwrap()
                                    .push(serde_json::json!({
                                        "input_slot": slot,
                                        "source": {
                                            "source": "contract_input",
                                            "input_id": contract_inputs(&contract_snapshot)
                                                .first()
                                                .cloned()
                                                .unwrap_or_default(),
                                        },
                                    }));
                                changed = true;
                            }
                        }
                    }
                });
                if let Some(slot_target) = contract.pointer_mut(&prefix) {
                    *slot_target = step;
                }
            }
            let parameters: Vec<serde_json::Value> = declaration["parameters"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !parameters.is_empty() {
                ui.label("parameters");
            }
            for parameter in parameters {
                let mut step = contract
                    .pointer_mut(&prefix)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                changed |=
                    parameter_form(ui, &format!("step{index}"), &mut step, &parameter, registry);
                if let Some(slot_target) = contract.pointer_mut(&prefix) {
                    *slot_target = step;
                }
            }
            let factors: Vec<serde_json::Value> =
                declaration["reproducibility"]["material_factors"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
            if !factors.is_empty() {
                ui.label("material factors — execution context the run records alongside the seed");
            }
            for factor in factors {
                let mut step = contract
                    .pointer_mut(&prefix)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                changed |= factor_form(ui, &format!("step{index}"), &mut step, &factor, registry);
                if let Some(slot_target) = contract.pointer_mut(&prefix) {
                    *slot_target = step;
                }
            }
        }
        {
            let mut step = contract
                .pointer_mut(&prefix)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            changed |= review_form(ui, &format!("step{index}"), &mut step);
            if let Some(slot_target) = contract.pointer_mut(&prefix) {
                *slot_target = step;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("seed");
            let pointer = format!("{prefix}/reproducibility/seed");
            if contract.pointer(&pointer).is_none()
                && let Some(step) = contract.pointer_mut(&prefix)
            {
                step["reproducibility"] = serde_json::json!({});
            }
            changed |= form_string(ui, contract, &pointer);
        });
    });
    (changed, remove)
}

fn show_question(ui: &mut egui::Ui, specimen: &mut Specimen) {
    section_heading(
        ui,
        "Question",
        "The contract as form fields — the bounded question, its policy, inputs, workflow steps, and every requirement. Edits write the contract buffer directly; Check still runs the authoritative compiler and nothing is saved until Save.",
    );
    let mut contract: serde_json::Value = match serde_json::from_str(&specimen.contract_text) {
        Ok(document) => document,
        Err(error) => {
            card(ui, |ui| {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("The contract buffer does not parse as JSON: {error}"),
                );
                ui.label("Fix it under Sources, then return here.");
            });
            return;
        }
    };
    let registry: serde_json::Value =
        serde_json::from_str(&specimen.registry_text).unwrap_or(serde_json::Value::Null);
    let registry = (!registry.is_null()).then_some(registry);
    let (kinds, purposes) = registry_choices(registry.as_ref());
    let mut changed = false;

    card(ui, |ui| {
        ui.label(egui::RichText::new("The bounded question").strong());
        changed |= form_text(ui, &mut contract, "/question", 3);
        ui.horizontal_wrapped(|ui| {
            ui.label("contract id");
            changed |= form_string(ui, &mut contract, "/contract_id");
            ui.label("revision");
            changed |= form_u64(ui, &mut contract, "/revision");
            ui.label("status");
            changed |= form_choice(
                ui,
                "status",
                &mut contract,
                "/status",
                &["draft", "in_review", "approved", "retired"],
            );
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Execution policy").strong());
        ui.horizontal_wrapped(|ui| {
            if contract.pointer("/execution_policy").is_none() {
                contract["execution_policy"] = serde_json::json!({});
            }
            changed |= form_bool(
                ui,
                &mut contract,
                "/execution_policy/permit_nominal_basis",
                "permit nominal-basis requirements",
            );
            changed |= form_bool(
                ui,
                &mut contract,
                "/execution_policy/require_qualification",
                "require qualification on bounded and enclosure evidence",
            );
            changed |= form_bool(
                ui,
                &mut contract,
                "/execution_policy/require_signatures",
                "require a signed manifest and receipts to run",
            );
        });
    });

    card(ui, |ui| {
        ui.label(
            egui::RichText::new("Assumptions — conditions accepted without being established")
                .strong(),
        );
        let count = contract["assumptions"].as_array().map_or(0, Vec::len);
        let mut remove: Option<usize> = None;
        for index in 0..count {
            ui.horizontal_wrapped(|ui| {
                changed |= form_string(ui, &mut contract, &format!("/assumptions/{index}"));
                if ui.small_button("remove").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            contract["assumptions"]
                .as_array_mut()
                .unwrap()
                .remove(index);
            changed = true;
        }
        if ui.small_button("+ assumption").clicked() {
            if !contract["assumptions"].is_array() {
                contract["assumptions"] = serde_json::json!([]);
            }
            contract["assumptions"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::Value::String(String::new()));
            changed = true;
        }
    });

    ui.label(egui::RichText::new("Inputs — what the workflow consumes").strong());
    let count = contract["inputs"].as_array().map_or(0, Vec::len);
    let mut remove: Option<usize> = None;
    for index in 0..count {
        let (edited, delete) = input_form(ui, index, &mut contract, registry.as_ref());
        changed |= edited;
        if delete {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        contract["inputs"].as_array_mut().unwrap().remove(index);
        changed = true;
    }
    if ui
        .button("+ input")
        .on_hover_text("Append a contract input; the compiler reports what it still needs.")
        .clicked()
    {
        if !contract["inputs"].is_array() {
            contract["inputs"] = serde_json::json!([]);
        }
        let role = registry_roles(registry.as_ref());
        let first = role.first().cloned().unwrap_or_default();
        contract["inputs"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "input_id": "",
                "role": {"id": first, "major": 1},
                "media_type": role_media_types(registry.as_ref(), &first)
                    .first()
                    .cloned()
                    .unwrap_or_default(),
                "claim_model": {"model": role_claim_models(registry.as_ref(), &first)
                    .first()
                    .cloned()
                    .unwrap_or_default()},
            }));
        changed = true;
    }

    ui.label(egui::RichText::new("Workflow — the method that answers the question").strong());
    let count = contract["workflow"].as_array().map_or(0, Vec::len);
    let mut remove: Option<usize> = None;
    for index in 0..count {
        let (edited, delete) = step_form(ui, index, &mut contract, registry.as_ref());
        changed |= edited;
        if delete {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        contract["workflow"].as_array_mut().unwrap().remove(index);
        changed = true;
    }
    if ui
        .button("+ step")
        .on_hover_text("Append a workflow step; the compiler reports what it still needs.")
        .clicked()
    {
        if !contract["workflow"].is_array() {
            contract["workflow"] = serde_json::json!([]);
        }
        contract["workflow"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "step_id": "",
                "capability_type": {
                    "id": registry_capability_types(registry.as_ref())
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                    "major": 1,
                },
            }));
        changed = true;
    }

    ui.label(egui::RichText::new("Requirements — what an answer must establish").strong());
    let count = contract["requirements"].as_array().map_or(0, Vec::len);
    let mut remove: Option<usize> = None;
    for index in 0..count {
        let (edited, delete) = requirement_form(
            ui,
            index,
            &mut contract,
            &kinds,
            &purposes,
            registry.as_ref(),
        );
        changed |= edited;
        if delete {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        contract["requirements"]
            .as_array_mut()
            .unwrap()
            .remove(index);
        changed = true;
    }
    if ui
        .button("+ requirement")
        .on_hover_text("Append a draft requirement; the compiler reports what it still needs.")
        .clicked()
    {
        if !contract["requirements"].is_array() {
            contract["requirements"] = serde_json::json!([]);
        }
        contract["requirements"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "requirement_id": "",
                "statement": "",
                "purpose": purposes
                    .first()
                    .map(|id| serde_json::json!({"id": id, "major": 1}))
                    .unwrap_or_else(|| serde_json::json!({"id": "", "major": 1})),
                "comparison": "less_than_or_equal",
                "limit": {
                    "kind": kinds.first().cloned().unwrap_or_default(),
                    "value": "0",
                    "unit": registry
                        .as_ref()
                        .and_then(|doc| registry_units(
                            Some(doc),
                            kinds.first().map(String::as_str).unwrap_or_default(),
                        ).first().cloned())
                        .unwrap_or_default(),
                },
                "basis": {"kind": "bounded"},
            }));
        changed = true;
    }
    ui.label(
        egui::RichText::new("Categorical requirements — closed-set checks on named outcomes")
            .strong(),
    );
    let count = contract["categorical_requirements"]
        .as_array()
        .map_or(0, Vec::len);
    let mut remove: Option<usize> = None;
    for index in 0..count {
        let (edited, delete) =
            categorical_form(ui, index, &mut contract, &purposes, registry.as_ref());
        changed |= edited;
        if delete {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        contract["categorical_requirements"]
            .as_array_mut()
            .unwrap()
            .remove(index);
        changed = true;
    }
    if ui
        .button("+ categorical requirement")
        .on_hover_text(
            "Append a categorical requirement; the compiler reports what it still needs.",
        )
        .clicked()
    {
        if !contract["categorical_requirements"].is_array() {
            contract["categorical_requirements"] = serde_json::json!([]);
        }
        contract["categorical_requirements"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "requirement_id": "",
                "statement": "",
                "purpose": purposes
                    .first()
                    .map(|id| serde_json::json!({"id": id, "major": 1}))
                    .unwrap_or_else(|| serde_json::json!({"id": "", "major": 1})),
                "predicate": {"operator": "equals", "value": ""},
            }));
        changed = true;
    }
    ui.add_space(4.0);
    ui.colored_label(
        muted(ui),
        "Review declarations and material factors stay under Sources — a review needs a policy digest no form can invent.",
    );

    if changed {
        specimen.contract_text = serde_json::to_string_pretty(&contract)
            .unwrap_or_else(|_| specimen.contract_text.clone());
        specimen.dirty = true;
    }
}

fn show_sources(ui: &mut egui::Ui, specimen: &mut Specimen) {
    section_heading(
        ui,
        "Sources",
        "Edit the contract and registry as JSON. Check runs the same compiler the case runner uses and changes nothing on disk; Save writes both buffers back to the files they were opened from.",
    );

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Open contract…")
            .on_hover_text("Open a contract JSON file; its registry.json sibling loads too.")
            .clicked()
        {
            specimen
                .picker
                .start(ui.ctx(), false, "Open a contract JSON file");
        }
        if ui
            .button("Check")
            .on_hover_text("Compile the buffers as they stand.")
            .clicked()
        {
            let _ = specimen.recheck();
        }
        if ui
            .add_enabled(
                specimen.dirty
                    && specimen.contract_path.is_some()
                    && specimen.registry_path.is_some(),
                egui::Button::new("Save"),
            )
            .on_hover_text("Write both buffers back to the files they were opened from.")
            .clicked()
        {
            specimen.save();
        }
        if ui
            .button("Reset to specimen")
            .on_hover_text("Discard the buffers and reload the embedded draft.")
            .clicked()
        {
            specimen.reset();
        }
        if specimen.dirty {
            ui.colored_label(CORE_ORANGE, "unsaved edits");
        }
    });
    if let Some(notice) = &specimen.notice {
        ui.colored_label(muted(ui), notice);
    }
    ui.add_space(4.0);
    match &specimen.check {
        Ok((_, report)) => {
            let blocking = report
                .findings
                .iter()
                .filter(|finding| finding.blocks_compilation())
                .count();
            ui.horizontal_wrapped(|ui| {
                match report.status {
                    CompilationStatus::Compiled => {
                        badge(ui, "COMPILED", egui::Color32::from_rgb(76, 175, 80));
                    }
                    CompilationStatus::Rejected => {
                        badge(ui, "REJECTED", egui::Color32::LIGHT_RED);
                    }
                }
                ui.colored_label(
                    muted(ui),
                    format!(
                        "{} finding(s), {} blocking — see Findings",
                        report.findings.len(),
                        blocking
                    ),
                );
            });
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("The sources do not compile: {error}"),
            );
        }
    }
    ui.separator();

    let contract_label = specimen.contract_path.as_ref().map_or_else(
        || "contract — embedded specimen".to_string(),
        |path| format!("contract — {}", path.display()),
    );
    ui.label(egui::RichText::new(contract_label).strong());
    if ui
        .add(
            egui::TextEdit::multiline(&mut specimen.contract_text)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(22),
        )
        .changed()
    {
        specimen.dirty = true;
    }
    ui.add_space(8.0);
    let registry_label = specimen.registry_path.as_ref().map_or_else(
        || "registry — embedded specimen".to_string(),
        |path| format!("registry — {}", path.display()),
    );
    ui.label(egui::RichText::new(registry_label).strong());
    if ui
        .add(
            egui::TextEdit::multiline(&mut specimen.registry_text)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(14),
        )
        .changed()
    {
        specimen.dirty = true;
    }
}

fn show_findings(
    ui: &mut egui::Ui,
    report: &CompileReport,
    contract_bytes: &[u8],
    registry_bytes: &[u8],
    applies: &mut Option<&mut Vec<(String, Vec<RepairEdit>)>>,
) {
    section_heading(
        ui,
        "Findings",
        "Every finding carries a stable code, a class, an accountable owner, a JSON Pointer, and typed repair candidates where a bounded repair exists. A candidate the compiler can state exactly can be applied to the source buffer with one click — the check re-runs and nothing writes to disk until Save.",
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
            &[("contract", contract_bytes), ("registry", registry_bytes)],
            applies.as_deref_mut(),
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

fn show_finding(
    ui: &mut egui::Ui,
    finding: &CoreDiagnostic,
    sources: &[(&str, &[u8])],
    mut applies: Option<&mut Vec<(String, Vec<RepairEdit>)>>,
) {
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
                for (index, candidate) in repair.candidates.iter().enumerate() {
                    let applicable = repair.edits.get(index).filter(|edits| !edits.is_empty());
                    match (applicable, applies.as_deref_mut()) {
                        (Some(edits), Some(collector))
                            if ui
                                .button(format!("Apply: {candidate}"))
                                .on_hover_text(
                                    "Apply the compiler's exact edit to the source buffer; the check re-runs and nothing writes to disk until Save.",
                                )
                                .clicked() =>
                        {
                            collector.push((finding.primary.document.clone(), edits.clone()));
                        }
                        _ => {
                            ui.monospace(candidate);
                        }
                    }
                }
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
        key_value(ui, "Snapshot", compiled.snapshot_sha256());
        key_value(ui, "Compiler", compiled.compiler());
        key_value(ui, "Semantic profile", compiled.semantic_profile());
    });
    for (index, step) in compiled.workflow().iter().enumerate() {
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
            if step.presentation_gate.is_some() {
                badge(
                    ui,
                    "OPTIONAL PRACTICALITY GATE",
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
    for requirement in &contract.categorical_requirements {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&requirement.requirement_id).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    verdict_badge(ui, VerdictStatus::NotEvaluated);
                });
            });
            ui.label(&requirement.statement);
            ui.colored_label(muted(ui), "No admitted categorical evidence exists.");
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
    fn invalid_open_preserves_the_current_case() {
        let context = egui::Context::default();
        let mut app = CoreApp::new(&context, case_view::CaseSetup::from_arguments(&[]).unwrap());
        assert_eq!(app.mode, Mode::Cases);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-003-thermal-spreader");
        app.open_case(&path);
        let selected = app.case.info.as_ref().unwrap().path.clone();
        app.open_case(&path.join("not-a-case"));
        assert!(app.browser.error.is_some());
        assert_eq!(app.case.info.as_ref().unwrap().path, selected);
        assert_eq!(app.mode, Mode::Cases);
    }
    #[test]
    fn embedded_specimen_is_an_honest_draft() {
        let specimen = Specimen::embedded().expect("embedded specimen should load");
        let (_, report) = specimen
            .check
            .as_ref()
            .expect("the embedded specimen compiles to a report");
        assert_eq!(report.status, CompilationStatus::Rejected);
        assert!(!report.findings.is_empty());
        for finding in &report.findings {
            assert_eq!(finding.class, FindingClass::Missing, "{finding:?}");
            assert_eq!(finding.owner, "requester", "{finding:?}");
            assert!(
                matches!(finding.code.as_str(), "CORE-S1301" | "CORE-T2501"),
                "only declared placeholders may block the specimen: {finding:?}"
            );
        }
    }

    fn draft_scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("avila-app-draft-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_draft_opens_edits_checks_and_saves() {
        let dir = draft_scratch("open");
        let contract_path = dir.join("contract.json");
        let registry_path = dir.join("registry.json");
        std::fs::write(&contract_path, CONTRACT_JSON).unwrap();
        std::fs::write(&registry_path, REGISTRY_JSON).unwrap();

        let mut specimen = Specimen::embedded().unwrap();
        specimen.open(contract_path.clone());
        assert_eq!(
            specimen.contract_path.as_deref(),
            Some(contract_path.as_path())
        );
        assert_eq!(
            specimen.registry_path.as_deref(),
            Some(registry_path.as_path())
        );
        assert!(!specimen.dirty);
        assert!(specimen.check.is_ok());

        specimen
            .contract_text
            .push_str("\n// an edit that breaks parsing\n");
        specimen.dirty = true;
        assert!(specimen.recheck().is_err());

        // A good edit rechecks into a report; Save writes both files.
        specimen.contract_text = String::from_utf8_lossy(CONTRACT_JSON).into_owned();
        assert!(specimen.recheck().is_ok());
        specimen.save();
        assert!(!specimen.dirty);
        assert_eq!(
            std::fs::read(&contract_path).unwrap(),
            specimen.contract_text.as_bytes()
        );
        assert_eq!(
            std::fs::read(&registry_path).unwrap(),
            specimen.registry_text.as_bytes()
        );

        specimen.reset();
        assert!(specimen.contract_path.is_none());
        assert!(specimen.check.is_ok());
    }

    #[test]
    fn apply_edits_runs_the_rfc6902_vocabulary() {
        let text = r#"{"a": {"b": 1}, "list": ["x", "y"], "gone": true}"#;
        let out = apply_edits(
            text,
            &[
                RepairEdit::Replace {
                    path: "/a/b".into(),
                    value: serde_json::json!(2),
                },
                RepairEdit::Add {
                    path: "/list/-".into(),
                    value: serde_json::json!("z"),
                },
                RepairEdit::Remove {
                    path: "/gone".into(),
                },
            ],
        )
        .unwrap();
        let document: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(document["a"]["b"], 2);
        assert_eq!(document["list"], serde_json::json!(["x", "y", "z"]));
        assert!(document.get("gone").is_none());
        assert!(
            apply_edits(
                text,
                &[RepairEdit::Replace {
                    path: "/missing".into(),
                    value: serde_json::json!(0)
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn a_repair_apply_patches_the_buffer_and_rechecks() {
        let mut specimen = Specimen::embedded().unwrap();
        specimen.apply_repairs(
            "contract",
            &[RepairEdit::Replace {
                path: "/contract_id".into(),
                value: serde_json::json!("edited.draft"),
            }],
        );
        assert!(specimen.dirty);
        assert!(specimen.contract_text.contains("edited.draft"));
        // The check re-ran against the edited buffer.
        assert!(specimen.check.is_ok());
        assert!(apply_edits(&specimen.contract_text, &[]).is_ok());
    }

    #[test]
    fn an_exact_repair_applies_and_clears_its_finding() {
        let mut specimen = Specimen::embedded().unwrap();
        // An undeclared material factor is a CORE-S1101 finding with an
        // exact removal repair.
        let mut document: serde_json::Value =
            serde_json::from_str(&specimen.contract_text).unwrap();
        document["workflow"][0]["reproducibility"]["material_factors"] =
            serde_json::json!({ "bogus-factor": "1" });
        specimen.contract_text = serde_json::to_string_pretty(&document).unwrap();
        specimen.recheck().unwrap();
        let edits = {
            let (_, report) = specimen.check.as_ref().unwrap();
            let finding = report
                .findings
                .iter()
                .find(|finding| finding.code == "CORE-S1101")
                .expect("an undeclared factor is a S1101 finding");
            let edits = finding.repairs[0].edits[0].clone();
            assert!(!edits.is_empty());
            edits
        };
        specimen.apply_repairs("contract", &edits);
        let (_, report) = specimen.check.as_ref().unwrap();
        assert!(
            !report.findings.iter().any(|f| f.code == "CORE-S1101"),
            "the applied repair cleared its finding"
        );
        // The draft's own missing-parameter findings are untouched.
        assert!(report.findings.iter().any(|f| f.code == "CORE-S1301"));
    }

    #[test]
    fn provisional_logo_decodes() {
        let image = image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png)
            .expect("logo should be a PNG");
        assert_eq!((image.width(), image.height()), (2000, 2000));
    }

    #[test]
    fn question_form_lists_registry_and_workflow_choices() {
        let contract: serde_json::Value =
            serde_json::from_slice(CONTRACT_JSON).expect("specimen contract parses");
        let registry: serde_json::Value =
            serde_json::from_slice(REGISTRY_JSON).expect("specimen registry parses");

        let (kinds, purposes) = registry_choices(Some(&registry));
        assert!(!kinds.is_empty());
        assert!(!purposes.is_empty());
        for kind in &kinds {
            assert!(
                !registry_units(Some(&registry), kind).is_empty(),
                "kind {kind} declares no units"
            );
        }

        let steps = workflow_steps(&contract);
        assert!(!steps.is_empty());
        for step in &steps {
            let slots = step_output_slots(&contract, Some(&registry), step);
            assert!(
                !slots.is_empty(),
                "step {step} has no registry-declared output slots"
            );
        }
        // The specimen's requirements' metric fields resolve to real choices.
        let requirement = &contract["requirements"][0];
        assert_eq!(requirement["metric"]["source"], "step_output");
        let step_id = requirement["metric"]["step_id"].as_str().unwrap();
        assert!(steps.iter().any(|step| step == step_id));
        let slot = requirement["metric"]["output_slot"].as_str().unwrap();
        assert!(
            step_output_slots(&contract, Some(&registry), step_id)
                .iter()
                .any(|declared| declared == slot)
        );
    }

    #[test]
    fn question_form_survives_a_noncompiling_buffer() {
        // The form reads the buffer as plain JSON: a syntactically valid but
        // noncompiling contract still renders, and an invalid one is reported
        // rather than panicking.
        let mut specimen = Specimen::embedded().unwrap();
        specimen.contract_text = "{ not json".to_string();
        assert!(serde_json::from_str::<serde_json::Value>(&specimen.contract_text).is_err());
        specimen.contract_text = "{\"schema_version\": \"x\"}".to_string();
        let doc: serde_json::Value = serde_json::from_str(&specimen.contract_text).unwrap();
        assert!(workflow_steps(&doc).is_empty());
        assert!(contract_inputs(&doc).is_empty());
        let (kinds, purposes) = registry_choices(Some(&serde_json::json!({})));
        assert!(kinds.is_empty() && purposes.is_empty());
    }

    #[test]
    fn input_form_choices_come_from_the_registry_roles() {
        let contract: serde_json::Value =
            serde_json::from_slice(CONTRACT_JSON).expect("specimen contract parses");
        let registry: serde_json::Value =
            serde_json::from_slice(REGISTRY_JSON).expect("specimen registry parses");

        let roles = registry_roles(Some(&registry));
        assert!(!roles.is_empty());
        for role in &roles {
            assert!(
                !role_media_types(Some(&registry), role).is_empty(),
                "role {role} admits no media types"
            );
            assert!(
                !role_claim_models(Some(&registry), role).is_empty(),
                "role {role} admits no claim models"
            );
        }
        // Every authored input's role resolves, and its media type and claim
        // model are among that role's admitted values.
        for input in contract["inputs"].as_array().into_iter().flatten() {
            let role = input["role"]["id"].as_str().unwrap();
            assert!(roles.iter().any(|declared| declared == role));
            let media = input["media_type"].as_str().unwrap();
            assert!(
                role_media_types(Some(&registry), role)
                    .iter()
                    .any(|m| m == media)
            );
            let model = input["claim_model"]["model"].as_str().unwrap();
            assert!(
                role_claim_models(Some(&registry), role)
                    .iter()
                    .any(|m| m == model)
            );
        }
        // An unknown role offers no constrained choices.
        assert!(role_media_types(Some(&registry), "unknown.role").is_empty());
        assert!(role_claim_models(Some(&registry), "unknown.role").is_empty());
    }

    #[test]
    fn step_form_reads_factor_and_review_declarations() {
        // The step card's material-factor fields come from the capability
        // type's reproducibility declaration — the same document the
        // compiler validates the bound values against.
        let registry: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/semantic-core/types/compiler.reproducibility.registry.v1.json"
        ))
        .unwrap();
        let ty = registry_capability_type(Some(&registry), "fixture.deterministic_metric")
            .expect("fixture registry declares the metric type");
        let factors = ty["reproducibility"]["material_factors"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(factors.len(), 2);
        let thread_count = factors
            .iter()
            .find(|f| f["factor_id"] == "thread_count")
            .expect("thread_count factor declared");
        assert_eq!(thread_count["value_type"]["type"], "integer");
        let rng = factors
            .iter()
            .find(|f| f["factor_id"] == "environment_image");
        assert!(rng.is_some());

        // The review registry's agent-review capability exists, and the
        // contract that binds it carries the review fields the form edits.
        let review_registry: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/semantic-core/types/compiler.review.registry.v1.json"
        ))
        .unwrap();
        let review_type =
            registry_capability_type(Some(&review_registry), "fixture.practical_agent_review")
                .expect("review registry declares the review type");
        assert!(review_type["review"].is_object());
        let contract: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../fixtures/semantic-core/types/types.R9.review-bound.pass.contract.json"
        ))
        .unwrap();
        let review = &contract["workflow"][1]["review"];
        assert!(review.is_object());
        assert_eq!(review["independence"]["mode"], "constraints");
        assert_eq!(
            review["independence"]["requirements"]
                .as_array()
                .map_or(0, Vec::len),
            2
        );
    }
}
