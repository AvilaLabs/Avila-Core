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
    AttemptLineageRequest, BindingStatus, CapabilityCandidate, CapabilityCheckState,
    CaseRunOptions, CaseRunReport, CaseRunStatus, ExecutionStatus, PresentationGateReadiness,
    SCAN_LIMIT, SignatureStatus, StepExecutionState, candidates_on_path, execute_case,
    human_summary, probe_capability, scan_dir,
};
use eframe::egui;

use crate::help::{HelpTab, TourTarget, TourTargets};
use crate::{CORE_ORANGE, badge, card, key_value, muted, section_heading, show_finding};

const GREEN: egui::Color32 = egui::Color32::from_rgb(95, 197, 128);
const RED: egui::Color32 = egui::Color32::from_rgb(232, 102, 102);
const AMBER: egui::Color32 = egui::Color32::from_rgb(230, 170, 70);
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedPath {
    pub name: String,
    pub path: String,
}

/// Where a file-picker result should land.
enum PickerTarget {
    /// Fill row `index` of the source roots (folder) or capabilities (file).
    Location { folder: bool, index: usize },
    /// Scan the picked folder's files against every declared capability.
    CapabilityScan,
}

/// Probe work handed to a background thread. Hash-only: candidates are
/// never executed, and the check/run path verifies the chosen bytes again.
enum ProbeJob {
    /// Hash one capability row's current path (name, path text).
    Typed(String, String),
    /// Search PATH for every declared capability's name.
    OnPath,
    /// Scan a folder once and offer its files to every capability.
    ScanDir(PathBuf),
}

/// What a finished probe reports back to the view.
enum ProbeReport {
    /// The row's typed path, hashed on demand; carries the text probed so a
    /// later edit is not mistaken for this result.
    Typed {
        name: String,
        text: String,
        candidate: Option<CapabilityCandidate>,
    },
    /// Candidates a PATH or folder search found, per capability.
    Found {
        source: String,
        outcomes: Vec<(String, Vec<CapabilityCandidate>)>,
    },
}

/// Latest probe outcome for one capability row.
#[derive(Default)]
struct ProbeRow {
    /// An on-demand check of the row's path: the text probed and its result.
    typed: Option<(String, CapabilityCandidate)>,
    /// Candidates a PATH or folder search found: a source label and the
    /// probed files.
    found: Option<(String, Vec<CapabilityCandidate>)>,
}

/// Run a probe job against the capabilities `manifest` declares. Hash-only
/// and read-only: candidates are hashed, never executed.
fn run_probe(job: ProbeJob, manifest: &CasePackageManifest) -> ProbeReport {
    match job {
        ProbeJob::Typed(name, text) => {
            let candidate = manifest
                .capabilities
                .iter()
                .find(|declared| declared.capability_id == name)
                .and_then(|declared| {
                    probe_capability(declared, &[PathBuf::from(&text)])
                        .into_iter()
                        .next()
                });
            ProbeReport::Typed {
                name,
                text,
                candidate,
            }
        }
        ProbeJob::OnPath => ProbeReport::Found {
            source: "PATH".into(),
            outcomes: manifest
                .capabilities
                .iter()
                .map(|declared| {
                    let found = candidates_on_path(&declared.capability_id);
                    (
                        declared.capability_id.clone(),
                        probe_capability(declared, &found),
                    )
                })
                .collect(),
        },
        ProbeJob::ScanDir(dir) => {
            let scanned = scan_dir(&dir, SCAN_LIMIT);
            ProbeReport::Found {
                source: format!("scan of {}", dir.display()),
                outcomes: manifest
                    .capabilities
                    .iter()
                    .map(|declared| {
                        (
                            declared.capability_id.clone(),
                            probe_capability(declared, &scanned),
                        )
                    })
                    .collect(),
            }
        }
    }
}

/// The probe lines under one capability row: the on-demand check of the
/// row's path, the latest PATH/folder search outcome, and the actions that
/// queue a background hash job. A found match is only offered, never
/// applied silently.
fn capability_probe_row(
    ui: &mut egui::Ui,
    probes: &mut BTreeMap<String, ProbeRow>,
    probing: bool,
    row: &mut NamedPath,
    probe_job: &mut Option<ProbeJob>,
) {
    let probe = probes.get(&row.name);
    if let Some((text, candidate)) = probe.and_then(|probe| probe.typed.as_ref()) {
        if text == row.path.trim() {
            let (label, color) = match candidate.state {
                CapabilityCheckState::Verified => {
                    ("This file matches the pinned program.".to_string(), GREEN)
                }
                CapabilityCheckState::Mismatch => (
                    "This file's bytes differ from the pinned program.".to_string(),
                    RED,
                ),
                CapabilityCheckState::Missing => {
                    ("This path is not a readable file.".to_string(), AMBER)
                }
                CapabilityCheckState::NotSupplied => {
                    ("This file was not supplied.".to_string(), muted(ui))
                }
            };
            ui.colored_label(color, label);
        } else {
            ui.small("Location changed since it was last checked.");
        }
    }
    // Extract the search outcome as owned data so the borrow on `probes`
    // ends before the Use button below mutates it.
    let found = probe
        .and_then(|probe| probe.found.as_ref())
        .map(|(source, candidates)| {
            let verified: Vec<CapabilityCandidate> = candidates
                .iter()
                .filter(|candidate| candidate.state == CapabilityCheckState::Verified)
                .cloned()
                .collect();
            (source.clone(), candidates.len(), verified)
        });
    if let Some((source, count, verified)) = found {
        let verified_count = verified.len();
        if let Some(first) = verified.into_iter().next() {
            let mut message = format!("{source} found {}", first.path.display());
            if verified_count > 1 {
                message += &format!(" (+{} more)", verified_count - 1);
            }
            ui.horizontal(|ui| {
                ui.small(message);
                if ui
                    .small_button("Use this program")
                    .on_hover_text(
                        "Fill this row with the found path; check or run verifies its bytes again.",
                    )
                    .clicked()
                {
                    row.path = first.path.display().to_string();
                    probes.entry(row.name.clone()).or_default().typed =
                        Some((row.path.trim().to_string(), first));
                }
            });
        } else if count == 0 {
            ui.small(format!("{source} offered no candidates."));
        } else {
            ui.small(format!(
                "{source} offered {count} candidate(s); none matches the pinned identity."
            ));
        }
    }
    if !row.path.trim().is_empty()
        && ui
            .add_enabled(!probing, egui::Button::new("Check file").small())
            .on_hover_text("Hash this file and compare it with the capability's pinned identity. The file is not executed; check or run verifies it again.")
            .clicked()
    {
        *probe_job = Some(ProbeJob::Typed(
            row.name.clone(),
            row.path.trim().to_string(),
        ));
    }
}

/// Launch-time configuration, from the command line or the package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseSetup {
    pub case_dir: String,
    /// Open the corresponding panel at launch, also useful for native UI review.
    pub show_setup: bool,
    pub show_help: bool,
    pub source_roots: Vec<NamedPath>,
    pub capabilities: Vec<NamedPath>,
    /// Free inputs of the case: input id and the file supplied for it.
    pub free_inputs: Vec<NamedPath>,
    /// Environment keys an execution requires and the values supplied.
    pub environment: Vec<NamedPath>,
    pub workspace: String,
    pub reuse: bool,
    /// Operator-owned JSON file caching verified digests of large artifacts
    /// resolved under a source root. Empty means off, the default.
    pub hash_cache: String,
    /// Optional campaign history appended by the runner for each run or plan.
    pub log: String,
    /// The requester and runner public keys this run accepts (ADR-0015).
    /// Empty means no trust root: every signature is reported `unsigned` or
    /// `signature not checked`, never `verified`, and a contract that sets
    /// `execution_policy.require_signatures` refuses the run outright.
    pub trust_root: String,
    /// A runner seed key (32 raw bytes). Empty means a freshly executed
    /// step's receipt is written unsigned.
    pub runner_key: String,
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
    /// Open the specimen compiler at a named workspace (a development aid
    /// with `--screenshot`).
    pub specimen_workspace: Option<String>,
    /// Open the query workspace with a saved report or campaign log.
    pub tools_path: Option<String>,
    /// Query name to select in the Tools workspace.
    pub tool: Option<String>,
    /// Open the History view with this campaign log.
    pub history_path: Option<String>,
    /// Preselect an attempt ID (or `line:N`) in the History view (a
    /// development aid with `--screenshot`).
    pub history_select: Option<String>,
    /// Record this run as a named attempt in the campaign log's lineage.
    /// Empty means an ordinary run with no attempt record.
    pub attempt_id: String,
    /// The earlier attempt in the same log this run descends from.
    pub parent_attempt_id: String,
    /// The free input the lineage snapshots as its candidate. Empty means
    /// `candidate`, matching the CLI's `--attempt` convention.
    pub candidate_input: String,
    /// The design revision this run is evidence for (ADR-0019). Empty
    /// means the run cites none.
    pub revision_id: String,
    /// The contract amendment a new root attempt cites when it
    /// deliberately continues a case under changed fixed identities.
    pub amendment_id: String,
}

impl CaseSetup {
    /// Parse `--case DIR`, `--source-root NAME=PATH`, `--capability NAME=PATH`,
    /// and `--workspace DIR` from the process arguments. Unknown arguments
    /// are reported, never ignored.
    pub fn from_arguments(arguments: &[String]) -> Result<Self, String> {
        let mut setup = Self {
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
                "--show-setup" => setup.show_setup = true,
                "--show-help" => setup.show_help = true,
                "--workspace" => setup.workspace = value()?,
                "--hash-cache" => setup.hash_cache = value()?,
                "--log" => setup.log = value()?,
                "--trust-root" => setup.trust_root = value()?,
                "--runner-key" => setup.runner_key = value()?,
                "--source-root" => setup.source_roots.push(named_path(&value()?)?),
                "--capability" => setup.capabilities.push(named_path(&value()?)?),
                "--input" => setup.free_inputs.push(named_path(&value()?)?),
                "--env" => setup.environment.push(named_path(&value()?)?),
                "--no-reuse" => setup.reuse = false,
                "--auto-run" => setup.auto_run = Some(false),
                "--auto-plan" => setup.auto_run = Some(true),
                "--screenshot" => setup.screenshot = Some(value()?),
                "--tab" => setup.tab = Some(value()?),
                "--light" => setup.light = true,
                "--tour" => setup.tour = Some(value()?),
                "--specimen" => setup.specimen_workspace = Some(value()?),
                "--tools" => setup.tools_path = Some(value()?),
                "--history" => setup.history_path = Some(value()?),
                "--history-select" => setup.history_select = Some(value()?),
                "--attempt" => setup.attempt_id = value()?,
                "--parent-attempt" => setup.parent_attempt_id = value()?,
                "--candidate-input" => setup.candidate_input = value()?,
                "--revision" => setup.revision_id = value()?,
                "--amendment" => setup.amendment_id = value()?,
                "--tool" => {
                    let name = value()?;
                    if crate::tools_view::Tool::by_name(&name).is_none() {
                        return Err(format!("unknown tool `{name}`"));
                    }
                    setup.tool = Some(name);
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        if setup.auto_run.is_some() && setup.case_dir.trim().is_empty() {
            return Err("--auto-run and --auto-plan require --case DIR".into());
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
        for input_id in &manifest.free_inputs {
            if !self.free_inputs.iter().any(|entry| &entry.name == input_id) {
                self.free_inputs.push(NamedPath {
                    name: input_id.clone(),
                    path: String::new(),
                });
            }
        }
        for key in manifest
            .executions
            .iter()
            .flat_map(|execution| execution.environment.iter())
        {
            if !self.environment.iter().any(|entry| &entry.name == key) {
                self.environment.push(NamedPath {
                    name: key.clone(),
                    path: String::new(),
                });
            }
        }
    }

    /// Fill empty source-root rows from locations this build can offer —
    /// `case` names the case's own folder, and a bundled example's other
    /// roots resolve against the shipped sibling trees. Typed and restored
    /// locations are never overwritten; a check or run still verifies every
    /// supplied path against the package's pinned identities.
    pub fn prefill_bundled(&mut self) {
        let case_dir = PathBuf::from(self.case_dir.trim());
        for row in &mut self.source_roots {
            if row.path.trim().is_empty()
                && let Some(path) = crate::case_browser::bundled_root(&case_dir, &row.name)
            {
                row.path = path.display().to_string();
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
            inputs: paths(&self.free_inputs),
            environment: self
                .environment
                .iter()
                .filter(|row| !row.name.trim().is_empty() && !row.path.trim().is_empty())
                .map(|row| (row.name.trim().to_string(), row.path.trim().to_string()))
                .collect(),
            workspace: (!self.workspace.trim().is_empty())
                .then(|| PathBuf::from(self.workspace.trim())),
            reuse: self.reuse,
            plan_only,
            hash_cache: (!self.hash_cache.trim().is_empty())
                .then(|| PathBuf::from(self.hash_cache.trim())),
            log: (!self.log.trim().is_empty()).then(|| PathBuf::from(self.log.trim())),
            gate_respond_by: None,
            trust_root: (!self.trust_root.trim().is_empty())
                .then(|| PathBuf::from(self.trust_root.trim())),
            runner_key: (!self.runner_key.trim().is_empty())
                .then(|| PathBuf::from(self.runner_key.trim())),
            // The runner owns every lineage rule; this only assembles the
            // explicit request the CLI's --attempt flags assemble.
            attempt: (!self.attempt_id.trim().is_empty()).then(|| AttemptLineageRequest {
                attempt_id: self.attempt_id.trim().to_string(),
                parent_attempt_id: (!self.parent_attempt_id.trim().is_empty())
                    .then(|| self.parent_attempt_id.trim().to_string()),
                candidate_input: if self.candidate_input.trim().is_empty() {
                    "candidate".into()
                } else {
                    self.candidate_input.trim().to_string()
                },
                revision_id: (!self.revision_id.trim().is_empty())
                    .then(|| self.revision_id.trim().to_string()),
                amendment_id: (!self.amendment_id.trim().is_empty())
                    .then(|| self.amendment_id.trim().to_string()),
            }),
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
#[cfg(test)]
fn inspect_case(case_dir: &str) -> Result<CasePackageManifest, String> {
    crate::case_browser::CaseInfo::read(std::path::Path::new(case_dir)).map(|info| info.manifest)
}

pub struct CaseView {
    pub setup: CaseSetup,
    pub info: Option<crate::case_browser::CaseInfo>,
    pub show_setup: bool,
    picker: crate::case_browser::FilePicker,
    picker_target: Option<PickerTarget>,
    /// A background hash-only probe, if one is running.
    probing: Option<Receiver<ProbeReport>>,
    /// Latest probe outcome per capability name.
    probes: BTreeMap<String, ProbeRow>,
    tab: CaseTab,
    manifest: Option<CasePackageManifest>,
    inspect_error: Option<String>,
    running: Option<Receiver<Result<CaseRunReport, String>>>,
    last_plan_only: bool,
    report_setup: Option<CaseSetup>,
    pending_setup: Option<CaseSetup>,
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
        let (info, inspect_error) = if setup.case_dir.is_empty() {
            (None, None)
        } else {
            match crate::case_browser::CaseInfo::read(std::path::Path::new(&setup.case_dir)) {
                Ok(info) => {
                    setup.case_dir = info.path.display().to_string();
                    setup.absorb_manifest(&info.manifest);
                    setup.prefill_bundled();
                    (Some(info), None)
                }
                Err(error) => (None, Some(error)),
            }
        };
        let manifest = info.as_ref().map(|info| info.manifest.clone());
        let tab = setup
            .tab
            .as_deref()
            .and_then(CaseTab::by_name)
            .unwrap_or_default();
        Self {
            info,
            show_setup: setup.show_setup,
            picker: Default::default(),
            picker_target: None,
            probing: None,
            probes: BTreeMap::new(),
            setup,
            tab,
            manifest,
            inspect_error,
            running: None,
            last_plan_only: false,
            report_setup: None,
            pending_setup: None,
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
    pub fn drive_screenshot(&mut self, context: &egui::Context, ready: bool) {
        let Some(path) = self.setup.screenshot.clone() else {
            return;
        };
        if !ready {
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

    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    pub fn select_case(
        &mut self,
        info: crate::case_browser::CaseInfo,
        saved: Option<&crate::case_browser::RecentCase>,
    ) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the current run to finish before opening another case.".into());
        }
        let mut setup = CaseSetup {
            case_dir: info.path.display().to_string(),
            reuse: true,
            screenshot: self.setup.screenshot.clone(),
            light: self.setup.light,
            ..Default::default()
        };
        setup.absorb_manifest(&info.manifest);
        if let Some(saved) = saved {
            for (rows, previous) in [
                (&mut setup.source_roots, &saved.roots),
                (&mut setup.capabilities, &saved.capabilities),
            ] {
                for row in rows {
                    if let Some(old) = previous.iter().find(|old| old.name == row.name) {
                        row.path.clone_from(&old.path);
                    }
                }
            }
        }
        *self = Self::new(setup);
        self.manifest = Some(info.manifest.clone());
        self.inspect_error = None;
        self.info = Some(info);
        Ok(())
    }

    pub fn recent(&self) -> Option<crate::case_browser::RecentCase> {
        let info = self.info.as_ref()?;
        Some(crate::case_browser::RecentCase {
            path: info.path.clone(),
            title: info.manifest.title.clone(),
            roots: self.setup.source_roots.clone(),
            capabilities: self.setup.capabilities.clone(),
        })
    }

    /// Dispatch a hash-only capability probe to a background thread. The
    /// probe never executes a candidate; the check/run path verifies the
    /// chosen bytes again.
    fn start_probe(&mut self, job: ProbeJob) {
        if self.probing.is_some() {
            return;
        }
        let Some(manifest) = self.manifest.clone() else {
            return;
        };
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let _ = sender.send(run_probe(job, &manifest));
        });
        self.probing = Some(receiver);
    }

    fn poll_probe(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.probing else {
            return;
        };
        match receiver.try_recv() {
            Ok(report) => {
                self.apply_probe(report);
                self.probing = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(120));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.probing = None;
            }
        }
    }

    fn apply_probe(&mut self, report: ProbeReport) {
        match report {
            ProbeReport::Typed {
                name,
                text,
                candidate,
            } => {
                if let Some(candidate) = candidate {
                    self.probes.entry(name).or_default().typed = Some((text, candidate));
                }
            }
            ProbeReport::Found { source, outcomes } => {
                for (name, candidates) in outcomes {
                    self.probes.entry(name).or_default().found = Some((source.clone(), candidates));
                }
            }
        }
    }

    fn start(&mut self, plan_only: bool) {
        if self.running.is_some() || self.info.is_none() {
            return;
        }
        let case_dir = PathBuf::from(self.setup.case_dir.trim());
        let options = self.setup.options(plan_only);
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let outcome = execute_case(&case_dir, &options).map_err(|error| error.to_string());
            let _ = sender.send(outcome);
        });
        self.pending_setup = Some(self.setup.clone());
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
                self.report_setup = self.pending_setup.take();
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
        let actions = ui.horizontal_wrapped(|ui| {
            if let Some(info) = &self.info { ui.strong(display(&info.manifest.title)); }
            ui.toggle_value(&mut self.show_setup, "Machine setup").on_hover_text("Choose where this case's data and programs live on this computer.");
            if ui.add_enabled(!self.busy() && self.info.is_some(), egui::Button::new("Check setup")).on_hover_text("Runs Core's plan operation: checks current inputs and reuse without launching solver steps.").clicked() { self.start(true); }
            if ui.add_enabled(!self.busy() && self.info.is_some(), egui::Button::new("Run case")).on_hover_text("Execute the case using these locations; reuse verified receipts where possible.").clicked() { self.start(false); }
            if self.busy() {
                ui.spinner();
                ui.label(format!("{} {:.1} s", if self.last_plan_only { "Checking setup…" } else { "Running case…" }, self.started.map_or(0.0, |started| started.elapsed().as_secs_f32())));
            } else if let Some(duration) = self.last_duration {
                ui.small(format!("Completed in {:.2} s", duration.as_secs_f32()));
            }
        });
        targets.set(TourTarget::RunButtons, actions.response.rect);
        if self.show_setup {
            egui::Panel::left("case-setup")
                .resizable(true)
                .default_size(370.0)
                .max_size(480.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.busy(), |ui| self.setup_panel(ui, targets));
                        });
                });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            if let Some(error) = &self.run_error { ui.colored_label(RED, error); }
            if self.report.is_some() {
                if self.busy() { ui.label("Showing the previous report while the new request completes."); }
                else if self.report_setup.as_ref().is_some_and(|setup| setup != &self.setup) {
                    ui.colored_label(AMBER, "Setup has changed. This report belongs to the previous setup; check again to assess the new locations.");
                }
                if let Some(info) = &self.info { ui.collapsing("Case question", |ui| { ui.label(&info.question); }); }
            }
            if self.report.is_none() {
                egui::ScrollArea::vertical().show(ui, |ui| self.case_overview(ui));
                return;
            }
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

    pub(crate) fn tick(&mut self, context: &egui::Context) {
        if let Some(path) = self.picker.poll()
            && let Some(target) = self.picker_target.take()
        {
            match target {
                PickerTarget::Location { folder, index } => {
                    let rows = if folder {
                        &mut self.setup.source_roots
                    } else {
                        &mut self.setup.capabilities
                    };
                    if let Some(row) = rows.get_mut(index) {
                        row.path = path.display().to_string();
                    }
                }
                PickerTarget::CapabilityScan => self.start_probe(ProbeJob::ScanDir(path)),
            }
        }
        self.start_automatic();
        self.poll(context);
        self.poll_probe(context);
    }

    pub(crate) fn report(&self) -> Option<&CaseRunReport> {
        self.report.as_ref()
    }

    pub(crate) fn settled(&self) -> bool {
        self.running.is_none()
    }

    fn setup_panel(&mut self, ui: &mut egui::Ui, targets: &mut TourTargets) {
        ui.add_space(4.0);
        let case = ui.scope(|ui| {
            ui.heading("Machine setup");
            ui.label("Tell Core where this case's files and programs are on your computer.");
            ui.small("Locations are remembered per case on this computer. Each check or run verifies identities again.");
            ui.collapsing("Case location", |ui| { ui.label(&self.setup.case_dir); });
            if let Some(error) = &self.inspect_error { ui.colored_label(RED, error); }
        });
        targets.set(TourTarget::CaseInput, case.response.rect);
        ui.add_space(8.0);
        let roots = ui.scope(|ui| {
            ui.strong("Data folders");
            ui.small("Choose the folder that contains each named collection of input files.");
            self.location_rows(ui, true);
        });
        targets.set(TourTarget::SourceRoots, roots.response.rect);
        ui.add_space(8.0);
        let capabilities = ui.scope(|ui| {
            ui.strong("Programs");
            ui.small("Select executables to run new steps. Verified saved steps may be reused without them.");
            if !self.setup.capabilities.is_empty() {
                ui.horizontal(|ui| {
                    let idle = self.probing.is_none() && !self.picker.busy();
                    if ui
                        .add_enabled(idle, egui::Button::new("Search PATH").small())
                        .on_hover_text("Hash executables on this computer's PATH named like the declared capabilities against their pinned identities. Nothing is executed.")
                        .clicked()
                    {
                        self.start_probe(ProbeJob::OnPath);
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Scan folder…").small())
                        .on_hover_text("Choose a folder; its files are hashed against every declared capability's pinned identity. Nothing is executed.")
                        .clicked()
                    {
                        self.picker_target = Some(PickerTarget::CapabilityScan);
                        self.picker
                            .start(ui.ctx(), true, "Scan a folder for programs");
                    }
                    if self.probing.is_some() {
                        ui.spinner();
                        ui.small("Hashing candidates…");
                    }
                });
            }
            self.location_rows(ui, false);
        });
        targets.set(TourTarget::Capabilities, capabilities.response.rect);
        if !self.setup.free_inputs.is_empty() {
            ui.add_space(8.0);
            named_values(
                ui,
                "FREE INPUTS",
                "free-inputs",
                &mut self.setup.free_inputs,
                "input id",
                "file to supply for this run (leave empty to use the reference)",
                "REFERENCE",
            );
        }
        if !self.setup.environment.is_empty() {
            ui.add_space(8.0);
            named_values(
                ui,
                "ENVIRONMENT",
                "environment",
                &mut self.setup.environment,
                "key",
                "value (needed only when that step runs)",
                "NOT SUPPLIED",
            );
        }

        ui.add_space(8.0);
        self.attempt_panel(ui);

        ui.add_space(8.0);
        let options = ui.collapsing("Advanced run options", |ui| {
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
            ui.horizontal(|ui| {
                ui.label("Hash cache");
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.hash_cache)
                        .hint_text("off by default; a JSON file to cache large artifact digests in")
                        .desired_width(f32::INFINITY),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Trust root");
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.trust_root)
                        .hint_text(
                            "unset: signatures report unsigned/not checked (ADR-0015 trust-root.json)",
                        )
                        .desired_width(f32::INFINITY),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Runner key");
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.runner_key)
                        .hint_text("unset: a freshly executed step's receipt is written unsigned")
                        .desired_width(f32::INFINITY),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Campaign log");
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.log)
                        .hint_text("Optional campaign.jsonl for the Run history tool")
                        .desired_width(f32::INFINITY),
                );
            });
        });
        targets.set(TourTarget::Options, options.header_response.rect);

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(
                "Omitted roots are reported as not checked and omitted executables as not run. A supplied root or executable that does not match fails closed.",
            )
            .color(muted(ui))
            .size(11.0),
        );
    }

    /// Optional attempt lineage: name the run, the earlier recorded attempt
    /// it descends from, and the free input the lineage snapshots as its
    /// candidate. The fields only assemble `AttemptLineageRequest`; Core owns
    /// every lineage rule and refuses the plan or run when they do not hold.
    fn attempt_panel(&mut self, ui: &mut egui::Ui) {
        ui.strong("Design attempt");
        ui.small("Optional. Record this plan or run as an attempt in the campaign log's lineage. Select an attempt in History to fill its parent; name the new attempt here.");
        ui.horizontal(|ui| {
            ui.label("Attempt ID");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.attempt_id)
                    .hint_text("empty: an ordinary run, no attempt record")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Parent attempt");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.parent_attempt_id)
                    .hint_text("optional attempt ID in the same log")
                    .desired_width(f32::INFINITY),
            );
        });
        let candidate = if self.setup.candidate_input.trim().is_empty() {
            "candidate".to_string()
        } else {
            self.setup.candidate_input.trim().to_string()
        };
        let mut names: Vec<String> = self
            .setup
            .free_inputs
            .iter()
            .map(|row| row.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();
        names.dedup();
        ui.horizontal(|ui| {
            ui.label("Candidate input");
            if names.is_empty() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.setup.candidate_input)
                        .hint_text("free input ID; `candidate` when empty")
                        .desired_width(f32::INFINITY),
                );
            } else {
                egui::ComboBox::from_id_salt("candidate-input")
                    .selected_text(&candidate)
                    .show_ui(ui, |ui| {
                        for name in &names {
                            ui.selectable_value(
                                &mut self.setup.candidate_input,
                                name.clone(),
                                name,
                            );
                        }
                    });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Design revision");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.revision_id)
                    .hint_text("optional revision ID this run is evidence for")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Amendment");
            ui.add(
                egui::TextEdit::singleline(&mut self.setup.amendment_id)
                    .hint_text("roots only: amendment ID for a changed question")
                    .desired_width(f32::INFINITY),
            );
        });
        if self.setup.attempt_id.trim().is_empty() {
            if !self.setup.parent_attempt_id.trim().is_empty()
                || !self.setup.candidate_input.trim().is_empty()
                || !self.setup.revision_id.trim().is_empty()
                || !self.setup.amendment_id.trim().is_empty()
            {
                ui.colored_label(
                    AMBER,
                    "An attempt ID is required for lineage; without it the parent, candidate, revision, and amendment fields are not sent.",
                );
            }
        } else {
            let mut intent = format!(
                "Plan or run will append attempt `{}` to the log",
                self.setup.attempt_id.trim()
            );
            if !self.setup.parent_attempt_id.trim().is_empty() {
                intent += &format!(" descending from `{}`", self.setup.parent_attempt_id.trim());
            }
            if !self.setup.revision_id.trim().is_empty() {
                intent += &format!(
                    ", as evidence for revision `{}`",
                    self.setup.revision_id.trim()
                );
            }
            if !self.setup.amendment_id.trim().is_empty() {
                intent += &format!(", citing amendment `{}`", self.setup.amendment_id.trim());
            }
            intent += &format!(", tracking input `{candidate}`");
            match self
                .setup
                .free_inputs
                .iter()
                .find(|row| row.name.trim() == candidate)
                .map(|row| row.path.trim())
            {
                Some(path) if !path.is_empty() => intent += &format!(" = {path}"),
                _ => intent += " — no file supplied for it yet",
            }
            if self.setup.log.trim().is_empty() {
                intent += ". Lineage requires a campaign log; set Campaign log under Advanced run options";
            }
            ui.add(egui::Label::new(egui::RichText::new(intent).small().color(muted(ui))).wrap());
        }
    }

    fn location_rows(&mut self, ui: &mut egui::Ui, folder: bool) {
        let rows = if folder {
            &mut self.setup.source_roots
        } else {
            &mut self.setup.capabilities
        };
        if rows.is_empty() {
            ui.small("None declared by this case.");
        }
        let mut probe_job = None;
        for (index, row) in rows.iter_mut().enumerate() {
            ui.push_id((folder, index), |ui| {
                ui.add_space(6.0);
                ui.strong(&row.name);
                if folder && row.name == "case" && ui.small_button("Use this case folder").on_hover_text("Use the opened package directory for this data collection; Core checks its contents when you check or run.").clicked() {
                    row.path.clone_from(&self.setup.case_dir);
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut row.path)
                            .hint_text(if folder {
                                "Choose a folder…"
                            } else {
                                "Choose a program…"
                            })
                            .desired_width((ui.available_width() - 110.0).max(80.0)),
                    );
                    if ui
                        .add_enabled(!self.picker.busy(), egui::Button::new("Browse…").wrap_mode(egui::TextWrapMode::Extend))
                        .clicked()
                    {
                        self.picker_target = Some(PickerTarget::Location { folder, index });
                        self.picker
                            .start(ui.ctx(), folder, &format!("Locate {}", row.name));
                    }
                });
                ui.small(if row.path.trim().is_empty() {
                    "No location selected"
                } else {
                    "Location selected · identity checked when you check or run"
                });
                if !folder {
                    capability_probe_row(
                        ui,
                        &mut self.probes,
                        self.probing.is_some(),
                        row,
                        &mut probe_job,
                    );
                }
            });
        }
        if let Some(job) = probe_job {
            self.start_probe(job);
        }
    }

    fn case_overview(&mut self, ui: &mut egui::Ui) {
        let Some(info) = &self.info else {
            ui.heading("Open a case to begin");
            ui.label(
                "Choose Cases above to browse examples or open a folder containing package.json.",
            );
            if let Some(error) = &self.inspect_error {
                ui.colored_label(RED, error);
            }
            return;
        };
        ui.add_space(16.0);
        ui.small(format!(
            "{} · Package preview · Not yet checked",
            info.manifest.case_id
        ));
        ui.heading("The question this case answers");
        ui.add_space(8.0);
        ui.label(egui::RichText::new(&info.question).size(19.0));
        if let Some(error) = &info.metadata_error {
            ui.colored_label(RED, format!("Could not preview the question: {error}"));
        }
        ui.add_space(20.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("What happens next");
            ui.label("1. Open Machine setup to locate any data folders and programs you have.");
            ui.label("2. Check setup to see what Core can reuse and what is missing. This does not launch solver steps.");
            ui.label("3. Run case when you are ready to execute. Review the report and requirement verdicts afterward.");
            ui.small("You can check with incomplete setup. Core reports missing inputs explicitly; opening this preview does not verify the evidence.");
        });
        ui.add_space(14.0);
        ui.collapsing(
            format!("Requirements ({})", info.requirements.len()),
            |ui| {
                for (id, statement) in &info.requirements {
                    ui.strong(id);
                    ui.label(statement);
                    ui.add_space(6.0);
                }
                if info.requirements.is_empty() {
                    ui.label("No requirements available in this preview.");
                }
            },
        );
        ui.collapsing("What's inside this case", |ui| {
            show_manifest(ui, &info.manifest);
        });
        ui.collapsing("Assumptions and limitations", |ui| {
            for assumption in &info.assumptions { ui.label(assumption); }
            for limitation in &info.manifest.limitations { ui.label(limitation); }
            if info.manifest.limitations.is_empty() { ui.label("No package-level limitations declared. Review the contract and evidence before interpreting a result."); }
        });
        ui.collapsing("Case location", |ui| {
            ui.label(info.path.display().to_string());
        });
        if let Some(error) = &self.run_error {
            ui.colored_label(RED, error);
        }
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
            CaseTab::Integrity => show_integrity(ui, report, self.manifest.as_ref()),
            CaseTab::Compile => {
                let sources: Vec<(&str, &[u8])> = self
                    .sources
                    .iter()
                    .map(|(document, bytes)| (document.as_str(), bytes.as_slice()))
                    .collect();
                show_compile(ui, report.compile.as_ref(), &sources);
                show_coverage(ui, report);
            }
            CaseTab::Execute => show_execute(ui, report),
            CaseTab::Claims => show_claims(ui, report),
            CaseTab::Verdicts => show_verdicts(ui, report),
        }
    }
}

fn named_values(
    ui: &mut egui::Ui,
    title: &str,
    id: &str,
    rows: &mut Vec<NamedPath>,
    name_hint: &str,
    value_hint: &str,
    empty_badge: &str,
) {
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
                        .hint_text(name_hint)
                        .desired_width(170.0),
                );
                if ui.small_button("×").on_hover_text("remove").clicked() {
                    remove = Some(index);
                }
                if row.path.trim().is_empty() {
                    badge(ui, empty_badge, muted(ui));
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut row.path)
                    .hint_text(value_hint)
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
        IntegrityCheckState::VerifiedCached => badge(ui, "VERIFIED (CACHED)", BLUE),
        IntegrityCheckState::NotChecked => badge(ui, "NOT CHECKED", muted(ui)),
        IntegrityCheckState::Missing => badge(ui, "MISSING", RED),
        IntegrityCheckState::Mismatch => badge(ui, "MISMATCH", RED),
    }
}

/// A document's ADR-0015 signature status, next to its byte-integrity
/// badge: a signature proves possession of a key at signing time, not the
/// correctness of what was signed, so this is reported alongside identity,
/// never in place of it.
/// `detail` adds the signing key id (or refusal reason) next to the badge;
/// pass `false` in a narrow grid cell, where that text would only push the
/// column wide enough to crowd out the ones after it.
fn signature_status_badge(ui: &mut egui::Ui, status: Option<&SignatureStatus>, detail: bool) {
    match status {
        None => {
            ui.label(egui::RichText::new("—").color(muted(ui)));
        }
        Some(SignatureStatus::Unsigned) => badge(ui, "UNSIGNED", muted(ui)),
        Some(SignatureStatus::NotChecked) => badge(ui, "NOT CHECKED", AMBER),
        Some(SignatureStatus::Verified { signed_by }) => {
            if detail {
                ui.horizontal(|ui| {
                    badge(ui, "VERIFIED", GREEN);
                    ui.label(
                        egui::RichText::new(format!(
                            "by {}…",
                            &signed_by[..signed_by.len().min(12)]
                        ))
                        .color(muted(ui))
                        .size(10.0),
                    );
                });
            } else {
                badge(ui, "VERIFIED", GREEN);
            }
        }
        Some(SignatureStatus::Invalid { reason }) => {
            if detail {
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "INVALID", RED);
                    ui.label(egui::RichText::new(reason).color(muted(ui)).size(10.0));
                });
            } else {
                badge(ui, "INVALID", RED);
            }
        }
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
            .filter(|check| {
                matches!(
                    check.state,
                    IntegrityCheckState::Verified | IntegrityCheckState::VerifiedCached
                )
            })
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
                    compiled.requirements.len() + compiled.categorical_requirements.len()
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
        if let Some(coverage) = &report.coverage {
            let complete = coverage.status == avila_core_compiler::CoverageStatus::Complete;
            stage_row(
                ui,
                "2b. Coverage",
                if complete { "COMPLETE" } else { "INCOMPLETE" },
                if complete { GREEN } else { RED },
                &format!(
                    "{} covered, {} omitted with a stated reason, {} omissible, {} unstated, {} under basis; set {} rev {}",
                    coverage.count(avila_core_compiler::CoverageState::Covered),
                    coverage.count(avila_core_compiler::CoverageState::OmittedStated),
                    coverage.count(avila_core_compiler::CoverageState::Omissible),
                    coverage.count(avila_core_compiler::CoverageState::OmittedUnstated),
                    coverage.count(avila_core_compiler::CoverageState::CoveredUnderBasis),
                    coverage.set_id,
                    coverage.set_revision
                ),
            );
        }
        if let Some(attempt) = &report.attempt {
            let mut detail = format!(
                "`{}` generation {}{}; candidate input `{}`",
                attempt.attempt_id,
                attempt.generation,
                attempt
                    .parent_attempt_id
                    .as_ref()
                    .map(|parent| format!(", descends from `{parent}`"))
                    .unwrap_or_default(),
                attempt.candidate_input
            );
            if let Some(comparison) = &report.attempt_comparison {
                detail += &format!(
                    "; vs parent {} verdict transition(s), {} exact margin delta(s), {} unavailable",
                    comparison.verdict_transitions.len(),
                    comparison.exact_margin_comparisons.len(),
                    comparison.verdict_comparison_unavailable.len()
                        + comparison.margin_comparison_unavailable.len()
                );
            }
            stage_row(ui, "2c. Attempt", "RECORDED", BLUE, &detail);
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
            None if !report.replay_applicable => stage_row(
                ui,
                "6. Replay",
                "NOT APPLICABLE",
                muted(ui),
                "free input(s) supplied; the committed expectations describe the reference input",
            ),
            None => stage_row(ui, "6. Replay", "NOT RUN", muted(ui), ""),
        }
        for input in &report.supplied_inputs {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Supplied input").strong());
                ui.label(
                    egui::RichText::new(format!(
                        "`{}` = {} ({})",
                        input.input_id, input.path, input.sha256
                    ))
                    .color(muted(ui)),
                );
            });
        }
    });
    ui.add_space(8.0);
    if !report.findings.is_empty() {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Actionable feedback");
                badge(ui, &format!("{} FINDING(S)", report.findings.len()), RED);
            });
            for finding in &report.findings {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(&finding.code).strong().color(RED));
                    ui.label(
                        egui::RichText::new(format!("{:?}", finding.stage).to_lowercase())
                            .color(muted(ui)),
                    );
                    if let Some(step_id) = &finding.step_id {
                        ui.label(egui::RichText::new(step_id).color(muted(ui)));
                    }
                    ui.label(&finding.message);
                });
                ui.label(
                    egui::RichText::new(format!("Next: {}", finding.next_action))
                        .color(muted(ui))
                        .size(11.0),
                );
            }
        });
        ui.add_space(8.0);
    }
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

fn show_integrity(
    ui: &mut egui::Ui,
    report: &CaseRunReport,
    manifest: Option<&CasePackageManifest>,
) {
    let integrity = &report.integrity;
    section_heading(
        ui,
        "Package integrity",
        "Byte identity of the package documents and of every artifact under a supplied root. A match proves identity only. A signature (ADR-0015) proves possession of a key at signing time, not correctness.",
    );
    // A receipt document's own runner signature, by the step it records;
    // every other document is covered only transitively, through the
    // manifest's requester signature (ADR-0015 clause 1).
    let receipt_signature_for_step = |step_id: &str| -> Option<&SignatureStatus> {
        report
            .execution
            .as_ref()?
            .steps
            .iter()
            .find(|step| step.step_id == step_id)
            .and_then(|step| step.receipt_signature.as_ref())
    };
    let signature_for_document = |document_id: &str, role: &str| -> Option<&SignatureStatus> {
        if role == "signature" {
            return None;
        }
        if role == "execution_receipt" {
            return manifest
                .and_then(|manifest| {
                    manifest
                        .documents
                        .iter()
                        .find(|document| document.document_id == document_id)
                })
                .and_then(|document| document.step_id.as_deref())
                .and_then(receipt_signature_for_step);
        }
        report.manifest_signature.as_ref()
    };
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Manifest:").strong());
            ui.label(&integrity.manifest_sha256);
            signature_status_badge(ui, report.manifest_signature.as_ref(), true);
        });
        // Horizontal, not just vertical: a signature-status column added to
        // an already-full-width grid must stay reachable by scrolling
        // rather than being silently clipped past the window edge.
        egui::ScrollArea::horizontal()
            .id_salt("documents-scroll")
            .show(ui, |ui| {
                egui::Grid::new("documents").striped(true).show(ui, |ui| {
                    for check in &integrity.documents {
                        ui.label(&check.document_id);
                        ui.label(egui::RichText::new(&check.role).color(muted(ui)));
                        ui.label(&check.path);
                        integrity_badge(ui, check.state);
                        signature_status_badge(
                            ui,
                            signature_for_document(&check.document_id, &check.role),
                            false,
                        );
                        ui.end_row();
                    }
                });
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
                        if step.presentation_gate.is_some() {
                            badge(ui, "OPTIONAL PRACTICALITY GATE", CORE_ORANGE);
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
                for requirement in &compiled.categorical_requirements {
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
        show_finding(ui, finding, sources, None);
    }
}

fn show_coverage(ui: &mut egui::Ui, report: &CaseRunReport) {
    use avila_core_compiler::{CoverageState, CoverageStatus};
    let Some(coverage) = &report.coverage else {
        return;
    };
    ui.add_space(12.0);
    section_heading(
        ui,
        "Coverage of the library requirement set",
        "A search optimizes exactly what the contract states. The set is what the library says any contract in its domain must address; each entry is covered by named contract requirements or omitted with a reason and an accepting owner.",
    );
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(&coverage.set_id).size(17.0).strong());
            ui.label(
                egui::RichText::new(format!("revision {}", coverage.set_revision)).color(muted(ui)),
            );
            match coverage.status {
                CoverageStatus::Complete => badge(ui, "COMPLETE", GREEN),
                CoverageStatus::Incomplete => badge(ui, "INCOMPLETE", RED),
            }
        });
        key_value(ui, "Owner", &coverage.set_owner);
        key_value(ui, "Set identity", &coverage.set_sha256);
        for issue in &coverage.issues {
            ui.colored_label(RED, issue);
        }
    });
    for entry in &coverage.entries {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(&entry.set_requirement_id)
                        .size(15.0)
                        .strong(),
                );
                match entry.state {
                    CoverageState::Covered => badge(ui, "COVERED", GREEN),
                    CoverageState::CoveredUnderBasis => badge(ui, "UNDER BASIS", RED),
                    CoverageState::OmittedStated => badge(ui, "OMITTED", AMBER),
                    CoverageState::Omissible => badge(ui, "OMISSIBLE", muted(ui)),
                    CoverageState::OmittedUnstated => badge(ui, "UNSTATED", RED),
                }
            });
            ui.label(&entry.statement);
            if !entry.covered_by.is_empty() {
                key_value(
                    ui,
                    "Covered by",
                    &entry
                        .covered_by
                        .iter()
                        .map(|cover| {
                            format!(
                                "{} ({:?}{})",
                                cover.requirement_id,
                                cover.basis,
                                if cover.adequate {
                                    ""
                                } else {
                                    ", below the set's minimum basis"
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; "),
                );
            }
            if let Some(reason) = &entry.reason {
                key_value(ui, "Omitted because", reason);
            }
            if let Some(accepted_by) = &entry.accepted_by {
                key_value(ui, "Accepted by", accepted_by);
            }
            for issue in &entry.issues {
                ui.colored_label(RED, issue);
            }
        });
    }
    if !coverage.additional_requirements.is_empty() {
        key_value(
            ui,
            "Beyond the set",
            &coverage.additional_requirements.join(", "),
        );
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
                if let Some(assessment) = &step.qualification {
                    let (label, color) = match assessment.state {
                        avila_core_compiler::EnvelopeState::Inside => ("INSIDE", GREEN),
                        avila_core_compiler::EnvelopeState::Outside => ("OUTSIDE", RED),
                        avila_core_compiler::EnvelopeState::Unknown => ("UNKNOWN", AMBER),
                        avila_core_compiler::EnvelopeState::Expired => ("EXPIRED", RED),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("Qualification envelope:").strong());
                        badge(ui, label, color);
                        ui.label(
                            egui::RichText::new(format!(
                                "{} rev {} by {}; {}/{} terms hold",
                                assessment.qualification_id,
                                assessment.revision,
                                assessment.owner,
                                assessment
                                    .terms
                                    .iter()
                                    .filter(
                                        |term| term.result == avila_core_kernel::TruthValue::True
                                    )
                                    .count(),
                                assessment.terms.len()
                            ))
                            .color(muted(ui)),
                        );
                    });
                    for term in assessment
                        .terms
                        .iter()
                        .filter(|term| term.result != avila_core_kernel::TruthValue::True)
                    {
                        ui.colored_label(RED, format!("{:?}: {}", term.result, term.predicate));
                    }
                    for issue in &assessment.issues {
                        ui.colored_label(RED, issue);
                    }
                }
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
            for finding in &step.findings {
                ui.colored_label(RED, format!("[{}] {}", finding.code, finding.message));
                ui.label(
                    egui::RichText::new(format!("Next: {}", finding.next_action))
                        .color(muted(ui))
                        .size(11.0),
                );
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
        for claim in &claims.evidence_claims {
            if let (Some(slot), Some(value)) = (
                claim.get("output_slot").and_then(serde_json::Value::as_str),
                claim
                    .pointer("/claim/value")
                    .and_then(serde_json::Value::as_str),
            ) {
                key_value(ui, &format!("Category · {slot}"), value);
            }
        }
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
                    "{}/{} evidence identities; {}/{} presentation-policy identities",
                    bindings.bound_evidence_records,
                    bindings.evidence_records,
                    bindings.bound_presentation_policies,
                    bindings.required_presentation_policies
                ));
                if bindings.receipted_evidence_records > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} carried by receipt for steps a supplied input reaches",
                            bindings.receipted_evidence_records
                        ))
                        .color(muted(ui)),
                    );
                }
                if bindings.withheld_evidence_records > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} withheld: reached by a supplied input and not run",
                            bindings.withheld_evidence_records
                        ))
                        .color(muted(ui)),
                    );
                }
            });
            for issue in &bindings.issues {
                ui.colored_label(RED, issue);
            }
        });
    }
    for gate in &report.presentation_gates {
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&gate.step_id).strong());
                badge(
                    ui,
                    match gate.readiness {
                        PresentationGateReadiness::ReadyForAgent => "READY FOR AGENT",
                        PresentationGateReadiness::AwaitingEvidence => "AWAITING EVIDENCE",
                    },
                    match gate.readiness {
                        PresentationGateReadiness::ReadyForAgent => BLUE,
                        PresentationGateReadiness::AwaitingEvidence => AMBER,
                    },
                );
                ui.label("connected agent · presentation routing only");
            });
            key_value(ui, "Presentation request", &gate.request_sha256);
            key_value(
                ui,
                "Dossier",
                &format!(
                    "{}/{} exact artifacts present",
                    gate.presented_evidence.len(),
                    gate.presented_evidence.len() + gate.missing_evidence.len()
                ),
            );
            for instruction in &gate.instructions {
                ui.label(
                    egui::RichText::new(format!("Instruction: {instruction}")).color(muted(ui)),
                );
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
                    let text = compact(value);
                    let shown = value
                        .as_str()
                        .map(avila_core_runner::display_number)
                        .filter(|shown| shown != &text)
                        .map_or(text.clone(), |shown| format!("{shown} (exact {text})"));
                    key_value(ui, field, &shown);
                }
            }
            if let Some(margin) = report
                .margins
                .iter()
                .find(|margin| margin.requirement_id == verdict.requirement_id)
                && let Some(value) = &margin.margin
            {
                let unit = margin.unit.as_deref().unwrap_or("");
                key_value(
                    ui,
                    "margin",
                    &format!(
                        "{} {unit} (exact {value}; positive when satisfied, on the decisive bound)",
                        avila_core_runner::display_number(value)
                    ),
                );
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
            key_value(ui, "Evidence", &verdict.evidence_ids.join(", "));
            egui::CollapsingHeader::new("Boundary")
                .id_salt(("boundary", &verdict.requirement_id))
                .show(ui, |ui| {
                    key_value(ui, "Semantic profile", &verdict.boundary.semantic_profile);
                    key_value(ui, "Compiler", &verdict.boundary.compiler);
                    key_value(ui, "Evaluator", &verdict.boundary.evaluator);
                    key_value(ui, "Snapshot", &verdict.boundary.compiled_snapshot_sha256);
                    key_value(ui, "Claims", &verdict.boundary.claims_sha256);
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

    fn example(name: &str) -> crate::case_browser::CaseInfo {
        crate::case_browser::CaseInfo::read(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/cases")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn machine_setup_keeps_the_question_inside_the_window() {
        for width in [920.0, 1260.0] {
            let context = egui::Context::default();
            crate::configure_style(&context);
            let mut view = CaseView::new(CaseSetup::default());
            view.select_case(example("case-000-actinv-aftermatter"), None)
                .unwrap();
            view.show_setup = true;
            let question = view.info.as_ref().unwrap().question.clone();
            for _ in 0..3 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| view.ui(ui, &mut TourTargets::default()),
                );
                output.textures_delta.clear();
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == question => Some(text),
                        _ => None,
                    })
                    .expect("case question must remain visible alongside setup");
                assert!(
                    text.visual_bounding_rect().right() <= width,
                    "question must wrap inside the window"
                );
            }
        }
    }

    #[test]
    fn opening_a_case_resets_results_and_only_restores_declared_locations() {
        let mut view = CaseView::new(CaseSetup::from_arguments(&[]).unwrap());
        assert!(view.setup.case_dir.is_empty());
        assert!(view.inspect_error.is_none());
        let first = example("case-000-actinv-aftermatter");
        view.select_case(first, None).unwrap();
        view.summary = "old report".into();
        view.run_error = Some("old error".into());
        view.setup.source_roots[0].path = "/old/data".into();
        view.setup.environment.push(NamedPath {
            name: "SECRET".into(),
            path: "not persisted".into(),
        });
        let next = example("case-003-thermal-spreader");
        let saved = crate::case_browser::RecentCase {
            path: next.path.clone(),
            roots: vec![
                NamedPath {
                    name: "case".into(),
                    path: "/restored/case".into(),
                },
                NamedPath {
                    name: "stale-root".into(),
                    path: "/stale".into(),
                },
            ],
            ..Default::default()
        };
        view.select_case(next, Some(&saved)).unwrap();
        assert!(view.summary.is_empty());
        assert!(view.run_error.is_none());
        assert!(view.report.is_none());
        assert!(view.running.is_none());
        assert!(view.setup.auto_run.is_none());
        assert!(
            !view
                .setup
                .source_roots
                .iter()
                .any(|r| r.name == "stale-root" || r.path == "/old/data")
        );
        assert_eq!(
            view.setup
                .source_roots
                .iter()
                .find(|r| r.name == "case")
                .unwrap()
                .path,
            "/restored/case"
        );
        assert!(
            !serde_json::to_string(&view.recent())
                .unwrap()
                .contains("SECRET")
        );
    }

    #[test]
    fn switching_is_blocked_during_a_run() {
        let mut view = CaseView::new(CaseSetup::default());
        let first = example("case-000-actinv-aftermatter");
        view.select_case(first.clone(), None).unwrap();
        let (_sender, receiver) = channel();
        view.running = Some(receiver);
        assert!(
            view.select_case(example("case-003-thermal-spreader"), None)
                .is_err()
        );
        assert_eq!(view.info.as_ref().unwrap().path, first.path);
        assert!(view.busy());
    }

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
            "--case".into(),
            "cases/x".into(),
            "--auto-plan".into(),
            "--screenshot".into(),
            "x.png".into(),
        ])
        .unwrap();
        assert_eq!(automatic.auto_run, Some(true));
        assert!(CaseSetup::from_arguments(&["--auto-plan".into()]).is_err());
        assert_eq!(automatic.screenshot.as_deref(), Some("x.png"));
        assert_eq!(CaseTab::by_name("verdicts"), Some(CaseTab::Verdicts));
        assert_eq!(display("A → B"), "A -> B");
        assert!(CaseSetup::from_arguments(&["--bogus".into()]).is_err());
        assert!(CaseSetup::from_arguments(&["--source-root".into(), "nope".into()]).is_err());
        let logged = CaseSetup::from_arguments(&["--log".into(), "campaign.jsonl".into()]).unwrap();
        assert_eq!(
            logged.options(false).log,
            Some(PathBuf::from("campaign.jsonl"))
        );
        let tools = CaseSetup::from_arguments(&[
            "--tools".into(),
            "run-report.json".into(),
            "--tool".into(),
            "requirements".into(),
        ])
        .unwrap();
        assert_eq!(tools.tools_path.as_deref(), Some("run-report.json"));
        assert_eq!(tools.tool.as_deref(), Some("requirements"));
        assert!(CaseSetup::from_arguments(&["--tool".into(), "unknown-query".into()]).is_err());
    }

    /// The setup panel keeps the exact inputs and intended parent visible
    /// before any plan or run: the attempt id, the bound parent, the tracked
    /// input and its supplied file, and the missing-log requirement.
    #[test]
    fn the_setup_panel_shows_the_intended_lineage_before_execution() {
        let context = egui::Context::default();
        crate::configure_style(&context);
        let mut view = CaseView::new(CaseSetup {
            attempt_id: "child-1".into(),
            parent_attempt_id: "copper-ratio2-r0".into(),
            candidate_input: "candidate".into(),
            free_inputs: vec![NamedPath {
                name: "candidate".into(),
                path: "candidates/reference.json".into(),
            }],
            ..CaseSetup::default()
        });
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(370.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| view.attempt_panel(ui),
        );
        output.textures_delta.clear();
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                _ => None,
            })
            .collect();
        for expected in [
            "child-1",
            "copper-ratio2-r0",
            "candidate",
            "candidates/reference.json",
            "requires a campaign log",
        ] {
            assert!(
                texts.iter().any(|text| text.contains(expected)),
                "{expected} missing from the setup panel"
            );
        }
    }

    /// The workbench assembles the same explicit `AttemptLineageRequest` the
    /// CLI's --attempt flags do; no attempt ID means no lineage request at
    /// all, even when the other fields are filled.
    #[test]
    fn attempt_fields_assemble_one_explicit_lineage_request() {
        let setup = CaseSetup {
            parent_attempt_id: "try-001".into(),
            candidate_input: "design".into(),
            ..CaseSetup::default()
        };
        assert!(setup.options(false).attempt.is_none());

        let setup = CaseSetup::from_arguments(&[
            "--attempt".into(),
            "try-002".into(),
            "--parent-attempt".into(),
            "try-001".into(),
            "--candidate-input".into(),
            "candidate".into(),
            "--log".into(),
            "campaign.jsonl".into(),
        ])
        .unwrap();
        let request = setup.options(false).attempt.unwrap();
        assert_eq!(request.attempt_id, "try-002");
        assert_eq!(request.parent_attempt_id.as_deref(), Some("try-001"));
        assert_eq!(request.candidate_input, "candidate");

        // An omitted candidate input defaults to `candidate`, as the CLI does.
        let setup = CaseSetup::from_arguments(&["--attempt".into(), "root-0".into()]).unwrap();
        let request = setup.options(true).attempt.unwrap();
        assert_eq!(request.candidate_input, "candidate");
        assert!(request.parent_attempt_id.is_none());
        assert!(setup.options(true).plan_only);
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

    #[test]
    fn trust_root_and_runner_key_are_parsed_and_left_unset_when_blank() {
        let setup = CaseSetup::from_arguments(&[
            "--trust-root".into(),
            "examples/keys/trust-root.json".into(),
            "--runner-key".into(),
            "examples/keys/runner.seed".into(),
        ])
        .unwrap();
        assert_eq!(setup.trust_root, "examples/keys/trust-root.json");
        assert_eq!(setup.runner_key, "examples/keys/runner.seed");
        let options = setup.options(false);
        assert_eq!(
            options.trust_root,
            Some(PathBuf::from("examples/keys/trust-root.json"))
        );
        assert_eq!(
            options.runner_key,
            Some(PathBuf::from("examples/keys/runner.seed"))
        );

        // Unset (the default) never fabricates a path: `execute_case` must
        // see `None`, not `Some(PathBuf::from(""))`, or every run would
        // look for a trust root file named the empty string.
        let unset = CaseSetup::default().options(false);
        assert_eq!(unset.trust_root, None);
        assert_eq!(unset.runner_key, None);
    }

    /// A bundled example's declared data folders are offered from the
    /// shipped trees on open — `case` is the package's own folder — while
    /// programs stay operator-supplied and a restored location still wins.
    #[test]
    fn a_bundled_example_prefills_its_shipped_data_folders() {
        let root_path = |view: &CaseView, name: &str| {
            view.setup
                .source_roots
                .iter()
                .find(|row| row.name == name)
                .map(|row| row.path.clone())
        };
        let mut view = CaseView::new(CaseSetup::default());
        view.select_case(example("case-003-thermal-spreader"), None)
            .unwrap();
        assert_eq!(
            root_path(&view, "case").as_deref(),
            Some(view.setup.case_dir.as_str())
        );
        assert_eq!(
            PathBuf::from(root_path(&view, "thermal").unwrap()),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/capabilities/thermal")
                .canonicalize()
                .unwrap()
        );
        assert!(
            view.setup
                .capabilities
                .iter()
                .all(|row| row.path.is_empty()),
            "programs are never prefilled"
        );

        // A restored location still wins over the bundled offer.
        let saved = crate::case_browser::RecentCase {
            path: example("case-003-thermal-spreader").path,
            roots: vec![NamedPath {
                name: "thermal".into(),
                path: "/my/thermal".into(),
            }],
            ..Default::default()
        };
        view.select_case(example("case-003-thermal-spreader"), Some(&saved))
            .unwrap();
        assert_eq!(root_path(&view, "thermal").as_deref(), Some("/my/thermal"));
        assert_eq!(
            root_path(&view, "case").as_deref(),
            Some(view.setup.case_dir.as_str())
        );

        // Outside the bundled tree only `case` resolves.
        let outside = probe_scratch("outside-case");
        assert_eq!(
            crate::case_browser::bundled_root(&outside, "case").unwrap(),
            outside
        );
        assert!(crate::case_browser::bundled_root(&outside, "thermal").is_none());
        std::fs::remove_dir_all(&outside).unwrap();
    }

    // --- capability probing --------------------------------------------

    /// A manifest declaring one capability pinned to `digest`, so a probe
    /// knows the expected identity without a shipped package.
    fn probe_manifest(capability_id: &str, digest: &str) -> CasePackageManifest {
        CasePackageManifest {
            schema_version: avila_core_evidence::CASE_PACKAGE_SCHEMA_VERSION.into(),
            case_id: "PROBE-CASE".into(),
            title: "Workbench capability probe fixture".into(),
            documents: Vec::new(),
            artifacts: Vec::new(),
            capabilities: vec![avila_core_evidence::PackageCapability {
                capability_id: capability_id.into(),
                package_id: format!("test/{capability_id}@1"),
                source_repository: None,
                source_commit: None,
                executable_sha256: digest.into(),
            }],
            executions: Vec::new(),
            free_inputs: Vec::new(),
            coverage: None,
            limitations: Vec::new(),
        }
    }

    fn probe_scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("avila-app-probe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_typed_probe_hashes_the_rows_path_against_the_pinned_identity() {
        let dir = probe_scratch("typed");
        let program = dir.join("stub");
        std::fs::write(&program, b"the pinned executable").unwrap();
        let digest = avila_core_evidence::sha256_file(&program).unwrap().0;
        let manifest = probe_manifest("stub", &digest);

        let ProbeReport::Typed {
            name,
            text,
            candidate,
        } = run_probe(
            ProbeJob::Typed("stub".into(), program.display().to_string()),
            &manifest,
        )
        else {
            panic!("a typed job reports a typed outcome")
        };
        assert_eq!(name, "stub");
        assert_eq!(text, program.display().to_string());
        let candidate = candidate.unwrap();
        assert_eq!(candidate.state, CapabilityCheckState::Verified);
        assert_eq!(candidate.sha256.as_deref(), Some(digest.as_str()));

        // A wrong file reports mismatch and a missing path reports missing;
        // an undeclared name has no pinned identity and reports nothing.
        let wrong = dir.join("wrong");
        std::fs::write(&wrong, b"other bytes").unwrap();
        let ProbeReport::Typed { candidate, .. } = run_probe(
            ProbeJob::Typed("stub".into(), wrong.display().to_string()),
            &manifest,
        ) else {
            panic!("a typed job reports a typed outcome")
        };
        assert_eq!(candidate.unwrap().state, CapabilityCheckState::Mismatch);
        let ProbeReport::Typed { candidate, .. } = run_probe(
            ProbeJob::Typed("stub".into(), dir.join("absent").display().to_string()),
            &manifest,
        ) else {
            panic!("a typed job reports a typed outcome")
        };
        assert_eq!(candidate.unwrap().state, CapabilityCheckState::Missing);
        let ProbeReport::Typed { candidate, .. } = run_probe(
            ProbeJob::Typed("mystery".into(), program.display().to_string()),
            &manifest,
        ) else {
            panic!("a typed job reports a typed outcome")
        };
        assert!(candidate.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_scan_offers_matches_to_every_declared_capability() {
        let dir = probe_scratch("scan");
        let program = dir.join("stub-1.0");
        std::fs::write(&program, b"the pinned executable").unwrap();
        std::fs::write(dir.join("other"), b"other bytes").unwrap();
        let digest = avila_core_evidence::sha256_file(&program).unwrap().0;
        let mut manifest = probe_manifest("stub", &digest);
        manifest
            .capabilities
            .push(avila_core_evidence::PackageCapability {
                capability_id: "absent".into(),
                package_id: "test/absent@1".into(),
                source_repository: None,
                source_commit: None,
                executable_sha256: format!("sha256:{}", "0".repeat(64)),
            });
        let ProbeReport::Found { source, outcomes } =
            run_probe(ProbeJob::ScanDir(dir.clone()), &manifest)
        else {
            panic!("a scan reports found outcomes")
        };
        assert!(source.contains("scan"), "{source}");
        assert_eq!(outcomes.len(), 2);
        let (name, candidates) = &outcomes[0];
        assert_eq!(name, "stub");
        assert!(candidates.iter().any(|candidate| {
            candidate.state == CapabilityCheckState::Verified
                && candidate.path == program.canonicalize().unwrap()
        }));
        let (name, candidates) = &outcomes[1];
        assert_eq!(name, "absent");
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.state == CapabilityCheckState::Mismatch)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn applying_probe_reports_updates_each_named_row() {
        let mut view = CaseView::new(CaseSetup::default());
        let candidate = CapabilityCandidate {
            path: PathBuf::from("/opt/stub"),
            sha256: Some("sha256:aa".into()),
            state: CapabilityCheckState::Verified,
        };
        view.apply_probe(ProbeReport::Typed {
            name: "stub".into(),
            text: "/opt/stub".into(),
            candidate: Some(candidate.clone()),
        });
        assert_eq!(
            view.probes["stub"].typed.as_ref().unwrap().1.state,
            CapabilityCheckState::Verified
        );
        view.apply_probe(ProbeReport::Found {
            source: "PATH".into(),
            outcomes: vec![
                ("stub".into(), vec![candidate]),
                ("absent".into(), Vec::new()),
            ],
        });
        assert_eq!(view.probes["stub"].found.as_ref().unwrap().0, "PATH");
        assert!(view.probes["absent"].found.as_ref().unwrap().1.is_empty());
        // A typed probe that names no declared capability is dropped.
        view.apply_probe(ProbeReport::Typed {
            name: "mystery".into(),
            text: "x".into(),
            candidate: None,
        });
        assert!(!view.probes.contains_key("mystery"));
    }

    /// The Programs section offers PATH and folder searches, shows the
    /// on-demand check verdict under the row, and offers a found match for
    /// the operator to take — never applying it silently.
    #[test]
    fn the_setup_panel_surfaces_probe_results_and_search_controls() {
        let context = egui::Context::default();
        crate::configure_style(&context);
        let mut view = CaseView::new(CaseSetup::default());
        view.select_case(example("case-000-actinv-aftermatter"), None)
            .unwrap();
        let verified = CapabilityCandidate {
            path: PathBuf::from("/opt/bin/python3"),
            sha256: Some("sha256:aa".into()),
            state: CapabilityCheckState::Verified,
        };
        let row = &mut view.setup.capabilities[0];
        assert_eq!(row.name, "python3");
        row.path = "/opt/bin/python3".into();
        view.probes.insert(
            "python3".into(),
            ProbeRow {
                typed: Some(("/opt/bin/python3".into(), verified.clone())),
                found: Some(("PATH".into(), vec![verified])),
            },
        );
        let mut targets = TourTargets::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(370.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| view.setup_panel(ui, &mut targets),
        );
        output.textures_delta.clear();
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                _ => None,
            })
            .collect();
        for expected in [
            "Search PATH",
            "Scan folder",
            "Check file",
            "This file matches the pinned program.",
            "PATH found /opt/bin/python3",
            "Use this program",
        ] {
            assert!(
                texts.iter().any(|text| text.contains(expected)),
                "{expected} missing from the setup panel"
            );
        }
    }
}
