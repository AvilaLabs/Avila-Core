//! Local navigation metadata only. Opening a package never runs or verifies it.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use avila_core_evidence::CasePackageManifest;
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::case_view::NamedPath;

#[derive(Clone, Debug)]
pub(crate) struct CaseInfo {
    pub path: PathBuf,
    pub manifest: CasePackageManifest,
    pub question: String,
    pub requirements: Vec<(String, String)>,
    pub assumptions: Vec<String>,
    pub metadata_error: Option<String>,
}

impl CaseInfo {
    pub fn read(path: &Path) -> Result<Self, String> {
        let path = if path.file_name().is_some_and(|n| n == "package.json") {
            path.parent()
                .ok_or("Choose a case folder or package.json")?
        } else {
            path
        };
        let path = path
            .canonicalize()
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let manifest: CasePackageManifest =
            serde_json::from_slice(&read_document(&path, "package.json")?).map_err(|e| {
                format!("This folder does not contain a readable Core package.json: {e}")
            })?;
        let question = manifest
            .documents
            .iter()
            .find(|d| d.role == "contract")
            .ok_or_else(|| "This package does not declare a contract document.".to_owned())
            .and_then(|d| read_document(&path, &d.path))
            .and_then(|bytes| {
                serde_json::from_slice::<avila_core_compiler::ContractSource>(&bytes)
                    .map_err(|e| e.to_string())
            });
        let (question, requirements, assumptions, metadata_error) = match question {
            Ok(contract) => {
                let requirements = contract
                    .requirements
                    .into_iter()
                    .map(|r| (r.requirement_id, r.statement))
                    .chain(
                        contract
                            .categorical_requirements
                            .into_iter()
                            .map(|r| (r.requirement_id, r.statement)),
                    )
                    .collect();
                (contract.question, requirements, contract.assumptions, None)
            }
            Err(error) => (String::new(), Vec::new(), Vec::new(), Some(error)),
        };
        Ok(Self {
            path,
            manifest,
            question,
            requirements,
            assumptions,
            metadata_error,
        })
    }
}

fn read_document(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("Document points outside the case folder.".into());
    }
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Document must be a regular file.".into());
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Document exceeds the 8 MiB preview limit.".into());
    }
    Ok(bytes)
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecentCase {
    pub path: PathBuf,
    pub title: String,
    pub roots: Vec<NamedPath>,
    pub capabilities: Vec<NamedPath>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalCases {
    pub recent: Vec<RecentCase>,
}
impl LocalCases {
    pub fn remember(&mut self, entry: RecentCase) {
        self.recent.retain(|old| old.path != entry.path);
        self.recent.insert(0, entry);
        self.recent.truncate(12);
    }
}

#[derive(Default)]
pub(crate) struct FilePicker {
    pending: Option<Receiver<Option<PathBuf>>>,
}
impl FilePicker {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn start(&mut self, context: &egui::Context, folder: bool, title: &str) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let context = context.clone();
        let title = title.to_owned();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().set_title(title);
            let selected = if folder {
                dialog.pick_folder()
            } else {
                dialog.pick_file()
            };
            let _ = tx.send(selected);
            context.request_repaint();
        });
    }
    pub fn poll(&mut self) -> Option<PathBuf> {
        match self.pending.as_ref()?.try_recv() {
            Ok(path) => {
                self.pending = None;
                path
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                None
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
        }
    }
}

pub(crate) struct CaseBrowser {
    pub local: LocalCases,
    examples: Vec<CaseInfo>,
    search: String,
    path: String,
    pub error: Option<String>,
    pub picker: FilePicker,
    pub report_picker: FilePicker,
}
impl CaseBrowser {
    pub fn new(local: LocalCases) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cases");
        let mut examples: Vec<_> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| CaseInfo::read(&entry.path()).ok())
            .collect();
        examples.sort_by(|a, b| a.manifest.case_id.cmp(&b.manifest.case_id));
        Self {
            local,
            examples,
            search: String::new(),
            path: String::new(),
            error: None,
            picker: FilePicker::default(),
            report_picker: FilePicker::default(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, active: bool) -> Option<PathBuf> {
        let mut selected = None;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0);
                ui.heading("Choose a case");
                ui.label("A case brings together a technical question, its requirements, and the methods and evidence used to answer it.");
                ui.label(egui::RichText::new("Opening a case lets you explore it. A run is a separate attempt to answer its question.").color(crate::muted(ui)));
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.picker.busy() && !active, egui::Button::new("Open case folder…").min_size(egui::vec2(160.0, 36.0))).on_hover_text("Choose the folder containing package.json. Ctrl+O").clicked() {
                        self.picker.start(ui.ctx(), true, "Open a Core case folder (contains package.json)");
                    }
                    if ui.add_enabled(!self.report_picker.busy(), egui::Button::new("Open saved results…")).on_hover_text("Choose run-report.json or the saved JSON output of a Core run. Tools reads it without executing anything.").clicked() {
                        self.report_picker.start(ui.ctx(), false, "Open a saved Core run report");
                    }
                    ui.small("You can also drop a case folder or package.json here.");
                });
                if active { ui.label("A case is running. You can browse here; opening another case will be available when it finishes."); }
                ui.collapsing("Open by path", |ui| {
                    ui.horizontal(|ui| {
                        let input = ui.add(egui::TextEdit::singleline(&mut self.path).hint_text("Case folder or package.json").desired_width(ui.available_width()-100.0));
                        let enter = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.add_enabled(!active, egui::Button::new("Open case")).clicked() || (enter && !active) { selected = Some(PathBuf::from(self.path.trim())); }
                    });
                });
                if let Some(error) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                ui.add_space(14.0);
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search case names or example questions").desired_width(f32::INFINITY));
                let query = self.search.to_lowercase();
                ui.add_space(8.0);
                ui.heading("Recent cases");
                if self.local.recent.is_empty() { ui.label("Cases you open will appear here. Start with an example below, or open a case someone shared with you."); }
                let mut forget = None;
                for (index, entry) in self.local.recent.iter().enumerate() {
                    if !entry.title.to_lowercase().contains(&query) && !entry.path.to_string_lossy().to_lowercase().contains(&query) { continue; }
                    ui.push_id(("recent", index), |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                if ui.add_enabled(!active, egui::Button::new(entry.title.replace('→', "->"))).clicked() { selected = Some(entry.path.clone()); }
                                if ui.small_button("Forget").on_hover_text("Remove from recent cases and forget saved locations. Does not delete any files.").clicked() { forget = Some(index); }
                            });
                            ui.small(entry.path.display().to_string());
                        });
                    });
                }
                if let Some(index) = forget { self.local.recent.remove(index); }
                ui.add_space(12.0);
                ui.heading("Explore an example");
                ui.small("Research examples demonstrate Core's workflow. They are not validated engineering benchmarks. External data or programs may be needed to run them.");
                let mut count = 0;
                for example in &self.examples {
                    if !format!("{} {} {}", example.manifest.case_id, example.manifest.title, example.question).to_lowercase().contains(&query) { continue; }
                    count += 1;
                    ui.push_id(&example.path, |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.small(&example.manifest.case_id);
                            if ui.add_enabled(!active, egui::Button::new(egui::RichText::new(example.manifest.title.replace('→', "->")).strong()).wrap()).clicked() { selected = Some(example.path.clone()); }
                            ui.label(&example.question);
                        });
                    });
                }
                if count == 0 { ui.label(if self.examples.is_empty() { "Examples are not installed with this build. Open a case folder to get started." } else { "No examples match your search." }); }
            });
        });
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_filtered_example_opens_by_click_at_both_window_widths() {
        for width in [920.0, 1260.0] {
            let context = egui::Context::default();
            let mut browser = CaseBrowser::new(LocalCases::default());
            browser.search = "thermal spreader".into();
            let expected = browser
                .examples
                .iter()
                .find(|case| case.manifest.case_id == "CASE-003")
                .unwrap()
                .clone();
            let title = expected.manifest.title.replace('→', "->");
            let input = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 800.0),
                )),
                ..Default::default()
            };
            let mut warmup = context.run_ui(input(), |ui| {
                browser.ui(ui, false);
            });
            warmup.textures_delta.clear();
            let mut output = context.run_ui(input(), |ui| {
                browser.ui(ui, false);
            });
            output.textures_delta.clear();
            let rect = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == title => {
                        Some(text.visual_bounding_rect())
                    }
                    _ => None,
                })
                .expect("the filtered example button must be visible");
            assert!(
                rect.right() <= width,
                "example title must wrap within the viewport"
            );
            let position = rect.center();
            let mut selected = None;
            for pressed in [true, false] {
                let mut raw = input();
                raw.events = vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                let mut clicked = context.run_ui(raw, |ui| {
                    if let Some(path) = browser.ui(ui, false) {
                        selected = Some(path);
                    }
                });
                clicked.textures_delta.clear();
            }
            assert_eq!(selected, Some(expected.path));
        }
    }

    #[test]
    fn cancelling_a_dialog_leaves_no_pending_selection() {
        let (tx, rx) = channel();
        let mut picker = FilePicker { pending: Some(rx) };
        tx.send(None).unwrap();
        assert!(picker.poll().is_none());
        assert!(!picker.busy());
        assert!(picker.poll().is_none());
    }
    #[test]
    fn local_locations_round_trip_without_execution_configuration() {
        let mut state = LocalCases::default();
        state.remember(RecentCase {
            path: "/cases/a".into(),
            roots: vec![NamedPath {
                name: "data".into(),
                path: "/data/a".into(),
            }],
            ..Default::default()
        });
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: LocalCases = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.recent[0].roots[0].path, "/data/a");
        assert!(!encoded.contains("environment"));
        assert!(!encoded.contains("auto_run"));
    }
    #[test]
    fn recents_deduplicate_and_bound_local_history() {
        let mut state = LocalCases::default();
        for n in 0..15 {
            state.remember(RecentCase {
                path: PathBuf::from(n.to_string()),
                ..Default::default()
            });
        }
        assert_eq!(state.recent.len(), 12);
        state.remember(RecentCase {
            path: "12".into(),
            title: "Updated".into(),
            ..Default::default()
        });
        assert_eq!(state.recent.len(), 12);
        assert_eq!(state.recent[0].title, "Updated");
    }
    #[test]
    fn document_preview_rejects_escape() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert!(
            read_document(&root, "../../Cargo.toml")
                .unwrap_err()
                .contains("outside")
        );
    }
    #[test]
    fn examples_have_authored_questions_and_manifest_paths_work() {
        let browser = CaseBrowser::new(LocalCases::default());
        assert!(!browser.examples.is_empty());
        for case in browser.examples {
            assert!(case.metadata_error.is_none(), "{:?}", case.metadata_error);
            assert!(!case.question.is_empty());
            assert_eq!(
                CaseInfo::read(&case.path.join("package.json"))
                    .unwrap()
                    .path,
                case.path
            );
        }
    }
}
