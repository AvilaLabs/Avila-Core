//! Human interface over the same recorded-result queries used by CLI and MCP.

use crate::{CORE_ORANGE, badge, card, muted, section_heading};
use avila_core_runner::CaseRunReport;
use avila_core_runner::query::{QueryContext, call_report_tool, call_tool};
use eframe::egui;
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tool {
    #[default]
    Overview,
    Requirements,
    Findings,
    Artifacts,
    Workflow,
    Evidence,
    Steps,
    History,
    Attempt,
    Constellation,
    Diagnostic,
}

impl Tool {
    const ALL: [Self; 11] = [
        Self::Overview,
        Self::Requirements,
        Self::Findings,
        Self::Artifacts,
        Self::Workflow,
        Self::Evidence,
        Self::Steps,
        Self::History,
        Self::Attempt,
        Self::Constellation,
        Self::Diagnostic,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Overview => "core_inspect",
            Self::Requirements => "core_requirements",
            Self::Findings => "core_findings",
            Self::Artifacts => "core_artifacts",
            Self::Workflow => "core_workflow",
            Self::Evidence => "core_evidence",
            Self::Steps => "core_steps",
            Self::History => "core_history",
            Self::Attempt => "core_attempt",
            Self::Constellation => "core_constellation",
            Self::Diagnostic => "core_explain",
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Run overview",
            Self::Requirements => "Requirements",
            Self::Findings => "Findings",
            Self::Artifacts => "Artifacts",
            Self::Workflow => "Workflow",
            Self::Evidence => "Evidence admissions",
            Self::Steps => "Execution steps",
            Self::History => "Run history",
            Self::Attempt => "Compare with parent",
            Self::Constellation => "Attempt constellation",
            Self::Diagnostic => "Explain a diagnostic",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Overview => "Read a run's recorded status and identities.",
            Self::Requirements => {
                "Inspect requirement verdicts, exact margins, evidence, and the question they answer."
            }
            Self::Findings => {
                "Find recorded blockers, their owners, and the available next actions."
            }
            Self::Artifacts => "Look up recorded document, artifact, and input identities.",
            Self::Workflow => "Inspect the compiled steps recorded for this run.",
            Self::Evidence => "See which evidence was admitted, missing, or quarantined, and why.",
            Self::Steps => "Read recorded execution and reuse decisions for each step.",
            Self::History => {
                "Search one campaign log. Matches may include planned, failed, executed, or reused steps."
            }
            Self::Attempt => {
                "Compare an attempt with its bound parent using Core's lineage checks and exact margins."
            }
            Self::Constellation => {
                "See one campaign log's whole recorded constellation: attempts, lineage edges, candidate states, and verdicts."
            }
            Self::Diagnostic => "Look up a diagnostic code and its next action in Core's catalog.",
        }
    }
    pub(crate) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| {
            tool.name() == name
                || tool.name().strip_prefix("core_") == Some(name)
                || tool.title().eq_ignore_ascii_case(name)
        })
    }
    fn report(self) -> bool {
        !matches!(
            self,
            Self::History | Self::Attempt | Self::Constellation | Self::Diagnostic
        )
    }
    fn paginated(self) -> bool {
        !matches!(self, Self::Overview | Self::Attempt | Self::Diagnostic)
    }
    fn id_label(self) -> Option<&'static str> {
        match self {
            Self::Requirements => Some("Requirement ID (optional)"),
            Self::Evidence => Some("Evidence ID (optional)"),
            Self::Steps => Some("Step ID (optional)"),
            Self::Attempt => Some("Attempt ID"),
            Self::Constellation => Some("Attempt ID (optional)"),
            Self::Diagnostic => Some("Diagnostic code"),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct ToolsView {
    tool: Tool,
    current_run: bool,
    report_path: String,
    log_path: String,
    id: String,
    case_id: String,
    invocation: String,
    page_size: usize,
    offset: usize,
    running: Option<Receiver<Result<Value, String>>>,
    result: Option<Value>,
    error: Option<String>,
    auto_query: bool,
    report_snapshot: Option<Vec<u8>>,
}

impl ToolsView {
    pub(crate) fn new(path: Option<String>, tool: Option<&str>) -> Self {
        let selected = tool.and_then(Tool::by_name).unwrap_or_default();
        let mut view = Self {
            tool: selected,
            page_size: 20,
            current_run: path.is_none(),
            auto_query: path.is_some(),
            ..Default::default()
        };
        if let Some(path) = path {
            if selected.report() {
                view.report_path = path;
            } else {
                view.log_path = path;
            }
        }
        view
    }

    pub(crate) fn settled(&self) -> bool {
        self.running.is_none() && !self.auto_query
    }

    fn clear_result(&mut self) {
        self.result = None;
        self.error = None;
        self.offset = 0;
        self.report_snapshot = None;
    }

    fn arguments(&self, offset: usize, has_current: bool) -> Result<Value, String> {
        let mut args = json!({});
        if self.tool != Tool::Diagnostic {
            if self.tool.report() && self.current_run {
                if !has_current {
                    return Err(
                        "Run or plan a case in the workbench, or select a saved report.".into(),
                    );
                }
            } else {
                let path = if self.tool.report() {
                    &self.report_path
                } else {
                    &self.log_path
                };
                if path.trim().is_empty() {
                    return Err("Choose a report or campaign log by pasting its path or dropping the file here.".into());
                }
                args["path"] = json!(path.trim());
            }
        }
        if self.tool.id_label().is_some() {
            if !self.id.trim().is_empty() {
                args["id"] = json!(self.id.trim());
            } else if matches!(self.tool, Tool::Attempt | Tool::Diagnostic) {
                return Err("Enter the attempt ID or diagnostic code.".into());
            }
        }
        if self.tool == Tool::History && !self.invocation.trim().is_empty() {
            args["invocation"] = json!(self.invocation.trim());
        }
        if matches!(self.tool, Tool::History | Tool::Constellation)
            && !self.case_id.trim().is_empty()
        {
            args["case_id"] = json!(self.case_id.trim());
        }
        if self.tool.paginated() {
            args["offset"] = json!(offset);
            args["limit"] = json!(self.page_size);
        }
        Ok(args)
    }

    fn start(
        &mut self,
        offset: usize,
        current: Option<&CaseRunReport>,
        context: &egui::Context,
        reuse_snapshot: bool,
    ) {
        if self.running.is_some() {
            return;
        }
        let args = match self.arguments(offset, current.is_some()) {
            Ok(args) => args,
            Err(error) => {
                self.result = None;
                self.error = Some(error);
                return;
            }
        };
        let bytes = if self.tool.report() && self.current_run {
            if reuse_snapshot && self.report_snapshot.is_some() {
                self.report_snapshot.clone()
            } else {
                match serde_json::to_vec(current.unwrap()) {
                    Ok(bytes) => Some(bytes),
                    Err(error) => {
                        self.result = None;
                        self.error = Some(error.to_string());
                        return;
                    }
                }
            }
        } else {
            None
        };
        self.report_snapshot = bytes.clone();
        let name = self.tool.name();
        let context = context.clone();
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let result = match bytes {
                Some(bytes) => call_report_tool(name, args, &bytes),
                None => call_tool(&QueryContext::unrestricted(), name, args),
            };
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.running = Some(receiver);
        self.error = None;
        self.result = None;
        self.offset = offset;
    }

    fn poll(&mut self) {
        let Some(receiver) = &self.running else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(value)) => {
                self.result = Some(value);
                self.running = None;
            }
            Ok(Err(error)) => {
                self.error = Some(error);
                self.running = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.error = Some("The query ended without a result. Try again.".into());
                self.running = None;
            }
        }
    }

    pub(crate) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        current: Option<&CaseRunReport>,
        workbench_log: &str,
    ) {
        self.poll();
        if std::mem::take(&mut self.auto_query) {
            self.start(0, current, ui.ctx(), false);
        }
        let busy = self.running.is_some();
        egui::Panel::left("tools-navigation").resizable(true).default_size(235.0).show(ui,|ui| {
            ui.set_min_width(205.0);
            egui::ScrollArea::vertical().id_salt("tools-list").show(ui,|ui| {
                ui.add_enabled_ui(!busy,|ui| {
                    for tool in Tool::ALL {
                        if tool==Tool::Overview {ui.label(egui::RichText::new("RUN REPORTS").small().color(muted(ui)));}
                        if tool==Tool::History {ui.separator();ui.label(egui::RichText::new("CAMPAIGN LOGS").small().color(muted(ui)));}
                        if tool==Tool::Diagnostic {ui.separator();ui.label(egui::RichText::new("REFERENCE").small().color(muted(ui)));}
                        if ui.selectable_label(self.tool==tool,tool.title()).clicked() && self.tool!=tool {
                            self.tool=tool;self.id.clear();self.clear_result();
                        }
                    }
                });
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Queries read records. Use the Case workbench to run capabilities or check current reuse.").small().color(muted(ui)));
            });
        });
        egui::CentralPanel::default().show(ui,|ui| {
            section_heading(ui,self.tool.title(),self.tool.description());
            let mut changed=false;
            ui.add_enabled_ui(!busy,|ui| {
                if self.tool.report() {
                    ui.horizontal_wrapped(|ui| {
                        changed|=ui.selectable_value(&mut self.current_run,true,"Current workbench run").changed();
                        changed|=ui.selectable_value(&mut self.current_run,false,"Saved report").changed();
                    });
                    if self.current_run {
                        ui.label(match current {Some(report)=>format!("{} — {}",report.case_id,report.title),None=>"No workbench report yet. Run or plan a case, or choose Saved report.".into()});
                    }
                }
                if self.tool!=Tool::Diagnostic && !(self.tool.report() && self.current_run) {
                    let report=self.tool.report();
                    ui.label(if report {"Report file"} else {"Campaign log"});
                    let path=if report {&mut self.report_path} else {&mut self.log_path};
                    changed|=ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY)
                        .hint_text(if report {"Paste a path to run-report.json, or drop the file here"} else {"Paste a path to campaign.jsonl, or drop the file here"})).changed();
                    if !report && !workbench_log.trim().is_empty() && ui.button("Use workbench log").clicked() {
                        self.log_path=workbench_log.trim().into();changed=true;
                    }
                }
                if let Some(label)=self.tool.id_label() {
                    ui.horizontal(|ui| {ui.label(label);changed|=ui.add(egui::TextEdit::singleline(&mut self.id).desired_width(280.0)).changed();});
                }
                if matches!(self.tool,Tool::History|Tool::Constellation) {
                    ui.horizontal(|ui| {ui.label("Case ID (optional)");changed|=ui.text_edit_singleline(&mut self.case_id).changed();});
                }
                if self.tool==Tool::History {
                    ui.horizontal(|ui| {ui.label("Invocation hash (optional)");changed|=ui.add(egui::TextEdit::singleline(&mut self.invocation).desired_width(f32::INFINITY).hint_text("sha256:…")).changed();});
                }
                ui.horizontal(|ui| {
                    if self.tool.paginated() {ui.label("Results per page");changed|=ui.add(egui::DragValue::new(&mut self.page_size).range(1..=100)).changed();}
                    if changed {self.clear_result();}
                    if ui.button(if self.tool==Tool::Diagnostic {"Explain"} else {"Run query"}).clicked() {self.start(0,current,ui.ctx(),false);}
                });
            });
            if self.running.is_none() && self.tool!=Tool::Diagnostic {
                let dropped=ui.ctx().input(|input|input.raw.dropped_files.first().map(|file|file.path().to_path_buf()));
                if let Some(path)=dropped {
                    if self.tool.report() {self.current_run=false;self.report_path=path.display().to_string();}
                    else {self.log_path=path.display().to_string();}
                    self.clear_result();
                }
            }
            ui.separator();
            if self.running.is_some() {ui.horizontal(|ui| {ui.spinner();ui.label("Reading records…");});}
            if let Some(error)=&self.error {ui.colored_label(egui::Color32::LIGHT_RED,error);}
            let mut next=None;
            if let Some(value)=&self.result {
                ui.horizontal_wrapped(|ui| {
                    if value.get("source").is_some() {badge(ui,"RECORDED RESULT",CORE_ORANGE);}
                    if let Some(case_id)=value["result"]["case_id"].as_str() {ui.label(case_id);}
                    if ui.button("Copy JSON").clicked() {ui.ctx().copy_text(serde_json::to_string_pretty(value).unwrap_or_default());}
                    if ui.button("Copy readable result").clicked() {ui.ctx().copy_text(avila_core_runner::query::human_query(value));}
                    let result=&value["result"];
                    if let Some(total)=result["total"].as_u64() {
                        let count=result["items"].as_array().map_or(0,Vec::len);
                        ui.label(if count==0 {format!("0 shown · {total} matches")} else {format!("{}–{} of {total}",self.offset+1,self.offset+count)});
                        if ui.add_enabled(self.offset>0,egui::Button::new("Previous")).clicked() {next=Some(self.offset.saturating_sub(self.page_size));}
                        let more=result["next_offset"].as_u64();
                        if ui.add_enabled(more.is_some(),egui::Button::new("Next")).clicked() {next=more.map(|n|n as usize);}
                    }
                });
                if let Some(source)=value.get("source") {
                    ui.add(egui::Label::new(egui::RichText::new(source["path"].as_str().or_else(||source["label"].as_str()).unwrap_or("")).small()).wrap());
                    ui.add(egui::Label::new(egui::RichText::new(source["sha256"].as_str().unwrap_or("")).small().monospace().color(muted(ui))).wrap());
                    ui.label(egui::RichText::new("These observations come from the identified report. This query does not recheck artifact files, signatures, or current reuse.").small().color(muted(ui)));
                }
                egui::ScrollArea::vertical().id_salt("tool-results").auto_shrink([false,false]).show(ui,|ui| {
                    let result=value.get("result").unwrap_or(value);
                    if let Some(items)=result["items"].as_array() {
                        if items.is_empty() {ui.label("No matching entries in this record.");}
                        for (index,item) in items.iter().enumerate() {
                            ui.push_id(index,|ui| {card(ui,|ui|render_entry(ui,item));});ui.add_space(6.0);
                        }
                    } else {card(ui,|ui|render_object(ui,result));}
                });
            } else if self.running.is_none() && self.error.is_none() {
                ui.add_space(16.0);ui.label(egui::RichText::new("Choose a source and run the query to see results here.").color(muted(ui)));
            }
            if let Some(offset)=next {self.start(offset,current,ui.ctx(),true);}
        });
    }
}

fn render_entry(ui: &mut egui::Ui, item: &Value) {
    ui.set_min_width(ui.available_width());
    let title = [
        "/requirement_id",
        "/step_id",
        "/evidence_id",
        "/code",
        "/record/path",
        "/record/input_id",
        "/case_id",
    ]
    .into_iter()
    .find_map(|pointer| item.pointer(pointer).and_then(Value::as_str));
    let state = ["/verdict/status", "/state", "/status", "/record/state"]
        .into_iter()
        .find_map(|pointer| item.pointer(pointer).and_then(Value::as_str));
    ui.horizontal_wrapped(|ui| {
        if let Some(title) = title {
            ui.label(egui::RichText::new(title).strong());
        }
        if let Some(state) = state {
            let color = match state {
                "pass" | "admitted" => egui::Color32::from_rgb(95, 197, 128),
                "fail" | "failed" | "quarantined" => egui::Color32::from_rgb(232, 102, 102),
                "inconclusive" => CORE_ORANGE,
                _ => muted(ui),
            };
            badge(ui, &state.to_uppercase().replace('_', " "), color);
        }
        if item["recorded_verdict_available"] == false {
            ui.label("No verdict recorded");
        }
        if let Some(kind) = item["kind"].as_str() {
            ui.label(kind.replace('_', " "));
        }
    });
    if let Some(recorded_at) = item["recorded_at"].as_str() {
        ui.label(egui::RichText::new(recorded_at).small().color(muted(ui)));
    }
    if let Some(attempt) = item["attempt_id"].as_str() {
        ui.label(format!("Attempt: {attempt}"));
    }
    if let Some(statement) = item["statement"]
        .as_str()
        .or_else(|| item["message"].as_str())
    {
        ui.label(statement);
    }
    if let Some(margin) = item["margin_record"]["margin"].as_str() {
        ui.label(format!(
            "Exact margin: {margin} {}",
            item["margin_record"]["unit"].as_str().unwrap_or("")
        ));
    }
    if let Some(steps) = item["steps"].as_array() {
        for step in steps {
            if let (Some(id), Some(state)) = (step["step_id"].as_str(), step["state"].as_str()) {
                ui.label(format!("{id}: {}", state.replace('_', " ")));
            }
        }
    }
    egui::CollapsingHeader::new("Details").show(ui, |ui| render_object(ui, item));
}

fn render_object(ui: &mut egui::Ui, value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let label = key.replace('_', " ");
                if value.is_array() || value.is_object() {
                    egui::CollapsingHeader::new(label)
                        .id_salt(key)
                        .default_open(key == "verdict")
                        .show(ui, |ui| render_object(ui, value));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(format!("{label}:")).strong());
                        ui.add(
                            egui::Label::new(
                                value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string()),
                            )
                            .wrap()
                            .selectable(true),
                        );
                    });
                }
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                ui.label("No entries recorded.");
            }
            for (index, item) in items.iter().enumerate() {
                ui.push_id(index, |ui| {
                    render_object(ui, item);
                    ui.separator();
                });
            }
        }
        _ => {
            ui.add(
                egui::Label::new(
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                )
                .wrap()
                .selectable(true),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_shared_tool_has_a_human_entry_point() {
        let mut shared: Vec<String> = avila_core_runner::query::tool_catalog()
            .iter()
            .map(|t| t["name"].as_str().unwrap().into())
            .collect();
        let mut ui: Vec<String> = Tool::ALL.iter().map(|tool| tool.name().into()).collect();
        shared.sort();
        ui.sort();
        assert_eq!(shared, ui);
    }
    #[test]
    fn forms_only_send_arguments_supported_by_the_selected_tool() {
        let mut view = ToolsView::new(Some("report.json".into()), None);
        view.id = "r".into();
        view.invocation = "ignored".into();
        assert_eq!(
            view.arguments(0, false).unwrap(),
            json!({"path":"report.json"})
        );
        view.tool = Tool::Requirements;
        assert_eq!(
            view.arguments(20, false).unwrap(),
            json!({"path":"report.json","id":"r","offset":20,"limit":20})
        );
        view.tool = Tool::Diagnostic;
        assert_eq!(view.arguments(0, false).unwrap(), json!({"id":"r"}));
        view.tool = Tool::Overview;
        view.current_run = true;
        assert!(view.arguments(0, false).is_err());
        assert_eq!(view.arguments(0, true).unwrap(), json!({}));
    }
    #[test]
    fn a_disconnected_query_clears_busy_state_and_reports_error() {
        let (sender, receiver) = channel();
        drop(sender);
        let mut view = ToolsView::new(None, None);
        view.running = Some(receiver);
        view.poll();
        assert!(view.running.is_none());
        assert!(view.error.is_some());
    }

    #[test]
    fn previous_page_retains_snapshot_until_an_explicit_new_query() {
        let case = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/cases/case-003-thermal-spreader");
        let mut report = avila_core_runner::execute_case(&case, &Default::default()).unwrap();
        let mut view = ToolsView::new(None, None);
        let context = egui::Context::default();
        view.start(0, Some(&report), &context, false);
        let first = view
            .running
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        report.title = "A different workbench run".into();
        // Previous can return to offset zero; it must still use the old bytes.
        view.start(0, Some(&report), &context, true);
        let previous = view
            .running
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(previous, first);
        view.start(0, Some(&report), &context, false);
        let refreshed = view
            .running
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_ne!(refreshed["source"]["sha256"], first["source"]["sha256"]);
        assert_eq!(refreshed["result"]["title"], "A different workbench run");
        view.clear_result();
        assert!(view.report_snapshot.is_none());
    }
}
