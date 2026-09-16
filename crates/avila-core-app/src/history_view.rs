//! A dedicated recorded-history view: open one campaign log, navigate its
//! recorded attempt constellation, and inspect a selected record's identity,
//! candidate state, typed changes, findings, and recorded outcomes.
//!
//! Every value shown comes from the shared `core_constellation` and
//! `core_attempt` query operations; the view verifies nothing, compares
//! nothing, and holds no scientific state of its own.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use avila_core_kernel::VerdictStatus;
use avila_core_runner::query::{QueryContext, call_tool};
use eframe::egui;
use serde_json::{Value, json};

use crate::case_browser::FilePicker;
use crate::tools_view::render_object;
use crate::{CORE_ORANGE, badge, card, muted, section_heading, verdict_badge};

const RED: egui::Color32 = egui::Color32::from_rgb(232, 102, 102);
const BLUE: egui::Color32 = egui::Color32::from_rgb(120, 164, 210);

/// Which recorded row is selected: an identity-bound attempt, or one
/// untracked log line addressed by its 1-based line number.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    Attempt(String),
    Record(u64),
}

impl Selection {
    fn matches(&self, item: &Value) -> bool {
        match self {
            Self::Attempt(id) => item["attempt_id"].as_str() == Some(id),
            Self::Record(line) => item["line"].as_u64() == Some(*line),
        }
    }

    /// `line:N` selects an untracked record; anything else is an attempt id.
    fn parse(text: &str) -> Self {
        text.strip_prefix("line:")
            .and_then(|line| line.parse::<u64>().ok())
            .map_or_else(|| Self::Attempt(text.to_string()), Self::Record)
    }
}

#[derive(Default)]
pub(crate) struct HistoryView {
    log_path: String,
    picker: FilePicker,
    running: Option<Receiver<Result<Value, String>>>,
    result: Option<Value>,
    error: Option<String>,
    selected: Option<Selection>,
    detail_running: Option<Receiver<Result<Value, String>>>,
    detail: Option<Value>,
    detail_error: Option<String>,
    auto_open: bool,
    pending_select: Option<Selection>,
}

impl HistoryView {
    pub(crate) fn new(path: Option<String>, select: Option<String>) -> Self {
        Self {
            auto_open: path.is_some(),
            log_path: path.unwrap_or_default(),
            pending_select: select.map(|text| Selection::parse(text.trim())),
            ..Default::default()
        }
    }

    pub(crate) fn settled(&self) -> bool {
        self.running.is_none() && self.detail_running.is_none() && !self.auto_open
    }

    /// Drop every record and selection derived from the previous source so a
    /// new log never inherits stale state.
    fn clear_records(&mut self) {
        self.result = None;
        self.error = None;
        self.selected = None;
        self.detail = None;
        self.detail_error = None;
        self.detail_running = None;
    }

    fn open(&mut self, context: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        let path = self.log_path.trim().to_string();
        if path.is_empty() {
            self.clear_records();
            self.error = Some(
                "Paste or drop a campaign log path (campaign.jsonl or attempts.jsonl).".into(),
            );
            return;
        }
        let (sender, receiver) = channel();
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = sender.send(fetch_log(&path));
            context.request_repaint();
        });
        self.clear_records();
        self.running = Some(receiver);
    }

    fn select(&mut self, selection: Selection, context: &egui::Context) {
        if self.selected.as_ref() == Some(&selection) {
            return;
        }
        self.selected = Some(selection.clone());
        self.detail = None;
        self.detail_error = None;
        self.detail_running = None;
        if let Selection::Attempt(id) = selection {
            let path = self.log_path.trim().to_string();
            let (sender, receiver) = channel();
            let context = context.clone();
            std::thread::spawn(move || {
                let _ = sender.send(fetch_attempt(&path, &id));
                context.request_repaint();
            });
            self.detail_running = Some(receiver);
        }
    }

    fn poll(&mut self) {
        if let Some(receiver) = &self.running {
            match receiver.try_recv() {
                Ok(Ok(result)) => self.result = Some(result),
                Ok(Err(error)) => self.error = Some(error),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("The history query ended without a result.".into());
                }
            }
            self.running = None;
        }
        let Some(receiver) = &self.detail_running else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(detail)) => {
                self.detail = Some(detail);
                self.detail_running = None;
            }
            Ok(Err(error)) => {
                self.detail_error = Some(error);
                self.detail_running = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.detail_error = Some("The attempt query ended without a result.".into());
                self.detail_running = None;
            }
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, workbench_log: &str) {
        self.poll();
        if std::mem::take(&mut self.auto_open) {
            self.open(ui.ctx());
        }
        if self.result.is_some()
            && let Some(selection) = self.pending_select.take()
        {
            self.select(selection, ui.ctx());
        }
        if let Some(path) = self.picker.poll() {
            self.log_path = path.display().to_string();
            self.open(ui.ctx());
        }
        let busy = self.running.is_some();

        egui::Panel::left("history-constellation")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| {
                ui.set_min_width(230.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                egui::ScrollArea::vertical()
                    .id_salt("history-tree")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        if let Some(value) = &self.result {
                            if let Some(selection) = self.constellation_tree(ui, value) {
                                self.select(selection, ui.ctx());
                            }
                        } else {
                            ui.label(
                                egui::RichText::new(
                                    "Open a campaign log to see its recorded attempts and runs.",
                                )
                                .color(muted(ui)),
                            );
                        }
                    });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            section_heading(
                ui,
                "Recorded history",
                "One campaign log's recorded attempts, runs, and their lineage. Records are observations, not current verification.",
            );
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("Campaign log");
                ui.add(
                    egui::TextEdit::singleline(&mut self.log_path)
                        .hint_text("Paste a path to campaign.jsonl, or drop the file here")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open log").clicked() {
                        self.open(ui.ctx());
                    }
                    if ui
                        .add_enabled(!self.picker.busy(), egui::Button::new("Browse…"))
                        .clicked()
                    {
                        self.picker
                            .start(ui.ctx(), false, "Open a Core campaign log");
                    }
                    if !workbench_log.trim().is_empty()
                        && workbench_log.trim() != self.log_path.trim()
                        && ui
                            .button("Use the case workbench's campaign log")
                            .on_hover_text(workbench_log.trim())
                            .clicked()
                    {
                        self.log_path = workbench_log.trim().into();
                        self.open(ui.ctx());
                    }
                });
            });
            if self.running.is_none() {
                let dropped = ui.ctx().input(|input| {
                    input
                        .raw
                        .dropped_files
                        .first()
                        .map(|file| file.path().to_path_buf())
                });
                if let Some(path) = dropped {
                    self.log_path = path.display().to_string();
                    self.open(ui.ctx());
                }
            }
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Reading the recorded history…");
                });
            }
            if let Some(error) = &self.error {
                ui.add_space(6.0);
                ui.colored_label(RED, error);
            }
            if let Some(value) = &self.result {
                self.source_strip(ui, value);
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("history-detail")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.result.is_none() && self.running.is_none() && self.error.is_none() {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new(
                                "No log open. Paste a path, drop a campaign log, or use Browse.",
                            )
                            .color(muted(ui)),
                        );
                        return;
                    }
                    self.detail_panel(ui);
                });
        });
    }

    /// The recorded-file identity and the recorded-only boundary, shown with
    /// every opened log.
    fn source_strip(&self, ui: &mut egui::Ui, value: &Value) {
        let result = &value["result"];
        let summary = &result["summary"];
        ui.horizontal_wrapped(|ui| {
            badge(ui, "RECORDED RESULT", CORE_ORANGE);
            for case_id in summary["case_ids"].as_array().into_iter().flatten() {
                if let Some(case_id) = case_id.as_str() {
                    badge(ui, case_id, BLUE);
                }
            }
            if let Some(lineage) = result["lineage_validation"].as_str() {
                badge(
                    ui,
                    &format!("LINEAGE {}", lineage.to_uppercase().replace('_', " ")),
                    muted(ui),
                );
            }
            if let Some(signatures) = result["signature_verification"].as_str() {
                badge(
                    ui,
                    &format!("SIGNATURES {}", signatures.to_uppercase().replace('_', " ")),
                    muted(ui),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            if let Some(lines) = summary["lines"].as_u64() {
                ui.label(format!("{lines} recorded runs"));
            }
            if let Some(attempts) = summary["attempt_records"].as_u64() {
                ui.label(format!("{attempts} attempts"));
            }
            if let Some(untracked) = summary["untracked_records"].as_u64()
                && untracked > 0
            {
                ui.label(format!("{untracked} untracked"));
            }
            if let Some(generation) = summary["max_generation"].as_u64() {
                ui.label(format!("max generation {generation}"));
            }
        });
        if let Some(source) = value.get("source") {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        source["path"]
                            .as_str()
                            .or_else(|| source["label"].as_str())
                            .unwrap_or(""),
                    )
                    .small(),
                )
                .wrap(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(source["sha256"].as_str().unwrap_or(""))
                        .small()
                        .monospace()
                        .color(muted(ui)),
                )
                .wrap(),
            );
        }
        ui.label(
            egui::RichText::new(
                "Recorded observations only: this view does not recheck artifact bytes, signatures, or current reuse. Missing ancestry is unknown, not absent.",
            )
            .small()
            .color(muted(ui)),
        );
    }

    /// The left panel: roots with their recorded children, then every run the
    /// log carries without an attempt record. Returns a click's selection.
    fn constellation_tree(&self, ui: &mut egui::Ui, value: &Value) -> Option<Selection> {
        let items = value["result"]["items"].as_array();
        let Some(items) = items else {
            ui.colored_label(RED, "The query result carried no item list.");
            return None;
        };
        if items.is_empty() {
            let status = value["result"]["match_status"].as_str().unwrap_or("");
            ui.label(if status == "no_match_in_record" {
                "This log exists but holds no matching records. Absence here means no match in this file only."
            } else {
                "This log holds no records."
            });
            return None;
        }
        let (roots, children, untracked) = tree(items);
        if roots.is_empty() && untracked.is_empty() {
            ui.label("No attempts or recorded runs were found.");
            return None;
        }
        let mut picked = None;
        if !roots.is_empty() {
            ui.label(
                egui::RichText::new("ATTEMPTS")
                    .size(10.0)
                    .strong()
                    .color(muted(ui)),
            );
            for index in roots {
                let selection = self.attempt_row(ui, items, index, &children, 0);
                if picked.is_none() {
                    picked = selection;
                }
            }
            ui.add_space(10.0);
        }
        if !untracked.is_empty() {
            ui.label(
                egui::RichText::new("OTHER RECORDED RUNS")
                    .size(10.0)
                    .strong()
                    .color(muted(ui)),
            );
            ui.label(
                egui::RichText::new("Runs recorded before or outside attempt lineage.")
                    .size(11.0)
                    .color(muted(ui)),
            );
            for index in untracked {
                let item = &items[index];
                let line = item["line"].as_u64().unwrap_or(0);
                let label = item["case_id"]
                    .as_str()
                    .map(|id| format!("line {line} · {id}"))
                    .unwrap_or_else(|| format!("line {line}"));
                let row = ui
                    .horizontal_wrapped(|ui| {
                        let selected = self.selected == Some(Selection::Record(line));
                        let response = ui.selectable_label(selected, label);
                        if let Some(status) = item["status"].as_str() {
                            status_badge(ui, status);
                        }
                        response
                    })
                    .inner;
                if row.clicked() {
                    picked = Some(Selection::Record(line));
                }
            }
        }
        picked
    }

    /// One attempt row plus its recorded children, indented by generation.
    /// Returns a selection change if a row was clicked.
    fn attempt_row(
        &self,
        ui: &mut egui::Ui,
        items: &[Value],
        index: usize,
        children: &BTreeMap<String, Vec<usize>>,
        depth: usize,
    ) -> Option<Selection> {
        let item = &items[index];
        let id = item["attempt_id"].as_str()?.to_string();
        let mut picked = None;
        ui.push_id(("attempt", &id), |ui| {
            ui.horizontal_wrapped(|ui| {
                if depth > 0 {
                    ui.add_space(6.0 + 16.0 * depth as f32);
                    ui.label(egui::RichText::new("+").color(muted(ui)));
                }
                let response =
                    ui.selectable_label(self.selected == Some(Selection::Attempt(id.clone())), &id);
                if let Some(generation) = item["generation"].as_u64() {
                    ui.label(
                        egui::RichText::new(format!("gen {generation}"))
                            .size(10.0)
                            .color(muted(ui)),
                    );
                }
                if let Some(status) = item["status"].as_str() {
                    status_badge(ui, status);
                }
                if response.clicked() {
                    picked = Some(Selection::Attempt(id.clone()));
                }
            });
            for child in children.get(&id).into_iter().flatten() {
                let selection = self.attempt_row(ui, items, *child, children, depth + 1);
                if picked.is_none() {
                    picked = selection;
                }
            }
        });
        picked
    }

    fn detail_panel(&mut self, ui: &mut egui::Ui) {
        let (Some(value), Some(selection)) = (&self.result, &self.selected) else {
            if self.result.is_some() {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("Select an attempt or a recorded run on the left.")
                        .color(muted(ui)),
                );
            }
            return;
        };
        let items = value["result"]["items"].as_array();
        let Some(item) = items.and_then(|items| items.iter().find(|item| selection.matches(item)))
        else {
            ui.colored_label(
                RED,
                "The selected record is no longer in this log's result. Reopen the log.",
            );
            return;
        };
        match selection {
            Selection::Attempt(_) => self.attempt_detail(ui, item),
            Selection::Record(_) => record_detail(ui, item),
        }
    }

    /// An attempt's validated detail: the lineage record, the surrounding
    /// run's recorded outcome, and the Core-recomputed parent comparison.
    fn attempt_detail(&self, ui: &mut egui::Ui, item: &Value) {
        let id = item["attempt_id"].as_str().unwrap_or("").to_string();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&id).size(19.0).strong());
                if let Some(generation) = item["generation"].as_u64() {
                    badge(ui, &format!("GENERATION {generation}"), muted(ui));
                }
                if let Some(status) = item["status"].as_str() {
                    status_badge(ui, status);
                }
                if item["parent_attempt_id"].is_null() {
                    badge(ui, "ROOT ATTEMPT", BLUE);
                }
            });
            match item["parent_attempt_id"].as_str() {
                Some(parent) => {
                    ui.label(format!("Child of attempt {parent}"));
                    if let Some(line) = item["parent_line"].as_u64() {
                        ui.label(
                            egui::RichText::new(format!("parent on log line {line}"))
                                .small()
                                .color(muted(ui)),
                        );
                    }
                }
                None => {
                    ui.label(
                        egui::RichText::new("No recorded parent; this attempt starts a lineage.")
                            .color(muted(ui)),
                    );
                }
            }
        });
        ui.add_space(8.0);

        if self.detail_running.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Validating the attempt's lineage…");
            });
        }
        if let Some(error) = &self.detail_error {
            ui.colored_label(RED, error);
        }
        if let Some(detail) = &self.detail {
            let result = &detail["result"];
            if result["match_status"].as_str() == Some("no_match_in_record") {
                ui.colored_label(
                    RED,
                    format!(
                        "No attempt named {id} exists in this log. Its ancestry is unknown here."
                    ),
                );
            } else {
                let attempt = &result["attempt"];
                card(ui, |ui| {
                    ui.label(egui::RichText::new("IDENTITIES").size(10.0).strong());
                    key_row(ui, "Record", result["record_sha256"].as_str().unwrap_or(""));
                    if let Some(line) = result["line"].as_u64() {
                        key_row(ui, "Log line", &line.to_string());
                    }
                    key_row(
                        ui,
                        "Candidate input",
                        attempt["candidate_input"].as_str().unwrap_or(""),
                    );
                    key_row(
                        ui,
                        "Candidate file",
                        attempt["candidate_artifact_sha256"].as_str().unwrap_or(""),
                    );
                    key_row(
                        ui,
                        "Candidate state",
                        attempt["candidate_state_sha256"].as_str().unwrap_or(""),
                    );
                    if let Some(parent_sha) = attempt["parent_record_sha256"].as_str() {
                        key_row(ui, "Parent record", parent_sha);
                    }
                    key_row(
                        ui,
                        "Fixed manifest",
                        attempt["fixed_manifest_sha256"].as_str().unwrap_or(""),
                    );
                    key_row(
                        ui,
                        "Fixed snapshot",
                        attempt["fixed_compiled_snapshot_sha256"]
                            .as_str()
                            .unwrap_or(""),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if let Some(lineage) = result["lineage_validation"].as_str() {
                            badge(
                                ui,
                                &format!("LINEAGE {}", lineage.to_uppercase()),
                                muted(ui),
                            );
                        }
                        if let Some(signatures) = result["signature_verification"].as_str() {
                            badge(
                                ui,
                                &format!(
                                    "SIGNATURES {}",
                                    signatures.to_uppercase().replace('_', " ")
                                ),
                                muted(ui),
                            );
                        }
                    });
                });
                ui.add_space(8.0);
                card(ui, |ui| {
                    ui.label(egui::RichText::new("CANDIDATE STATE").size(10.0).strong());
                    ui.label(
                        egui::RichText::new(
                            "The canonical JSON Core snapshotted and diffed for this attempt.",
                        )
                        .small()
                        .color(muted(ui)),
                    );
                    render_object(ui, &attempt["candidate_state"]);
                });
                let changes = attempt["changes"].as_array();
                if !item["parent_attempt_id"].is_null() || changes.is_some_and(|c| !c.is_empty()) {
                    ui.add_space(8.0);
                    card(ui, |ui| {
                        ui.label(
                            egui::RichText::new("TYPED CHANGES FROM PARENT")
                                .size(10.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Core's derived diff of the two canonical candidate states; observation, not designer rationale.",
                            )
                            .small()
                            .color(muted(ui)),
                        );
                        match changes {
                            Some(changes) if !changes.is_empty() => show_changes(ui, changes),
                            _ => {
                                ui.label("No recorded candidate changes from its parent.");
                            }
                        }
                    });
                }
                if let Some(comparison) = result.get("comparison").filter(|c| !c.is_null()) {
                    ui.add_space(8.0);
                    card(ui, |ui| {
                        show_comparison(ui, comparison);
                    });
                }
            }
        }
        ui.add_space(8.0);
        outcome_detail(ui, item);
    }
}

/// The log's full constellation: every page merged in order so the tree sees
/// the complete recorded lineage, not a page slice.
fn fetch_log(path: &str) -> Result<Value, String> {
    let context = QueryContext::unrestricted();
    let mut offset = 0;
    let mut items: Vec<Value> = Vec::new();
    let mut envelope;
    loop {
        let page = call_tool(
            &context,
            "core_constellation",
            json!({"path": path, "offset": offset, "limit": 100}),
        )?;
        if let Some(page_items) = page["result"]["items"].as_array() {
            items.extend(page_items.iter().cloned());
        }
        let next = page["result"]["next_offset"].as_u64().map(|n| n as usize);
        envelope = page;
        match next {
            Some(next) => offset = next,
            None => break,
        }
    }
    envelope["result"]["items"] = json!(items);
    envelope["result"]["offset"] = json!(0);
    envelope["result"]["next_offset"] = Value::Null;
    Ok(envelope)
}

/// The runner's identity-validated attempt view: the full lineage record and
/// the recomputed comparison with its bound parent.
fn fetch_attempt(path: &str, id: &str) -> Result<Value, String> {
    call_tool(
        &QueryContext::unrestricted(),
        "core_attempt",
        json!({"path": path, "id": id}),
    )
}

/// Partition items into roots, a parent→children map, and untracked runs, all
/// in recorded (file) order.
fn tree(items: &[Value]) -> (Vec<usize>, BTreeMap<String, Vec<usize>>, Vec<usize>) {
    let mut roots = Vec::new();
    let mut children: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut untracked = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item["attempt_id"].as_str() {
            None => untracked.push(index),
            Some(_) => match item["parent_attempt_id"].as_str() {
                Some(parent) => children.entry(parent.to_string()).or_default().push(index),
                None => roots.push(index),
            },
        }
    }
    (roots, children, untracked)
}

fn status_badge(ui: &mut egui::Ui, status: &str) {
    let color = match status {
        "evaluated" => egui::Color32::from_rgb(95, 197, 128),
        "rejected" | "error" => RED,
        "planned" => CORE_ORANGE,
        _ => muted(ui),
    };
    badge(ui, &status.to_uppercase().replace('_', " "), color);
}

/// A recorded run's own verdict badge, parsed from the record's status text.
fn verdict_status_badge(ui: &mut egui::Ui, status: &str) {
    match serde_json::from_value::<VerdictStatus>(json!(status)) {
        Ok(status) => verdict_badge(ui, status),
        Err(_) => badge(ui, &status.to_uppercase().replace('_', " "), muted(ui)),
    }
}

fn key_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("{key}:")).strong());
        ui.add(
            egui::Label::new(egui::RichText::new(value).monospace().size(11.0))
                .wrap()
                .selectable(true),
        );
    });
}

/// The recorded outcome shared by attempts and untracked runs: verdicts,
/// steps, supplied inputs, and findings exactly as the log carries them.
fn outcome_detail(ui: &mut egui::Ui, item: &Value) {
    card(ui, |ui| {
        ui.label(egui::RichText::new("RECORDED OUTCOME").size(10.0).strong());
        key_row(
            ui,
            "Log line",
            &item["line"].as_u64().unwrap_or(0).to_string(),
        );
        key_row(ui, "Record", item["record_sha256"].as_str().unwrap_or(""));
        if let Some(recorded_at) = item["recorded_at"].as_str() {
            key_row(ui, "Recorded at", recorded_at);
        }
        match item["case_id"].as_str() {
            Some(case_id) => key_row(ui, "Case", case_id),
            None => key_row(ui, "Case", "not recorded on this line"),
        }
        if let Some(execution) = item["execution_status"].as_str() {
            key_row(ui, "Execution", execution);
        }
        if let Some(workspace) = item["workspace"].as_str() {
            key_row(ui, "Workspace", workspace);
        }
        if let Some(request) = item.get("attempt_request") {
            ui.label(
                egui::RichText::new(
                    "Attempt requested but not admitted; the refusal finding below is the record.",
                )
                .small()
                .color(muted(ui)),
            );
            render_object(ui, request);
        }
        let verdicts = item["verdicts"].as_array();
        if verdicts.is_some_and(|verdicts| !verdicts.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("VERDICTS").size(10.0).strong());
            for verdict in verdicts.into_iter().flatten() {
                ui.horizontal_wrapped(|ui| {
                    if let Some(id) = verdict["requirement_id"].as_str() {
                        ui.label(egui::RichText::new(id).strong());
                    }
                    if let Some(status) = verdict["status"].as_str() {
                        verdict_status_badge(ui, status);
                    }
                    if let Some(margin) = verdict["margin"].as_str() {
                        ui.label(
                            egui::RichText::new(format!(
                                "margin {margin} {}",
                                verdict["unit"].as_str().unwrap_or("")
                            ))
                            .monospace()
                            .size(11.0),
                        );
                    }
                });
            }
        }
        let steps = item["steps"].as_array();
        if steps.is_some_and(|steps| !steps.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("STEPS").size(10.0).strong());
            for step in steps.into_iter().flatten() {
                if let (Some(id), Some(state)) = (step["step_id"].as_str(), step["state"].as_str())
                {
                    ui.label(format!("{id}: {}", state.replace('_', " ")));
                }
            }
        }
        let inputs = item["supplied_inputs"].as_array();
        if inputs.is_some_and(|inputs| !inputs.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("SUPPLIED INPUTS").size(10.0).strong());
            for input in inputs.into_iter().flatten() {
                ui.horizontal_wrapped(|ui| {
                    if let Some(id) = input["input_id"].as_str() {
                        ui.label(egui::RichText::new(id).strong());
                    }
                    if let Some(sha) = input["sha256"].as_str() {
                        ui.add(
                            egui::Label::new(egui::RichText::new(sha).monospace().size(11.0))
                                .wrap(),
                        );
                    }
                });
            }
        }
        let findings = item["findings"].as_array();
        if findings.is_some_and(|findings| !findings.is_empty()) {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("FINDINGS").size(10.0).strong());
            for finding in findings.into_iter().flatten() {
                ui.horizontal_wrapped(|ui| {
                    if let Some(code) = finding["code"].as_str() {
                        ui.label(egui::RichText::new(code).strong().color(CORE_ORANGE));
                    }
                    if let Some(class) = finding["class"].as_str() {
                        badge(ui, &class.to_uppercase().replace('_', " "), muted(ui));
                    }
                    if let Some(owner) = finding["owner"].as_str() {
                        ui.label(egui::RichText::new(format!("owner: {owner}")).color(muted(ui)));
                    }
                });
                if let Some(message) = finding["message"].as_str() {
                    ui.label(message);
                }
            }
        }
    });
}

/// An untracked run's detail: the whole recorded row, with its missing
/// ancestry stated rather than guessed.
fn record_detail(ui: &mut egui::Ui, item: &Value) {
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("Recorded run · line {}", item["line"]))
                    .size(19.0)
                    .strong(),
            );
            if let Some(status) = item["status"].as_str() {
                status_badge(ui, status);
            }
            badge(ui, "NO ATTEMPT RECORD", muted(ui));
        });
        ui.label(
            egui::RichText::new(
                "This log line carries no attempt lineage record, so it has no recorded ancestry — an unknown, not a root.",
            )
            .color(muted(ui)),
        );
    });
    ui.add_space(8.0);
    outcome_detail(ui, item);
}

/// Core's typed candidate diff: added, removed, or replaced JSON pointers.
fn show_changes(ui: &mut egui::Ui, changes: &[Value]) {
    for change in changes {
        let pointer = change["pointer"].as_str().unwrap_or("");
        match change["kind"].as_str() {
            Some("added") => {
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "ADDED", egui::Color32::from_rgb(95, 197, 128));
                    ui.monospace(pointer);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(change["value"].to_string()).monospace(),
                        )
                        .wrap(),
                    );
                });
            }
            Some("removed") => {
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "REMOVED", RED);
                    ui.monospace(pointer);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("was {}", change["value"])).monospace(),
                        )
                        .wrap(),
                    );
                });
            }
            Some("replaced") => {
                ui.horizontal_wrapped(|ui| {
                    badge(ui, "REPLACED", BLUE);
                    ui.monospace(pointer);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{}  ->  {}",
                                change["before"], change["after"]
                            ))
                            .monospace(),
                        )
                        .wrap(),
                    );
                });
            }
            _ => {
                ui.horizontal_wrapped(|ui| {
                    if let Some(kind) = change["kind"].as_str() {
                        badge(ui, &kind.to_uppercase().replace('_', " "), muted(ui));
                    }
                    ui.monospace(pointer);
                });
            }
        }
    }
}

/// The comparison Core recomputed between this attempt and its bound parent.
/// CQ-03 develops this into the dedicated changed-input/changed-result view.
fn show_comparison(ui: &mut egui::Ui, comparison: &Value) {
    ui.label(
        egui::RichText::new("RECORDED PARENT COMPARISON")
            .size(10.0)
            .strong(),
    );
    if let Some(parent) = comparison["parent_attempt_id"].as_str() {
        ui.label(format!("Compared with its parent attempt {parent}."));
    }
    render_object(ui, comparison);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn example_log() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-009-ncsx-copper-discharge/search/attempts.jsonl")
    }

    fn legacy_log() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-001-shield-search/search/campaign-log.jsonl")
    }

    #[test]
    fn a_real_lineage_log_lists_root_child_and_validation() {
        let result = fetch_log(example_log().to_str().unwrap()).unwrap();
        let items = result["result"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["attempt_id"], "copper-ratio2-r0");
        assert_eq!(items[1]["attempt_id"], "copper-ratio4-r1");
        assert_eq!(items[1]["parent_attempt_id"], "copper-ratio2-r0");
        assert_eq!(
            result["result"]["summary"]["roots"],
            json!(["copper-ratio2-r0"])
        );
        assert_eq!(result["result"]["lineage_validation"], "consistent");
        // The merge must make the complete log visible at once.
        assert!(result["result"]["next_offset"].is_null());
    }

    #[test]
    fn a_legacy_log_lists_untracked_runs_without_attempt_ids() {
        let result = fetch_log(legacy_log().to_str().unwrap()).unwrap();
        let items = result["result"]["items"].as_array().unwrap();
        assert_eq!(result["result"]["summary"]["attempt_records"], 0);
        assert!(
            items.len() > 100,
            "pagination merged the whole 203-line log"
        );
        assert!(items.iter().all(|item| item["attempt_id"].is_null()));
        let (_, _, untracked) = tree(items);
        assert_eq!(untracked.len(), items.len());
    }

    #[test]
    fn attempt_detail_carries_the_recomputed_comparison() {
        let result = fetch_attempt(example_log().to_str().unwrap(), "copper-ratio4-r1").unwrap();
        let detail = &result["result"];
        assert_eq!(detail["match_status"], "found");
        assert_eq!(detail["attempt"]["attempt_id"], "copper-ratio4-r1");
        let comparison = &detail["comparison"];
        assert_eq!(comparison["parent_attempt_id"], "copper-ratio2-r0");
        assert!(
            !comparison["exact_margin_comparisons"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let missing = fetch_attempt(example_log().to_str().unwrap(), "never-existed").unwrap();
        assert_eq!(missing["result"]["match_status"], "no_match_in_record");
    }

    #[test]
    fn malformed_and_tampered_logs_surface_the_shared_error() {
        let dir = std::env::temp_dir().join(format!(
            "avila-core-history-view-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let malformed = dir.join("malformed.jsonl");
        std::fs::write(&malformed, b"{\"case_id\":\"x\"}\n{broken\n").unwrap();
        let error = fetch_log(malformed.to_str().unwrap()).unwrap_err();
        assert!(error.contains("line"), "{error}");
        // A child naming a parent the log does not carry fails the whole read.
        let orphan = dir.join("orphan.jsonl");
        let mut lines: Vec<String> = std::fs::read_to_string(example_log())
            .unwrap()
            .split('\n')
            .map(str::to_owned)
            .collect();
        let mut child: Value = serde_json::from_str(&lines[1]).unwrap();
        child["attempt"]["parent_attempt_id"] = json!("absent-parent");
        lines[1] = child.to_string();
        std::fs::write(&orphan, lines.join("\n")).unwrap();
        let error = fetch_log(orphan.to_str().unwrap()).unwrap_err();
        assert!(error.contains("absent-parent"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_the_source_drops_the_previous_selection() {
        let mut view = HistoryView::new(None, None);
        view.result = Some(json!({"result":{"items":[]}}));
        view.selected = Some(Selection::Attempt("old".into()));
        view.detail = Some(json!({}));
        view.clear_records();
        assert!(view.result.is_none());
        assert!(view.selected.is_none());
        assert!(view.detail.is_none());
    }

    #[test]
    fn selecting_a_row_and_reopening_survive_a_narrow_window() {
        let context = egui::Context::default();
        crate::configure_style(&context);
        let mut view = HistoryView::new(None, None);
        view.log_path = example_log().display().to_string();
        view.result = Some(fetch_log(&view.log_path).unwrap());
        view.selected = Some(Selection::Attempt("copper-ratio4-r1".into()));
        view.detail = Some(fetch_attempt(&view.log_path, "copper-ratio4-r1").unwrap());
        for width in [920.0, 1260.0] {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| view.ui(ui, ""),
            );
            output.textures_delta.clear();
            // The child attempt and its parent edge are both visible.
            let texts: Vec<&str> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            assert!(texts.iter().any(|text| text.contains("copper-ratio4-r1")));
            assert!(
                texts
                    .iter()
                    .any(|text| text.contains("Child of attempt copper-ratio2-r0"))
            );
        }
    }

    #[test]
    fn a_disconnected_detail_thread_reports_instead_of_hanging() {
        let (sender, receiver) = channel();
        drop(sender);
        let mut view = HistoryView::new(None, None);
        view.detail_running = Some(receiver);
        view.poll();
        assert!(view.detail_running.is_none());
        assert!(view.detail_error.is_some());
    }
}
