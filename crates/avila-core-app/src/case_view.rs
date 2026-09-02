//! The case workbench: run a composed case through the same runner the CLI
//! uses and render its report. Every state, badge, and number shown here is
//! read from the run report; the view computes nothing and holds no
//! scientific state of its own.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use avila_core_compiler::CompileReport;
use avila_core_evidence::{CasePackageManifest, IntegrityCheckState, PackageIntegrityStatus};
use avila_core_kernel::VerdictStatus;
use avila_core_runner::{
    BindingStatus, CapabilityCheckState, CaseRunOptions, CaseRunReport, CaseRunStatus,
    ExecutionStatus, StepExecutionState, execute_case, human_summary,
};
use eframe::egui;

use crate::help::{HelpTab, TourTarget, TourTargets};
use crate::{CORE_ORANGE, badge, card, key_value, muted, section_heading, show_finding};

const GREEN: egui::Color32 = egui::Color32::from_rgb(95, 197, 128);
const RED: egui::Color32 = egui::Color32::from_rgb(232, 102, 102);
const BLUE: egui::Color32 = egui::Color32::from_rgb(120, 164, 210);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseTab {
    #[default]
    Overview,
    Integrity,
    Compile,
    Execute,
    Claims,
    Verdicts,
}

impl CaseTab {
    const ALL: [Self; 6] = [
        Self::Overview,
        Self::Integrity,
        Self::Compile,
        Self::Execute,
        Self::Claims,
        Self::Verdicts,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Integrity => "Integrity",
            Self::Compile => "Compile",
            Self::Execute => "Execute",
            Self::Claims => "Claims",
            Self::Verdicts => "Verdicts",
        }
    }

    fn by_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|tab| tab.label().eq_ignore_ascii_case(name))
    }

    const fn help_tab(self) -> HelpTab {
        match self {
            Self::Overview => HelpTab::Overview,
            Self::Integrity => HelpTab::Integrity,
            Self::Compile => HelpTab::Compile,
            Self::Execute => HelpTab::Execute,
            Self::Claims => HelpTab::Claims,
            Self::Verdicts => HelpTab::Verdicts,
        }
    }

    const fn from_help(tab: HelpTab) -> Self {
        match tab {
            HelpTab::Overview => Self::Overview,
            HelpTab::Integrity => Self::Integrity,
            HelpTab::Compile => Self::Compile,
            HelpTab::Execute => Self::Execute,
            HelpTab::Claims => Self::Claims,
            HelpTab::Verdicts => Self::Verdicts,
        }
    }
}

/// The bundled egui fonts have no glyph for the arrow that case titles use;
/// render it as ASCII rather than as a missing-glyph box. Display only: the
/// report text itself is never changed.
fn display(text: &str) -> String {
    text.replace('→', "->")
}

/// One `NAME=PATH` row in the setup panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamedPath {
    pub name: String,
    pub path: String,
}

/// Launch-time configuration, from the command line or the package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseSetup {
    pub case_dir: String,
    pub source_roots: Vec<NamedPath>,
    pub capabilities: Vec<NamedPath>,
    pub workspace: String,
    pub reuse: bool,
    /// Start a run (or a plan) as soon as the window opens.
    pub auto_run: Option<bool>,
    /// Save a PNG of the window once the automatic run has rendered, then
    /// close. A development aid for reviewing the view without a hand.
    pub screenshot: Option<String>,
    /// The tab to open first (a development aid with `--screenshot`).
    pub tab: Option<String>,
    /// Start in light mode.
    pub light: bool,
    /// Start a walkthrough immediately, by its first word or full title.
    pub tour: Option<String>,
}

impl CaseSetup {
    /// Parse `--case DIR`, `--source-root NAME=PATH`, `--capability NAME=PATH`,
    /// and `--workspace DIR` from the process arguments. Unknown arguments
    /// are reported, never ignored.
    pub fn from_arguments(arguments: &[String]) -> Result<Self, String> {
        let mut setup = Self {
            case_dir: "examples/cases/case-000-actinv-aftermatter".into(),
            reuse: true,
            ..Self::default()
        };
        let mut iterator = arguments.iter();
        while let Some(flag) = iterator.next() {
            let mut value = || {
                iterator
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{flag} requires a value"))
            };
            match flag.as_str() {
                "--case" => setup.case_dir = value()?,
                "--workspace" => setup.workspace = value()?,
                "--source-root" => setup.source_roots.push(named_path(&value()?)?),
                "--capability" => setup.capabilities.push(named_path(&value()?)?),
                "--no-reuse" => setup.reuse = false,
                "--auto-run" => setup.auto_run = Some(false),
                "--auto-plan" => setup.auto_run = Some(true),
                "--screenshot" => setup.screenshot = Some(value()?),
                "--tab" => setup.tab = Some(value()?),
                "--light" => setup.light = true,
                "--tour" => setup.tour = Some(value()?),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(setup)
    }

    /// Add empty rows for every root and capability the case package
    /// requests that the setup does not already name, so the user sees what
    /// the case asks for before running it.
    pub fn absorb_manifest(&mut self, manifest: &CasePackageManifest) {
        for artifact in &manifest.artifacts {
            if !self
                .source_roots
                .iter()
                .any(|root| root.name == artifact.source_root)
            {
                self.source_roots.push(NamedPath {
                    name: artifact.source_root.clone(),
                    path: String::new(),
                });
            }
        }
        for capability in &manifest.capabilities {
            if !self
                .capabilities
                .iter()
                .any(|entry| entry.name == capability.capability_id)
            {
                self.capabilities.push(NamedPath {
                    name: capability.capability_id.clone(),
                    path: String::new(),
                });
            }
        }
    }

    fn options(&self, plan_only: bool) -> CaseRunOptions {
        let paths = |rows: &[NamedPath]| -> BTreeMap<String, PathBuf> {
            rows.iter()
                .filter(|row| !row.name.trim().is_empty() && !row.path.trim().is_empty())
                .map(|row| (row.name.trim().to_string(), PathBuf::from(row.path.trim())))
                .collect()
        };
        CaseRunOptions {
            source_roots: paths(&self.source_roots),
            capabilities: paths(&self.capabilities),
            workspace: (!self.workspace.trim().is_empty())
                .then(|| PathBuf::from(self.workspace.trim())),
            reuse: self.reuse,
            plan_only,
            ..CaseRunOptions::default()
        }
    }
}

fn named_path(value: &str) -> Result<NamedPath, String> {
    let (name, path) = value
        .split_once('=')
        .ok_or_else(|| format!("`{value}` must have the form NAME=PATH"))?;
    if name.is_empty() || path.is_empty() {
        return Err(format!("`{value}` must contain a non-empty name and path"));
    }
    Ok(NamedPath {
        name: name.into(),
        path: path.into(),
    })
}

/// Read the case package to learn which roots and capabilities it requests.
pub fn inspect_case(case_dir: &str) -> Result<CasePackageManifest, String> {
    let path = PathBuf::from(case_dir).join("package.json");
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub struct CaseView {
    pub setup: CaseSetup,
    tab: CaseTab,
    manifest: Option<CasePackageManifest>,
    inspect_error: Option<String>,
    running: Option<Receiver<Result<CaseRunReport, String>>>,
    last_plan_only: bool,
    report: Option<CaseRunReport>,
    run_error: Option<String>,
    summary: String,
    settled_frames: u32,
    screenshot_requested: bool,
    started: Option<Instant>,
    last_duration: Option<Duration>,
    /// The case documents as bytes, read when a report arrives, so findings
    /// can be shown at their source lines.
    sources: Vec<(String, Vec<u8>)>,
}

impl CaseView {
    pub fn new(mut setup: CaseSetup) -> Self {
        let (manifest, inspect_error) = match inspect_case(&setup.case_dir) {
            Ok(manifest) => {
                setup.absorb_manifest(&manifest);
                (Some(manifest), None)
            }
            Err(error) => (None, Some(error)),
        };
        let tab = setup
            .tab
            .as_deref()
            .and_then(CaseTab::by_name)
            .unwrap_or_default();
        Self {
            setup,
            tab,
            manifest,
            inspect_error,
            running: None,
            last_plan_only: false,
            report: None,
            run_error: None,
            summary: String::new(),
            settled_frames: 0,
            screenshot_requested: false,
            started: None,
            last_duration: None,
            sources: Vec::new(),
        }
    }

    fn read_sources(&mut self) {
        let case_dir = PathBuf::from(self.setup.case_dir.trim());
        self.sources = [
            ("contract", "contract.json"),
            ("registry", "registry.json"),
            ("claims", "claims.json"),
        ]
        .into_iter()
        .filter_map(|(document, file)| {
            std::fs::read(case_dir.join(file))
                .ok()
                .map(|bytes| (document.to_string(), bytes))
        })
        .collect();
    }

    pub fn help_tab(&self) -> HelpTab {
        self.tab.help_tab()
    }

    pub fn show_help_tab(&mut self, tab: HelpTab) {
        self.tab = CaseTab::from_help(tab);
    }

    fn finish(&mut self) {
        self.running = None;
        self.last_duration = self.started.take().map(|started| started.elapsed());
    }

    /// Start the automatic run requested on the command line, once.
    pub fn start_automatic(&mut self) {
        if let Some(plan_only) = self.setup.auto_run.take() {
            self.start(plan_only);
        }
    }

    /// Drive the development screenshot: after the automatic run has rendered
    /// for a few frames, ask the viewport for a capture; when it arrives, write
    /// it as PNG and close the window.
    pub fn drive_screenshot(&mut self, context: &egui::Context) {
        let Some(path) = self.setup.screenshot.clone() else {
            return;
        };
        if self.running.is_some() || (self.report.is_none() && self.run_error.is_none()) {
            return;
        }
        self.settled_frames += 1;
        context.request_repaint_after(Duration::from_millis(50));
        if self.settled_frames == 6 && !self.screenshot_requested {
            self.screenshot_requested = true;
            context.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let captured = context.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = captured {
            let [width, height] = image.size;
            let bytes: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            match image::save_buffer(
                &path,
                &bytes,
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgba8,
            ) {
                Ok(()) => eprintln!("wrote {path} ({width}×{height})"),
                Err(error) => eprintln!("could not write {path}: {error}"),
            }
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn open_case(&mut self) {
        match inspect_case(&self.setup.case_dir) {
            Ok(manifest) => {
                self.setup.absorb_manifest(&manifest);
                self.manifest = Some(manifest);
                self.inspect_error = None;
            }
            Err(error) => {
                self.manifest = None;
                self.inspect_error = Some(error);
            }
        }
    }

    fn start(&mut self, plan_only: bool) {
        if self.running.is_some() {
            return;
        }
        let case_dir = PathBuf::from(self.setup.case_dir.trim());
        let options = self.setup.options(plan_only);
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let outcome = execute_case(&case_dir, &options).map_err(|error| error.to_string());
            let _ = sender.send(outcome);
        });
        self.last_plan_only = plan_only;
        self.running = Some(receiver);
        self.run_error = None;
        self.started = Some(Instant::now());
    }

    fn poll(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.running else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(report)) => {
                self.summary = human_summary(&report);
                self.report = Some(report);
                self.read_sources();
                self.finish();
            }
            Ok(Err(error)) => {
                self.run_error = Some(error);
                self.finish();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(120));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.run_error = Some("the run thread ended without a report".into());
                self.finish();
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, targets: &mut TourTargets) {
        self.start_automatic();
        self.poll(ui.ctx());
        self.drive_screenshot(ui.ctx());
        egui::Panel::left("case-setup")
            .resizable(true)
            .show(ui, |ui| {
                ui.set_min_width(340.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.setup_panel(ui, targets));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let tabs = ui.horizontal_wrapped(|ui| {
                for tab in CaseTab::ALL {
                    if ui.selectable_label(self.tab == tab, tab.label()).clicked() {
                        self.tab = tab;
                    }
                }
            });
            targets.set(TourTarget::Tabs, tabs.response.rect);
            ui.separator();
            let panel = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.report_panel(ui, targets));
            targets.set(TourTarget::ReportPanel, panel.inner_rect);
        });
    }

    fn setup_panel(&mut self, ui: &mut egui::Ui, targets: &mut TourTargets) {
        ui.add_space(4.0);
        let case = ui.scope(|ui| {
            ui.label(
                egui::RichText::new("CASE")
                    .size(10.0)
                    .strong()
                    .color(muted(ui)),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.case_dir)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    self.open_case();
                }
                if let Some(manifest) = &self.manifest {
                    ui.label(
                        egui::RichText::new(display(&format!(
                            "{} — {}",
                            manifest.case_id, manifest.title
                        )))
                        .color(muted(ui)),
                    );
                }
            });
            if let Some(error) = &self.inspect_error {
                ui.colored_label(RED, error);
            }
            if let Some(manifest) = &self.manifest {
                ui.label(
                    egui::RichText::new(format!(
                        "{} documents, {} artifacts, {} capabilities, {} executions declared",
                        manifest.documents.len(),
                        manifest.artifacts.len(),
                        manifest.capabilities.len(),
                        manifest.executions.len()
                    ))
                    .color(muted(ui))
                    .size(11.0),
                );
            }
        });
        targets.set(TourTarget::CaseInput, case.response.rect);

        ui.add_space(8.0);
        let roots = ui.scope(|ui| {
            named_paths(ui, "SOURCE ROOTS", "roots", &mut self.setup.source_roots);
        });
        targets.set(TourTarget::SourceRoots, roots.response.rect);
        ui.add_space(8.0);
        let capabilities = ui.scope(|ui| {
            named_paths(
                ui,
                "CAPABILITIES",
                "capabilities",
                &mut self.setup.capabilities,
            );
        });
        targets.set(TourTarget::Capabilities, capabilities.response.rect);

        ui.add_space(8.0);
        let options = ui.scope(|ui| {
            ui.label(
                egui::RichText::new("OPTIONS")
                    .size(10.0)
                    .strong()
                    .color(muted(ui)),
            );
            ui.checkbox(
                &mut self.setup.reuse,
                "Reuse steps whose committed receipt still verifies",
            );
            ui.horizontal(|ui| {
                ui.label("Workspace");
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.workspace)
                        .hint_text("workspaces/<case>/<run> by default")
                        .desired_width(f32::INFINITY),
                );
            });
        });
        targets.set(TourTarget::Options, options.response.rect);

        ui.add_space(10.0);
        let buttons = ui.horizontal(|ui| {
            let idle = self.running.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Plan"))
                .on_hover_text("Report what would be reused or rerun, and why, without running")
                .clicked()
            {
                self.start(true);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Run"))
                .on_hover_text("Verify, execute or reuse, generate claims, evaluate, replay")
                .clicked()
            {
                self.start(false);
            }
            if !idle {
                ui.spinner();
                let elapsed = self
                    .started
                    .map_or(0.0, |started| started.elapsed().as_secs_f32());
                ui.label(format!(
                    "{} {elapsed:.1} s",
                    if self.last_plan_only {
                        "planning…"
                    } else {
                        "running…"
                    }
                ));
            } else if let Some(duration) = self.last_duration {
                ui.label(
                    egui::RichText::new(format!(
                        "{} in {:.2} s",
                        if self.last_plan_only {
                            "planned"
                        } else {
                            "ran"
                        },
                        duration.as_secs_f32()
                    ))
                    .color(muted(ui)),
                );
            }
        });
        targets.set(TourTarget::RunButtons, buttons.response.rect);
        if let Some(error) = &self.run_error {
            ui.colored_label(RED, format!("The runner could not run: {error}"));
        }
        if let Some(report) = &self.report {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                outcome_badge(ui, report.status);
                ui.label(egui::RichText::new(&report.case_id).strong());
            });
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "Omitted roots are reported as not checked and omitted executables as not run. A supplied root or executable that does not match fails closed.",
            )
            .color(muted(ui))
            .size(11.0),
        );
    }

    fn report_panel(&mut self, ui: &mut egui::Ui, targets: &mut TourTargets) {
        let Some(report) = &self.report else {
            section_heading(
                ui,
                "No run yet",
                "Open a case, name its roots and executables, then Plan or Run.",
            );
            if let Some(manifest) = &self.manifest {
                show_manifest(ui, manifest);
            }
            return;
        };
        match self.tab {
            CaseTab::Overview => show_overview(ui, report, &self.summary, targets),
            CaseTab::Integrity => show_integrity(ui, report),
            CaseTab::Compile => {
                let sources: Vec<(&str, &[u8])> = self
                    .sources
                    .iter()
                    .map(|(document, bytes)| (document.as_str(), bytes.as_slice()))
                    .collect();
                show_compile(ui, report.compile.as_ref(), &sources);
            }
            CaseTab::Execute => show_execute(ui, report),
            CaseTab::Claims => show_claims(ui, report),
            CaseTab::Verdicts => show_verdicts(ui, report),
        }
    }
}

fn named_paths(ui: &mut egui::Ui, title: &str, id: &str, rows: &mut Vec<NamedPath>) {
    ui.label(
        egui::RichText::new(title)
            .size(10.0)
            .strong()
            .color(muted(ui)),
    );
    let mut remove = None;
    for (index, row) in rows.iter_mut().enumerate() {
        ui.push_id((id, index), |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut row.name)
                        .hint_text("name")
                        .desired_width(170.0),
                );
                if ui.small_button("×").on_hover_text("remove").clicked() {
                    remove = Some(index);
                }
                if row.path.trim().is_empty() {
                    badge(ui, "NOT SUPPLIED", muted(ui));
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut row.path)
                    .hint_text("path on this machine")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);
        });
    }
    if let Some(index) = remove {
        rows.remove(index);
    }
    if ui.small_button("+ add").clicked() {
        rows.push(NamedPath::default());
    }
}

fn show_manifest(ui: &mut egui::Ui, manifest: &CasePackageManifest) {
    card(ui, |ui| {
        key_value(
            ui,
            "Case",
            &display(&format!("{} — {}", manifest.case_id, manifest.title)),
        );
        let roots: Vec<&str> = {
            let mut roots: Vec<&str> = manifest
                .artifacts
                .iter()
                .map(|artifact| artifact.source_root.as_str())
                .collect();
            roots.sort_unstable();
            roots.dedup();
            roots
        };
        key_value(ui, "Requested roots", &roots.join(", "));
        key_value(
            ui,
            "Bound capabilities",
            &manifest
                .capabilities
                .iter()
                .map(|capability| {
                    format!("{} ({})", capability.capability_id, capability.package_id)
                })
                .collect::<Vec<_>>()
                .join(", "),
        );
        key_value(
            ui,
            "Executed steps",
            &manifest
                .executions
                .iter()
                .map(|execution| format!("{} via {}", execution.step_id, execution.adapter))
                .collect::<Vec<_>>()
                .join(", "),
        );
        for limitation in &manifest.limitations {
            ui.label(egui::RichText::new(limitation).color(muted(ui)).size(11.0));
        }
    });
}

fn outcome_badge(ui: &mut egui::Ui, status: CaseRunStatus) {
    match status {
        CaseRunStatus::Evaluated => badge(ui, "EVALUATED", GREEN),
        CaseRunStatus::Rejected => badge(ui, "REJECTED", RED),
        CaseRunStatus::Planned => badge(ui, "PLANNED", CORE_ORANGE),
    }
}

fn integrity_badge(ui: &mut egui::Ui, state: IntegrityCheckState) {
    match state {
        IntegrityCheckState::Verified => badge(ui, "VERIFIED", GREEN),
        IntegrityCheckState::NotChecked => badge(ui, "NOT CHECKED", muted(ui)),
        IntegrityCheckState::Missing => badge(ui, "MISSING", RED),
        IntegrityCheckState::Mismatch => badge(ui, "MISMATCH", RED),
    }
}

fn step_badge(ui: &mut egui::Ui, state: StepExecutionState) {
    match state {
        StepExecutionState::Executed => badge(ui, "EXECUTED", GREEN),
        StepExecutionState::Reused => badge(ui, "REUSED", BLUE),
        StepExecutionState::Planned => badge(ui, "PLANNED", CORE_ORANGE),
        StepExecutionState::NotRun => badge(ui, "NOT RUN", muted(ui)),
        StepExecutionState::Refused => badge(ui, "REFUSED", RED),
        StepExecutionState::Failed => badge(ui, "FAILED", RED),
    }
}

fn verdict_badge(ui: &mut egui::Ui, verdict: VerdictStatus) {
    match verdict {
        VerdictStatus::Pass => badge(ui, "PASS", GREEN),
        VerdictStatus::Fail => badge(ui, "FAIL", RED),
        VerdictStatus::Inconclusive => badge(ui, "INCONCLUSIVE", CORE_ORANGE),
        VerdictStatus::NotEvaluated => badge(ui, "NOT EVALUATED", muted(ui)),
    }
}

fn stage_row(ui: &mut egui::Ui, title: &str, label: &str, color: egui::Color32, detail: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(title).strong());
        badge(ui, label, color);
        ui.label(egui::RichText::new(detail).color(muted(ui)));
    });
}

fn show_overview(
    ui: &mut egui::Ui,
    report: &CaseRunReport,
    summary: &str,
    targets: &mut TourTargets,
) {
    section_heading(ui, &display(&report.title), &report.case_id);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Outcome").strong());
            outcome_badge(ui, report.status);
        });
        ui.add_space(6.0);
        let integrity = &report.integrity;
        let verified_artifacts = integrity
            .artifacts
            .iter()
            .filter(|check| check.state == IntegrityCheckState::Verified)
            .count();
        let (label, color) = match integrity.status {
            PackageIntegrityStatus::Complete => ("COMPLETE", GREEN),
            PackageIntegrityStatus::Partial => ("PARTIAL", CORE_ORANGE),
            PackageIntegrityStatus::Failed => ("FAILED", RED),
        };
        stage_row(
            ui,
            "1. Integrity",
            label,
            color,
            &format!(
                "{}/{} documents and {verified_artifacts}/{} artifacts re-hashed",
                integrity
                    .documents
                    .iter()
                    .filter(|check| check.state == IntegrityCheckState::Verified)
                    .count(),
                integrity.documents.len(),
                integrity.artifacts.len()
            ),
        );
        match report
            .compile
            .as_ref()
            .and_then(|compile| compile.compiled.as_ref())
        {
            Some(compiled) => stage_row(
                ui,
                "2. Compile",
                "COMPILED",
                GREEN,
                &format!(
                    "{} revision {}; {} steps; {} requirements",
                    compiled.contract_id,
                    compiled.contract_revision,
                    compiled.workflow.len(),
                    compiled.requirements.len()
                ),
            ),
            None if report.compile.is_some() => stage_row(
                ui,
                "2. Compile",
                "REJECTED",
                RED,
                &format!(
                    "{} finding(s)",
                    report
                        .compile
                        .as_ref()
                        .map_or(0, |compile| compile.findings.len())
                ),
            ),
            None => stage_row(ui, "2. Compile", "NOT RUN", muted(ui), ""),
        }
        match &report.execution {
            Some(execution) => {
                let (label, color) = match execution.status {
                    ExecutionStatus::Executed => ("EXECUTED", GREEN),
                    ExecutionStatus::Reused => ("REUSED", BLUE),
                    ExecutionStatus::Planned => ("PLANNED", CORE_ORANGE),
                    ExecutionStatus::NotRun => ("NOT RUN", muted(ui)),
                    ExecutionStatus::Partial => ("PARTIAL", CORE_ORANGE),
                    ExecutionStatus::Refused => ("REFUSED", RED),
                    ExecutionStatus::Failed => ("FAILED", RED),
                };
                let detail = execution
                    .steps
                    .iter()
                    .map(|step| format!("{} {:?}", step.step_id, step.state).to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", ");
                stage_row(ui, "3. Execute", label, color, &detail);
            }
            None => stage_row(ui, "3. Execute", "NOT RUN", muted(ui), ""),
        }
        match &report.claims {
            Some(claims) => stage_row(
                ui,
                "4. Claims",
                if claims.matches_committed {
                    "MATCH"
                } else {
                    "MISMATCH"
                },
                if claims.matches_committed { GREEN } else { RED },
                &format!(
                    "{} attestations; {} executed, {} reused, {} recorded claims",
                    claims.input_attestations,
                    claims.executed_claims,
                    claims.reused_claims,
                    claims.recorded_claims
                ),
            ),
            None => stage_row(ui, "4. Claims", "NOT RUN", muted(ui), ""),
        }
        match &report.campaign {
            Some(campaign) => {
                for verdict in &campaign.verdicts {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("5. Evaluate").strong());
                        verdict_badge(ui, verdict.verdict.status);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} — {}",
                                verdict.requirement_id, verdict.verdict.rule
                            ))
                            .color(muted(ui)),
                        );
                    });
                }
            }
            None => stage_row(ui, "5. Evaluate", "NOT RUN", muted(ui), ""),
        }
        match &report.replay {
            Some(replay) => stage_row(
                ui,
                "6. Replay",
                if replay.matches { "MATCH" } else { "MISMATCH" },
                if replay.matches { GREEN } else { RED },
                "generated campaign report against the committed expectation",
            ),
            None => stage_row(ui, "6. Replay", "NOT RUN", muted(ui), ""),
        }
    });
    ui.add_space(8.0);
    let notice = ui.label(
        egui::RichText::new(&report.notice)
            .color(muted(ui))
            .size(11.0),
    );
    targets.set(TourTarget::OverviewText, notice.rect);
    ui.add_space(8.0);
    egui::CollapsingHeader::new("Text summary (identical to the CLI)")
        .default_open(false)
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(display(summary)).monospace().size(11.0))
                    .wrap(),
            );
        });
}

fn show_integrity(ui: &mut egui::Ui, report: &CaseRunReport) {
    let integrity = &report.integrity;
    section_heading(
        ui,
        "Package integrity",
        "Byte identity of the package documents and of every artifact under a supplied root. A match proves identity only.",
    );
    card(ui, |ui| {
        key_value(ui, "Manifest", &integrity.manifest_sha256);
        egui::Grid::new("documents").striped(true).show(ui, |ui| {
            for check in &integrity.documents {
                ui.label(&check.document_id);
                ui.label(egui::RichText::new(&check.role).color(muted(ui)));
                ui.label(&check.path);
                integrity_badge(ui, check.state);
                ui.end_row();
            }
        });
    });
    ui.add_space(8.0);
    card(ui, |ui| {
        egui::Grid::new("artifacts").striped(true).show(ui, |ui| {
            for check in &integrity.artifacts {
                ui.label(&check.artifact_id);
                ui.label(egui::RichText::new(&check.source_root).color(muted(ui)));
                ui.label(&check.path);
                integrity_badge(ui, check.state);
                ui.end_row();
            }
        });
    });
    for limitation in &integrity.limitations {
        ui.label(egui::RichText::new(limitation).color(muted(ui)).size(11.0));
    }
}

fn show_compile(ui: &mut egui::Ui, compile: Option<&CompileReport>, sources: &[(&str, &[u8])]) {
    section_heading(
        ui,
        "Compilation",
        "Structural and semantic consistency under the draft profile. No execution, admission, or verdict.",
    );
    let Some(compile) = compile else {
        ui.label("Compilation did not run because an earlier gate did not pass.");
        return;
    };
    match &compile.compiled {
        Some(compiled) => {
            card(ui, |ui| {
                key_value(
                    ui,
                    "Contract",
                    &format!(
                        "{} revision {}",
                        compiled.contract_id, compiled.contract_revision
                    ),
                );
                key_value(ui, "Question", &compiled.question);
                key_value(ui, "Snapshot", &compiled.snapshot_sha256);
                ui.add_space(6.0);
                for step in &compiled.workflow {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&step.step_id).strong());
                        ui.label(
                            egui::RichText::new(format!(
                                "{}@{}",
                                step.capability_type.id, step.capability_type.major
                            ))
                            .color(muted(ui)),
                        );
                        if step.review_obligation.is_some() {
                            badge(ui, "PENDING REVIEW", CORE_ORANGE);
                        }
                    });
                }
                ui.add_space(6.0);
                for requirement in &compiled.requirements {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&requirement.requirement_id).strong());
                        ui.label(&requirement.statement);
                    });
                }
            });
        }
        None => {
            ui.horizontal(|ui| {
                badge(ui, "REJECTED", RED);
                ui.label(format!("{} finding(s)", compile.findings.len()));
            });
        }
    }
    for finding in &compile.findings {
        show_finding(ui, finding, sources);
    }
}

fn show_execute(ui: &mut egui::Ui, report: &CaseRunReport) {
    section_heading(
        ui,
        "Execution",
        "Each declared step is reused from its committed receipt, run afresh, planned, or not run. Every change that forces a rerun is named by class.",
    );
    let Some(execution) = &report.execution else {
        ui.label("No execution stage ran.");
        return;
    };
    if let Some(workspace) = &execution.workspace {
        key_value(ui, "Workspace", workspace);
    }
    for step in &execution.steps {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&step.step_id).size(17.0).strong());
                step_badge(ui, step.state);
                ui.label(egui::RichText::new(&step.adapter).color(muted(ui)));
            });
            if let Some(capability) = &step.capability {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Capability:").strong());
                    ui.label(format!(
                        "{} ({})",
                        capability.capability_id, capability.package_id
                    ));
                    match capability.state {
                        CapabilityCheckState::Verified => badge(ui, "DIGEST VERIFIED", GREEN),
                        CapabilityCheckState::Mismatch => badge(ui, "DIGEST MISMATCH", RED),
                        CapabilityCheckState::Missing => badge(ui, "EXECUTABLE MISSING", RED),
                        CapabilityCheckState::NotSupplied => badge(ui, "NOT SUPPLIED", muted(ui)),
                    }
                });
                key_value(ui, "Bound executable", &capability.expected_sha256);
            }
            if let Some(identity) = &step.planned_invocation_sha256 {
                key_value(ui, "Planned invocation", identity);
            }
            if !step.changes.is_empty() {
                ui.label(egui::RichText::new("Changes since the committed receipt").strong());
                for change in &step.changes {
                    ui.horizontal_wrapped(|ui| {
                        badge(
                            ui,
                            &format!("{:?}", change.class).to_uppercase(),
                            CORE_ORANGE,
                        );
                        ui.label(&change.detail);
                    });
                }
            }
            if let Some(receipt) = &step.receipt {
                key_value(
                    ui,
                    "Receipt",
                    &format!(
                        "{} {} — {:?}, exit {}, {} ms",
                        receipt.workspace_path,
                        receipt.sha256,
                        receipt.status,
                        receipt
                            .exit_status
                            .map_or("none".to_string(), |code| code.to_string()),
                        receipt.duration_ms
                    ),
                );
            }
            if !step.inputs.is_empty() {
                egui::CollapsingHeader::new(format!("{} staged inputs", step.inputs.len()))
                    .id_salt(("inputs", &step.step_id))
                    .show(ui, |ui| {
                        egui::Grid::new(("inputs-grid", &step.step_id))
                            .striped(true)
                            .show(ui, |ui| {
                                for input in &step.inputs {
                                    ui.label(&input.input_slot);
                                    ui.label(
                                        egui::RichText::new(&input.evidence_id).color(muted(ui)),
                                    );
                                    ui.label(&input.workspace_path);
                                    integrity_badge(ui, input.integrity);
                                    ui.end_row();
                                }
                            });
                    });
            }
            for output in &step.outputs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(&output.workspace_path).strong());
                    match output.reproduces_bound_artifact {
                        Some(true) => badge(ui, "REPRODUCES BOUND IDENTITY", GREEN),
                        Some(false) => badge(ui, "DIFFERS FROM BOUND IDENTITY", RED),
                        None => {}
                    }
                });
                ui.label(
                    egui::RichText::new(output.sha256.as_deref().unwrap_or("missing"))
                        .color(muted(ui))
                        .size(11.0),
                );
            }
            if let Some(verification) = &step.verification {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Receipt verification:").strong());
                    match verification.state {
                        avila_core_evidence::ReceiptCheckState::Verified => {
                            badge(ui, "VERIFIED FROM BYTES", GREEN);
                        }
                        avila_core_evidence::ReceiptCheckState::Failed => badge(ui, "FAILED", RED),
                    }
                });
            }
            if let Some(replay) = &step.replay {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Committed receipt:").strong());
                    if replay.matches {
                        badge(ui, "MATCH", GREEN);
                    } else {
                        badge(ui, "DRIFT", RED);
                        ui.label(replay.differences.join("; "));
                    }
                });
            }
            for issue in &step.issues {
                ui.colored_label(RED, issue);
            }
        });
        ui.add_space(6.0);
    }
    for step in &execution.not_executed {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&step.step_id).strong());
            badge(ui, "NOT EXECUTED", muted(ui));
            ui.label(egui::RichText::new(&step.reason).color(muted(ui)));
        });
    }
}

fn show_claims(ui: &mut egui::Ui, report: &CaseRunReport) {
    section_heading(
        ui,
        "Generated claims",
        "Input attestations from package identities; claims from executed or reused outputs; recorded attestations carried for steps that did not run.",
    );
    let Some(claims) = &report.claims else {
        ui.label("No claims were generated because an earlier gate did not pass.");
        return;
    };
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Committed claims.json:").strong());
            if claims.matches_committed {
                badge(ui, "MATCH", GREEN);
            } else {
                badge(ui, "MISMATCH", RED);
            }
        });
        key_value(ui, "Generated identity", &claims.generated_sha256);
        key_value(ui, "Committed identity", &claims.committed_sha256);
        key_value(
            ui,
            "Input attestations",
            &claims.input_attestations.to_string(),
        );
        key_value(
            ui,
            "Claims from executed outputs",
            &claims.executed_claims.to_string(),
        );
        key_value(
            ui,
            "Claims from reused outputs",
            &claims.reused_claims.to_string(),
        );
        key_value(
            ui,
            "Recorded claims carried",
            &claims.recorded_claims.to_string(),
        );
        key_value(ui, "Review decisions", &claims.decisions.to_string());
    });
    if let Some(bindings) = &report.bindings {
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Identity binding:").strong());
                match bindings.status {
                    BindingStatus::Verified => badge(ui, "VERIFIED", GREEN),
                    BindingStatus::Failed => badge(ui, "FAILED", RED),
                }
                ui.label(format!(
                    "{}/{} evidence identities; {}/{} review-policy identities",
                    bindings.bound_evidence_records,
                    bindings.evidence_records,
                    bindings.bound_review_policies,
                    bindings.required_review_policies
                ));
            });
            for issue in &bindings.issues {
                ui.colored_label(RED, issue);
            }
        });
    }
}

fn show_verdicts(ui: &mut egui::Ui, report: &CaseRunReport) {
    section_heading(
        ui,
        "Requirement verdicts",
        "Each verdict is a conditional derivation from admitted records under the named profile. It is not scientific truth, certification, or approval.",
    );
    let Some(campaign) = &report.campaign else {
        ui.label("No verdict was produced because an earlier gate did not pass.");
        return;
    };
    let admitted = campaign
        .admissions
        .iter()
        .filter(|record| record.state == avila_core_compiler::AdmissionState::Admitted)
        .count();
    key_value(
        ui,
        "Admitted evidence records",
        &format!("{admitted}/{}", campaign.admissions.len()),
    );
    for verdict in &campaign.verdicts {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(&verdict.requirement_id)
                        .size(17.0)
                        .strong(),
                );
                verdict_badge(ui, verdict.verdict.status);
                ui.label(egui::RichText::new(&verdict.verdict.rule).color(muted(ui)));
            });
            ui.label(&verdict.statement);
            let output = serde_json::to_value(&verdict.verdict).unwrap_or_default();
            for field in [
                "limit_canonical",
                "lower_canonical",
                "upper_canonical",
                "nominal_canonical",
                "canonical_unit",
                "numbers_present",
            ] {
                if let Some(value) = output.get(field).filter(|value| !value.is_null()) {
                    key_value(ui, field, &compact(value));
                }
            }
            if let Some(reasons) = output.get("reasons").and_then(|value| value.as_array())
                && !reasons.is_empty()
            {
                key_value(
                    ui,
                    "reasons",
                    &reasons.iter().map(compact).collect::<Vec<_>>().join("; "),
                );
            }
            if let Some(reviews) = output
                .get("reviews_outstanding")
                .and_then(|value| value.as_array())
                && !reviews.is_empty()
            {
                key_value(
                    ui,
                    "reviews outstanding",
                    &reviews.iter().map(compact).collect::<Vec<_>>().join("; "),
                );
            }
            key_value(ui, "Evidence", &verdict.evidence_ids.join(", "));
            egui::CollapsingHeader::new("Boundary")
                .id_salt(("boundary", &verdict.requirement_id))
                .show(ui, |ui| {
                    key_value(ui, "Semantic profile", &verdict.boundary.semantic_profile);
                    key_value(ui, "Compiler", &verdict.boundary.compiler);
                    key_value(ui, "Evaluator", &verdict.boundary.evaluator);
                    key_value(ui, "Snapshot", &verdict.boundary.compiled_snapshot_sha256);
                    key_value(ui, "Claims", &verdict.boundary.claims_sha256);
                    key_value(
                        ui,
                        "Review attestation",
                        &format!("{:?}", verdict.boundary.review_attestation).to_lowercase(),
                    );
                });
        });
        ui.add_space(6.0);
    }
    if let Some(identity) = &campaign.campaign_sha256 {
        key_value(ui, "Campaign identity", identity);
    }
    if let Some(replay) = &report.replay {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Committed campaign report:").strong());
            if replay.matches {
                badge(ui, "MATCH", GREEN);
            } else {
                badge(ui, "MISMATCH", RED);
            }
        });
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(&campaign.notice)
            .color(muted(ui))
            .size(11.0),
    );
}

fn compact(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_configure_the_setup_and_reject_unknown_flags() {
        let setup = CaseSetup::from_arguments(&[
            "--case".into(),
            "cases/x".into(),
            "--source-root".into(),
            "aftermatter=/tmp/a".into(),
            "--capability".into(),
            "python3=/usr/bin/python3".into(),
            "--no-reuse".into(),
        ])
        .unwrap();
        assert_eq!(setup.case_dir, "cases/x");
        assert_eq!(setup.source_roots[0].name, "aftermatter");
        assert_eq!(setup.capabilities[0].path, "/usr/bin/python3");
        assert!(!setup.reuse);
        assert_eq!(setup.auto_run, None);
        let automatic = CaseSetup::from_arguments(&[
            "--auto-plan".into(),
            "--screenshot".into(),
            "x.png".into(),
        ])
        .unwrap();
        assert_eq!(automatic.auto_run, Some(true));
        assert_eq!(automatic.screenshot.as_deref(), Some("x.png"));
        assert_eq!(CaseTab::by_name("verdicts"), Some(CaseTab::Verdicts));
        assert_eq!(display("A → B"), "A -> B");
        assert!(CaseSetup::from_arguments(&["--bogus".into()]).is_err());
        assert!(CaseSetup::from_arguments(&["--source-root".into(), "nope".into()]).is_err());
    }

    #[test]
    fn the_committed_case_names_its_roots_and_capabilities() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/cases/case-000-actinv-aftermatter"
        );
        let manifest = inspect_case(dir).unwrap();
        let mut setup = CaseSetup::default();
        setup.absorb_manifest(&manifest);
        let roots: Vec<&str> = setup
            .source_roots
            .iter()
            .map(|root| root.name.as_str())
            .collect();
        assert_eq!(roots, ["aftermatter", "actinv-data", "actinv-release"]);
        let capabilities: Vec<&str> = setup
            .capabilities
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(capabilities, ["python3", "aftermatter-cli"]);
        assert!(setup.options(true).plan_only);
        assert!(
            setup.options(false).source_roots.is_empty(),
            "empty paths are not options"
        );
    }
}
