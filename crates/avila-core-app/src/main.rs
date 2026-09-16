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
    Sources,
    Contract,
    Findings,
    Compiled,
    Results,
    Evidence,
}

impl Workspace {
    const ALL: [Self; 7] = [
        Self::Overview,
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
                .and_then(|report| {
                    serde_json::from_str::<ContractSource>(&self.contract_text)
                        .map(|contract| (contract, report))
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
}
