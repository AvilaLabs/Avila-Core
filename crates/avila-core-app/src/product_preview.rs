#![forbid(unsafe_code)]
//! Disposable product concept. All project data and transitions are simulated.
//! No compiler, runner, solver, network, or persistent project operations.
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(17, 20, 25);
const PANEL: Color32 = Color32::from_rgb(23, 27, 33);
const CARD: Color32 = Color32::from_rgb(29, 34, 41);
const LINE: Color32 = Color32::from_rgb(48, 55, 64);
const TEXT: Color32 = Color32::from_rgb(231, 235, 239);
const MUTED: Color32 = Color32::from_rgb(154, 165, 179);
const ORANGE: Color32 = Color32::from_rgb(255, 151, 74);
const GREEN: Color32 = Color32::from_rgb(117, 209, 169);
const BLUE: Color32 = Color32::from_rgb(135, 181, 234);
const RED: Color32 = Color32::from_rgb(241, 132, 135);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Projects,
    Library,
    Compute,
    Organization,
    Project,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Design,
    Methods,
    Explore,
    Evidence,
    History,
}
impl Tab {
    const ALL: [Self; 6] = [
        Self::Overview,
        Self::Design,
        Self::Methods,
        Self::Explore,
        Self::Evidence,
        Self::History,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Design => "Design",
            Self::Methods => "Methods",
            Self::Explore => "Explore",
            Self::Evidence => "Evidence",
            Self::History => "History",
        }
    }
}
#[derive(Clone, Copy)]
struct Candidate {
    name: &'static str,
    material: &'static str,
    thickness: u32,
    mass: f32,
    dose: f32,
    heat: f32,
}
const CANDIDATES: [Candidate; 3] = [
    Candidate {
        name: "Original design",
        material: "Reference steel",
        thickness: 24,
        mass: 120.0,
        dose: 8.4,
        heat: 32.0,
    },
    Candidate {
        name: "Lightweight alternative",
        material: "Low-activation alloy",
        thickness: 18,
        mass: 96.0,
        dose: 11.2,
        heat: 25.0,
    },
    Candidate {
        name: "Balanced alternative",
        material: "Low-activation alloy",
        thickness: 21,
        mass: 106.0,
        dose: 7.6,
        heat: 27.0,
    },
];
struct Preview {
    page: Page,
    tab: Tab,
    title: String,
    draft_title: String,
    candidate: usize,
    baseline: usize,
    assessed: [bool; 3],
    available: [bool; 3],
    agent: bool,
    budget: u32,
    running: Option<Instant>,
    exploring: bool,
    run_candidate: usize,
    progress: f32,
    completed: bool,
    search: String,
    category: usize,
    source: usize,
    selected_package: usize,
    installed: [bool; 4],
    location: usize,
    dialog: Option<&'static str>,
    notice: String,
    events: Vec<String>,
    logo: Option<egui::TextureHandle>,
    screenshot: Option<String>,
    frames: usize,
}
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let mut app = Preview::new();
    if let Some(i) = args.iter().position(|a| a == "--screen") {
        match args.get(i + 1).map(String::as_str).unwrap_or("projects") {
            "library" => app.page = Page::Library,
            "compute" => app.page = Page::Compute,
            "organization" => app.page = Page::Organization,
            "projects" => {}
            screen => {
                app.page = Page::Project;
                app.tab = Tab::ALL
                    .into_iter()
                    .find(|t| t.label().to_lowercase() == screen)
                    .unwrap_or(Tab::Overview);
            }
        }
    }
    if args.iter().any(|a| a == "--completed") {
        app.finish_run(true);
    }
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        app.screenshot = args.get(i + 1).cloned();
    }
    eframe::run_native(
        "Avila Core — Product Preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 960.0])
                .with_min_inner_size([1060.0, 740.0]),
            persist_window: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            if let Ok(img) = image::load_from_memory(include_bytes!(
                "../../../assets/branding/Avila_Core_Logo.png"
            )) {
                let rgba = img.to_rgba8();
                app.logo = Some(cc.egui_ctx.load_texture(
                    "core-logo",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        rgba.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                ));
            }
            Ok(Box::new(app))
        }),
    )
}
fn style(ctx: &egui::Context) {
    ctx.set_zoom_factor(0.85);
    ctx.set_theme(egui::Theme::Dark);
    let mut s = (*ctx.style_of(egui::Theme::Dark)).clone();
    s.visuals.panel_fill = BG;
    s.visuals.window_fill = PANEL;
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.faint_bg_color = CARD;
    s.visuals.extreme_bg_color = BG;
    s.visuals.selection.bg_fill = Color32::from_rgb(95, 61, 37);
    s.visuals.selection.stroke = Stroke::new(1.0, ORANGE);
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    s.visuals.widgets.inactive.weak_bg_fill = CARD;
    s.visuals.widgets.inactive.bg_fill = CARD;
    s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 59, 70);
    s.spacing.item_spacing = Vec2::new(12.0, 10.0);
    s.spacing.button_padding = Vec2::new(14.0, 9.0);
    s.spacing.interact_size.y = 34.0;
    s.text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.0));
    s.text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(27.0));
    s.text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    ctx.set_style_of(egui::Theme::Dark, s);
}
fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED));
}
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(ORANGE));
}
fn title(ui: &mut egui::Ui, heading: &str, subtitle: &str) {
    ui.heading(heading);
    muted(ui, subtitle);
    ui.add_space(14.0);
}
fn card<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(10)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            f(ui)
        })
        .inner
}
fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(color));
        });
}
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(text).color(BG).strong())
            .fill(ORANGE)
            .corner_radius(6),
    )
    .clicked()
}
fn metric(ui: &mut egui::Ui, label: &str, value: String, detail: &str, color: Color32) {
    card(ui, |ui| {
        muted(ui, label);
        ui.label(RichText::new(value).size(31.0).color(color));
        ui.label(RichText::new(detail).size(12.0).color(MUTED));
    });
}
impl Preview {
    fn new() -> Self {
        Self {
            page: Page::Projects,
            tab: Tab::Overview,
            title: "Irradiated component study".into(),
            draft_title: "New component assessment".into(),
            candidate: 0,
            baseline: 0,
            assessed: [true, false, false],
            available: [true, false, false],
            agent: true,
            budget: 24,
            running: None,
            exploring: false,
            run_candidate: 0,
            progress: 0.0,
            completed: false,
            search: String::new(),
            category: 0,
            source: 0,
            selected_package: 0,
            installed: [true, true, false, true],
            location: 0,
            dialog: None,
            notice: String::new(),
            events: vec!["Baseline recorded · Original design · illustrative assessment".into()],
            logo: None,
            screenshot: None,
            frames: 0,
        }
    }
    fn start_run(&mut self, explore: bool) {
        self.running = Some(Instant::now());
        self.exploring = explore;
        self.run_candidate = self.candidate;
        self.progress = 0.0;
        self.notice = if explore {
            "Exploration started. Requirements are fixed; all activity is simulated."
        } else {
            "Assessment started. This preview will play a scripted result; no solver is running."
        }
        .into();
    }
    fn finish_run(&mut self, explore: bool) {
        self.running = None;
        self.progress = 1.0;
        if explore {
            self.assessed = [true; 3];
            self.available = [true; 3];
            self.completed = true;
            self.candidate = 2;
            self.events.push(format!(
                "{} exploration completed · 3 illustrative alternatives · fixed requirements",
                if self.agent {
                    "Agent"
                } else {
                    "Parameter-sweep"
                }
            ));
            self.notice =
                "Exploration complete. Compare the alternatives, including the one that failed."
                    .into();
        } else {
            self.assessed[self.run_candidate] = true;
            self.candidate = self.run_candidate;
            self.events.push(format!(
                "Assessed {} · simulated evidence attached",
                CANDIDATES[self.candidate].name
            ));
            self.notice =
                "Illustrative assessment complete. Open Evidence to inspect the result.".into();
        }
    }
    fn accept(&mut self) {
        if self.assessed[self.candidate] && CANDIDATES[self.candidate].dose <= 10.0 {
            self.baseline = self.candidate;
            self.events.push(format!(
                "You accepted {} as the baseline · preview only",
                CANDIDATES[self.candidate].name
            ));
            self.notice =
                "Baseline updated in this preview session. Technical results are unchanged.".into();
            self.dialog = None;
        }
    }
    fn sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .exact_size(218.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(18))
            .show(root, |ui| {
                ui.add_space(8.0);
                if let Some(logo) = &self.logo {
                    ui.add(
                        egui::Image::new(logo)
                            .uv(egui::Rect::from_min_max(
                                egui::pos2(0.04, 0.32),
                                egui::pos2(0.96, 0.70),
                            ))
                            .fit_to_exact_size(Vec2::new(160.0, 66.0))
                            .maintain_aspect_ratio(false),
                    );
                }
                ui.add_space(14.0);
                muted(ui, "AVILA LABS / WORKSPACE");
                ui.add_space(12.0);
                for (page, name) in [
                    (Page::Projects, "Projects"),
                    (Page::Library, "Library"),
                    (Page::Compute, "Compute"),
                    (Page::Organization, "Organization"),
                ] {
                    let selected = self.page == page;
                    let b = egui::Button::new(RichText::new(name).color(if selected {
                        ORANGE
                    } else {
                        TEXT
                    }))
                    .fill(if selected {
                        Color32::from_rgb(53, 39, 30)
                    } else {
                        PANEL
                    })
                    .stroke(Stroke::NONE);
                    if ui.add_sized([182.0, 39.0], b).clicked() {
                        self.page = page;
                    }
                }
                ui.add_space(24.0);
                ui.separator();
                ui.add_space(12.0);
                muted(ui, "PINNED PROJECT");
                if ui
                    .add(egui::Button::new("Component study").selected(self.page == Page::Project))
                    .clicked()
                {
                    self.page = Page::Project;
                }
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    muted(ui, "All data stays in this demo.");
                    badge(ui, "SIMULATED WORKSPACE", ORANGE);
                    ui.add_space(6.0);
                    if ui.button("Reset preview").clicked() {
                        let logo = self.logo.take();
                        let capture = self.screenshot.take();
                        *self = Self::new();
                        self.logo = logo;
                        self.screenshot = capture;
                    }
                });
            });
    }
    fn home(&mut self, ui: &mut egui::Ui) {
        eyebrow(ui, "YOUR ENGINEERING WORKSPACE");
        title(
            ui,
            "Good work starts with a clear question.",
            "Pick up a project, explore a method, or start something new.",
        );
        ui.horizontal(|ui| {
            if primary(ui, "New project") {
                self.page = Page::Library;
                self.category = 0;
            }
            if ui.button("Open example evidence bundle").clicked() {
                self.page = Page::Project;
                self.tab = Tab::Evidence;
                self.notice =
                    "Opened the bundled illustrative evidence. No file was imported.".into();
            }
        });
        ui.add_space(22.0);
        ui.columns(3, |c| {
            metric(
                &mut c[0],
                "Workspace",
                "1 project".into(),
                "Private · on this device",
                TEXT,
            );
            metric(
                &mut c[1],
                "Available capabilities",
                format!(
                    "{} connected",
                    self.installed.iter().filter(|b| **b).count()
                ),
                "Illustrative package connections",
                TEXT,
            );
            metric(
                &mut c[2],
                "Current baseline",
                if self.baseline == 0 {
                    "Original"
                } else {
                    "Balanced"
                }
                .into(),
                "Exact revision retained",
                GREEN,
            );
        });
        ui.add_space(26.0);
        ui.label(RichText::new("Recent project").size(19.0).strong());
        card(ui, |ui| {
            ui.horizontal(|ui| {
                eyebrow(ui, "NUCLEAR ENGINEERING");
                badge(ui, "PRIVATE", MUTED);
            });
            ui.add_space(9.0);
            ui.label(RichText::new(&self.title).size(24.0).strong());
            muted(
                ui,
                "Reduce component mass while keeping shutdown dose below the fixed limit.",
            );
            ui.add_space(16.0);
            ui.horizontal_wrapped(|ui| {
                badge(ui, "3 requirements", BLUE);
                badge(
                    ui,
                    if self.completed {
                        "3 alternatives"
                    } else {
                        "1 baseline"
                    },
                    GREEN,
                );
                muted(ui, "Transport  /  Activation  /  Dose");
            });
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if primary(ui, "Open workspace") {
                    self.page = Page::Project;
                    self.tab = Tab::Overview;
                }
                if ui.button("Explore alternatives").clicked() {
                    self.page = Page::Project;
                    self.tab = Tab::Explore;
                }
            });
        });
        ui.add_space(22.0);
        ui.columns(2, |c| {
            card(&mut c[0], |ui| {
                eyebrow(ui, "NEXT STEP"); ui.label(RichText::new(if self.completed { "Your exploration is ready to review" } else { "What if this component used less material?" }).size(18.0));
                muted(ui, "Compare changes against the same question. Keep the evidence behind every alternative.");
                if ui.button("Open comparison").clicked() { self.page = Page::Project; self.tab = Tab::Explore; }
            });
            card(&mut c[1], |ui| {
                eyebrow(ui, "DISCOVER"); ui.label(RichText::new("Start with a method you can inspect").size(18.0));
                muted(ui, "Browse workflows, methods, and datasets. See who maintains them and where they apply.");
                if ui.button("Browse library").clicked() { self.page = Page::Library; }
            });
        });
    }
    fn library(&mut self, ui: &mut egui::Ui) {
        eyebrow(ui, "KNOWLEDGE, READY TO USE");
        title(
            ui,
            "Library",
            "Bring a workflow into your project. Keep its sources, assumptions, and versions visible.",
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search workflows, methods, datasets…")
                    .desired_width(310.0),
            );
            egui::ComboBox::from_id_salt("source")
                .selected_text(
                    ["All sources", "Public library", "Avila Labs workspace"][self.source],
                )
                .show_ui(ui, |ui| {
                    for (i, s) in ["All sources", "Public library", "Avila Labs workspace"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut self.source, i, *s);
                    }
                });
        });
        ui.horizontal(|ui| {
            for (i, s) in [
                "All",
                "Workflows",
                "Methods",
                "Datasets",
                "Requirement sets",
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut self.category, i, *s);
            }
        });
        ui.add_space(14.0);
        let packages = [
            (
                "Irradiated component assessment",
                "WORKFLOW",
                "Avila Labs · public example",
                "Transport, activation, and dose in one guided project.",
                1,
                1,
            ),
            (
                "ACTINV",
                "METHOD",
                "Avila Labs · public example",
                "Activation and inventory capability with explicit input roles.",
                2,
                1,
            ),
            (
                "OpenBNCT evidence checker",
                "METHOD",
                "Avila Labs · public example",
                "Regenerate and inspect a nuclear-data evidence chain.",
                2,
                1,
            ),
            (
                "Reference material collection",
                "DATASET",
                "Avila Labs workspace · private example",
                "Versioned compositions with declared source records.",
                3,
                2,
            ),
            (
                "Component screening requirements",
                "REQUIREMENTS",
                "Avila Labs workspace · private example",
                "Three fixed gates for this illustrative design study.",
                4,
                2,
            ),
        ];
        ui.columns(2, |cols| {
            let mut count = 0;
            for (i, (name, kind, owner, desc, category, source)) in packages.iter().enumerate() {
                if self.category != 0 && self.category != *category || self.source != 0 && self.source != *source || !format!("{name} {desc}").to_lowercase().contains(&self.search.to_lowercase()) { continue; }
                count += 1;
                card(&mut cols[0], |ui| {
                    eyebrow(ui, kind); ui.label(RichText::new(*name).size(19.0).strong()); muted(ui, *owner); ui.label(*desc);
                    if ui.button(if self.selected_package == i { "Selected" } else { "Inspect package" }).clicked() { self.selected_package = i; }
                }); cols[0].add_space(10.0);
            }
            if count == 0 { muted(&mut cols[0], "No examples match these filters. Try All sources or clear your search."); }
            let i = self.selected_package;
            card(&mut cols[1], |ui| {
                eyebrow(ui, "PACKAGE DETAILS"); ui.label(RichText::new(packages[i].0).size(22.0).strong());
                badge(ui, "ILLUSTRATIVE CATALOG ENTRY", ORANGE);
                ui.add_space(8.0); muted(ui, "Publisher"); ui.label(packages[i].2);
                muted(ui, "Version"); ui.label("Preview 1 · pinned when added");
                muted(ui, "Scope & evidence"); ui.label("Research workflow example. No qualification or organization approval is asserted by this preview.");
                ui.separator();
                if i == 0 {
                    ui.label("Included in this workflow");
                    for s in ["Three explicit assessment requirements", "Transport -> inventory -> dose chain", "Reference material and exposure inputs", "A baseline with inspectable example evidence"] { muted(ui, s); }
                    ui.add_space(12.0);
                    if primary(ui, "Create project from workflow") { self.dialog = Some("create"); }
                } else if i < 4 {
                    let installed = self.installed[i];
                    if primary(ui, if installed { "Connected · inspect in Methods" } else { "Connect capability (simulated)" }) {
                        if installed { self.page = Page::Project; self.tab = Tab::Methods; }
                        else { self.installed[i] = true; self.notice = format!("{} connected in this demo. No package was downloaded.", packages[i].0); }
                    }
                } else if primary(ui, "Inspect project requirements") { self.page = Page::Project; self.tab = Tab::Overview; }
                ui.add_space(14.0); muted(ui, "In the full product: validation records, release history, licenses, and compatible execution environments.");
            });
        });
    }
    fn project(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            eyebrow(ui, "PROJECT / PRIVATE WORKSPACE");
            badge(ui, "ILLUSTRATIVE STUDY", ORANGE);
        });
        ui.heading(&self.title);
        muted(
            ui,
            format!(
                "Execution: {} · simulated connection",
                ["This workstation", "Organization cluster", "Managed cloud"][self.location]
            ),
        );
        ui.horizontal_wrapped(|ui| {
            muted(ui, "Fixed question");
            ui.label("Can we reduce mass while keeping shutdown dose below 10 µSv/h?");
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for tab in Tab::ALL {
                if ui.selectable_label(self.tab == tab, tab.label()).clicked() {
                    self.tab = tab;
                }
            }
        });
        ui.separator();
        ui.add_space(14.0);
        match self.tab {
            Tab::Overview => self.overview(ui),
            Tab::Design => self.design(ui),
            Tab::Methods => self.methods(ui),
            Tab::Explore => self.explore(ui),
            Tab::Evidence => self.evidence(ui),
            Tab::History => self.history(ui),
        }
    }
    fn requirements(&self, ui: &mut egui::Ui) {
        let c = CANDIDATES[self.candidate];
        let assessed = self.assessed[self.candidate];
        egui::Grid::new("requirements")
            .num_columns(4)
            .spacing([26.0, 17.0])
            .striped(true)
            .show(ui, |ui| {
                for h in ["Requirement", "Limit", "Observed", "Assessment"] {
                    muted(ui, h);
                }
                ui.end_row();
                for (name, limit, value, pass) in [
                    (
                        "R-01  Shutdown dose",
                        "≤ 10 µSv/h",
                        format!("{:.1} µSv/h", c.dose),
                        c.dose <= 10.0,
                    ),
                    (
                        "R-02  Component mass",
                        "≤ 120 kg",
                        format!("{:.0} kg", c.mass),
                        c.mass <= 120.0,
                    ),
                    (
                        "R-03  Decay heat",
                        "≤ 40 W",
                        format!("{:.0} W", c.heat),
                        c.heat <= 40.0,
                    ),
                ] {
                    ui.label(name);
                    ui.label(limit);
                    ui.label(if assessed { value } else { "—".into() });
                    badge(
                        ui,
                        if !assessed {
                            "NOT EVALUATED"
                        } else if pass {
                            "PASS"
                        } else {
                            "FAIL"
                        },
                        if !assessed {
                            MUTED
                        } else if pass {
                            GREEN
                        } else {
                            RED
                        },
                    );
                    ui.end_row();
                }
            });
    }
    fn overview(&mut self, ui: &mut egui::Ui) {
        let c = CANDIDATES[self.candidate];
        let assessed = self.assessed[self.candidate];
        ui.horizontal(|ui| {
            ui.label(RichText::new(c.name).size(22.0).strong());
            badge(
                ui,
                if self.candidate == self.baseline {
                    "BASELINE"
                } else {
                    "ALTERNATIVE"
                },
                BLUE,
            );
        });
        ui.add_space(12.0);
        ui.columns(3, |cols| {
            metric(
                &mut cols[0],
                "Component mass",
                format!("{:.0} kg", c.mass),
                "Illustrative design value",
                TEXT,
            );
            metric(
                &mut cols[1],
                "Shutdown dose",
                if assessed {
                    format!("{:.1} µSv/h", c.dose)
                } else {
                    "Not evaluated".into()
                },
                "Requirement ≤ 10 µSv/h",
                if assessed && c.dose > 10.0 {
                    RED
                } else {
                    GREEN
                },
            );
            metric(
                &mut cols[2],
                "Evidence state",
                if assessed { "Assessed" } else { "Pending" }.into(),
                "Simulated results · not physical predictions",
                BLUE,
            );
        });
        ui.add_space(18.0);
        card(ui, |ui| {
            ui.label(
                RichText::new("Requirements stay fixed as the design changes")
                    .size(19.0)
                    .strong(),
            );
            ui.add_space(12.0);
            self.requirements(ui);
        });
        ui.add_space(18.0);
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                eyebrow(ui, "NEXT ACTION"); ui.label(RichText::new("Explore the design space").size(20.0));
                muted(ui, "Try a material and thickness alternative yourself, or delegate a bounded exploration.");
                ui.horizontal(|ui| { if primary(ui, "Change design") { self.tab = Tab::Design; }
                if ui.button("Explore").clicked() { self.tab = Tab::Explore; } });
            });
            card(&mut cols[1], |ui| {
                eyebrow(ui, "ASSESSMENT BOUNDARY"); ui.label("Reference exposure · cooling time 30 days");
                muted(ui, "All values and method records in this preview are invented. Applicability would be established for each real project.");
                if ui.button("Inspect evidence").clicked() { self.tab = Tab::Evidence; }
            });
        });
    }
    fn design(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "Make a change. See what it affects.",
            "Choose an illustrative design revision. The baseline remains available for comparison.",
        );
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                eyebrow(ui, "DESIGN REVISION");
                for (i, c) in CANDIDATES.iter().enumerate() {
                    if ui.add_enabled(self.running.is_none(), egui::RadioButton::new(self.candidate == i, c.name)).clicked() { self.candidate = i; self.available[i] = true; }
                }
                ui.separator(); let c = CANDIDATES[self.candidate];
                for (label, value) in [("Material", c.material.to_string()), ("Wall thickness", format!("{} mm", c.thickness)), ("Mass", format!("{:.0} kg", c.mass)), ("Geometry", "component.step · referenced asset".into()), ("Cooling time", "30 days · unchanged".into())] {
                    muted(ui, label); ui.label(value);
                }
            });
            card(&mut cols[1], |ui| {
                eyebrow(ui, "CHANGE IMPACT / SIMULATED PLAN");
                ui.label(RichText::new(if self.assessed[self.candidate] { "Recorded evidence can be reused" } else { "Material and geometry affect three methods" }).size(21.0));
                ui.add_space(12.0);
                for (name, state, color) in [("Nuclear reference dataset", "REUSE", GREEN), ("Exposure history", "UNCHANGED", MUTED), ("Transport calculation", if self.assessed[self.candidate] { "REUSE" } else { "RECOMPUTE" }, if self.assessed[self.candidate] { GREEN } else { ORANGE }), ("Activation inventory", if self.assessed[self.candidate] { "REUSE" } else { "RECOMPUTE" }, if self.assessed[self.candidate] { GREEN } else { ORANGE }), ("Shutdown-dose assessment", "REEVALUATE", BLUE)] {
                    ui.horizontal(|ui| { ui.label(name); badge(ui, state, color); });
                }
                ui.add_space(12.0); muted(ui, "Requirements and limits remain fixed. Real change impact would come from declared dependencies and verified reuse rules.");
                ui.add_space(8.0);
                if self.running.is_none() && primary(ui, "Assess this alternative") { self.start_run(false); }
                self.run_progress(ui);
                if self.assessed[self.candidate] && ui.button("View assessment").clicked() { self.tab = Tab::Evidence; }
            });
        });
    }
    fn methods(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "A connected chain of methods",
            "Each method owns its science. Core preserves the connections and the evidence.",
        );
        ui.horizontal_wrapped(|ui| {
            badge(ui, "Materials + geometry", BLUE);
            ui.label("->");
            badge(ui, "Transport", MUTED);
            ui.label("->");
            badge(ui, "ACTINV", MUTED);
            ui.label("->");
            badge(ui, "Dose assessment", MUTED);
        });
        ui.add_space(18.0);
        for (number, name, input, output) in [
            (
                "01",
                "Transport capability",
                "Geometry, compositions, exposure",
                "Spatially resolved flux",
            ),
            (
                "02",
                "ACTINV",
                "Flux, material inventory, nuclear data",
                "Nuclide inventory and decay heat",
            ),
            (
                "03",
                "Dose assessment capability",
                "Inventory, geometry, cooling interval",
                "Shutdown dose with declared uncertainty",
            ),
        ] {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    eyebrow(ui, number);
                    ui.label(RichText::new(name).size(21.0).strong());
                    badge(ui, "VERSION PINNED · DEMO", BLUE);
                });
                ui.horizontal_wrapped(|ui| {
                    muted(ui, format!("Inputs  {input}"));
                    ui.label("->");
                    ui.label(output);
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Inspect applicability").clicked() {
                        self.dialog = Some("method");
                    }
                    if ui.button("Find alternatives").clicked() {
                        self.page = Page::Library;
                        self.category = 2;
                    }
                });
            });
            ui.add_space(12.0);
        }
        muted(
            ui,
            "Example composition only. The preview does not claim these packages form a qualified engineering workflow.",
        );
    }
    fn run_progress(&mut self, ui: &mut egui::Ui) {
        if self.running.is_some() {
            ui.add_space(10.0);
            let phase = if self.progress < 0.25 {
                "Checking inputs and planning reusable work"
            } else if self.progress < 0.55 {
                "Simulating capability execution"
            } else if self.progress < 0.8 {
                "Attaching illustrative evidence"
            } else {
                "Comparing against fixed requirements"
            };
            ui.add(
                egui::ProgressBar::new(self.progress)
                    .text(phase)
                    .animate(true),
            );
            if ui.button("Cancel simulation").clicked() {
                self.running = None;
                self.progress = 0.0;
                self.notice = "Simulation cancelled. No new assessment was recorded.".into();
            }
        }
    }
    fn explore(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "Explore with a clear boundary",
            "Manual exploration and agents use the same designs, requirements, and evidence.",
        );
        egui::CollapsingHeader::new("Investigation settings")
            .id_salt(("exploration-settings", self.completed))
            .default_open(!self.completed)
            .show(ui, |ui| {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                eyebrow(ui, "INVESTIGATION");
                ui.label(RichText::new("Reduce component mass").size(21.0).strong());
            });
            ui.label("Preserve dose ≤ 10 µSv/h and decay heat ≤ 40 W. Compare material and thickness alternatives.");
            ui.add_space(6.0);
            ui.add_enabled_ui(self.running.is_none(), |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.agent, false, "Parameter sweep");
                    ui.selectable_value(&mut self.agent, true, "Agent-assisted");
                    ui.add(egui::Slider::new(&mut self.budget, 3..=48).text("candidate limit"));
                });
            });
            muted(
                ui,
                "Preview runs three scripted candidates within this limit. No model calls, compute spending, or physical predictions.",
            );
            if self.running.is_none()
                && primary(
                    ui,
                    if self.completed {
                        "Run another simulation"
                    } else {
                        "Start exploration"
                    },
                )
            {
                self.start_run(true);
            }
            self.run_progress(ui);
        });
        });
        ui.add_space(18.0);
        if self.completed {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Three alternatives. One fixed question.")
                        .size(21.0)
                        .strong(),
                );
                badge(ui, "SIMULATION COMPLETE", GREEN);
            });
            ui.add_space(10.0);
            card(ui, |ui| {
                egui::Grid::new("candidates")
                    .num_columns(5)
                    .spacing([24.0, 16.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Alternative", "Mass", "Dose", "Result", ""] {
                            muted(ui, h);
                        }
                        ui.end_row();
                        for (i, c) in CANDIDATES.iter().enumerate() {
                            ui.label(c.name);
                            ui.label(format!("{:.0} kg", c.mass));
                            ui.label(format!("{:.1} µSv/h", c.dose));
                            badge(
                                ui,
                                if c.dose <= 10.0 { "3 PASS" } else { "1 FAIL" },
                                if c.dose <= 10.0 { GREEN } else { RED },
                            );
                            if ui
                                .selectable_label(self.candidate == i, "Compare")
                                .clicked()
                            {
                                self.candidate = i;
                            }
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(14.0);
            self.comparison(ui);
        } else {
            card(ui, |ui| {
                eyebrow(ui, "WHAT YOU WILL GET");
                ui.label("Alternatives you can inspect, not just a chat response.");
                muted(
                    ui,
                    "This demonstration keeps the failed candidate visible, shows the mass/dose tradeoff, and lets you decide what to baseline.",
                );
            });
        }
    }
    fn comparison(&mut self, ui: &mut egui::Ui) {
        let c = CANDIDATES[self.candidate];
        let b = CANDIDATES[self.baseline];
        card(ui, |ui| {
            eyebrow(ui, "COMPARED WITH YOUR BASELINE");
            ui.label(
                RichText::new(format!("{} -> {}", b.name, c.name))
                    .size(20.0)
                    .strong(),
            );
            ui.horizontal_wrapped(|ui| {
                badge(ui, &format!("Mass {:+.0} kg", c.mass - b.mass), BLUE);
                if self.assessed[self.candidate] {
                    badge(ui, &format!("Dose {:+.1} µSv/h", c.dose - b.dose), BLUE);
                } else {
                    badge(ui, "Dose not evaluated", MUTED);
                }
                badge(ui, "Requirements unchanged", GREEN);
            });
            muted(
                ui,
                if !self.assessed[self.candidate] {
                    "This proposed revision has no assessment yet. Run an assessment to compare its evidence."
                } else if self.candidate == self.baseline {
                    "This is the current baseline. Choose another revision to inspect a change."
                } else if self.candidate == 1 {
                    "Recorded demo rationale: less material reduces mass, but this example exceeds the dose limit."
                } else {
                    "Recorded demo rationale: retain more thickness while changing the material. These are scripted observations, not a causal claim."
                },
            );
            ui.horizontal(|ui| {
                if ui.button("Inspect supporting evidence").clicked() {
                    self.tab = Tab::Evidence;
                }
                if ui
                    .add_enabled(
                        self.assessed[self.candidate]
                            && c.dose <= 10.0
                            && self.candidate != self.baseline,
                        egui::Button::new("Review as baseline"),
                    )
                    .clicked()
                {
                    self.dialog = Some("baseline");
                }
            });
        });
    }
    fn evidence(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "Follow the result back to its evidence",
            "The assessment, method applicability, and human acceptance remain separate.",
        );
        card(ui, |ui| {
            eyebrow(ui, CANDIDATES[self.candidate].name);
            self.requirements(ui);
        });
        ui.add_space(18.0);
        if !self.assessed[self.candidate] {
            card(ui, |ui| {
                ui.label("This revision has not been assessed.");
                muted(ui, "Earlier evidence belongs to the earlier revision.");
                if self.running.is_none() && primary(ui, "Simulate assessment") {
                    self.start_run(false);
                }
                self.run_progress(ui);
            });
            return;
        }
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                eyebrow(ui, "EVIDENCE TRAIL / DEMO");
                for (a, b) in [("Requirement R-01", "Shutdown dose ≤ 10 µSv/h"), ("Admitted claim", "Nominal illustrative dose value"), ("Producing method", "Dose assessment capability · pinned demo revision"), ("Upstream evidence", "ACTINV inventory -> transport flux"), ("Source inputs", "Material, geometry, exposure, nuclear dataset")] {
                    ui.label(RichText::new(a).strong()); muted(ui, b); ui.add_space(5.0);
                }
                if ui.button("Inspect receipt").clicked() { self.dialog = Some("receipt"); }
            });
            card(&mut cols[1], |ui| {
                eyebrow(ui, "BOUNDARY"); ui.label(RichText::new("What this example establishes").size(20.0));
                ui.label("Only the intended product interaction. The values, evidence trail, and statuses are scripted.");
                ui.add_space(10.0); muted(ui, "In the complete product, this panel would identify numerical uncertainty, qualification, assumptions, and the exact assessment snapshot.");
                ui.add_space(10.0);
                if primary(ui, "Preview evidence handoff") { self.dialog = Some("export"); }
            });
        });
    }
    fn history(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "A history of decisions, not just runs",
            "Keep the original question, alternatives, and their evidence connected.",
        );
        card(ui, |ui| {
            eyebrow(ui, "DESIGN ANCESTRY");
            ui.label("Original design");
            for (i, candidate) in CANDIDATES.iter().enumerate().skip(1) {
                if self.available[i] {
                    ui.horizontal(|ui| {
                        ui.add_space(26.0);
                        ui.label("+");
                        if ui
                            .selectable_label(self.candidate == i, candidate.name)
                            .clicked()
                        {
                            self.candidate = i;
                        }
                        badge(
                            ui,
                            if self.assessed[i] {
                                "ASSESSED"
                            } else {
                                "NOT EVALUATED"
                            },
                            if self.assessed[i] { BLUE } else { MUTED },
                        );
                    });
                }
            }
            ui.add_space(8.0);
            muted(
                ui,
                format!("Current baseline: {}", CANDIDATES[self.baseline].name),
            );
        });
        ui.add_space(14.0);
        self.comparison(ui);
        ui.add_space(14.0);
        card(ui, |ui| {
            eyebrow(ui, "SESSION ACTIVITY");
            for (i, event) in self.events.iter().enumerate().rev() {
                ui.horizontal_wrapped(|ui| {
                    muted(ui, format!("{:02}", i + 1));
                    ui.label(event);
                });
                ui.separator();
            }
        });
    }
    fn compute(&mut self, ui: &mut egui::Ui) {
        eyebrow(ui, "YOUR DATA. YOUR EXECUTION ENVIRONMENT.");
        title(
            ui,
            "Compute",
            "Choose where work runs, independently of where you open the project.",
        );
        for (i, name, desc) in [
            (
                0,
                "This workstation",
                "Local execution · project data remains on this device",
            ),
            (
                1,
                "Organization cluster",
                "Private execution · organization-managed capabilities",
            ),
            (
                2,
                "Managed cloud",
                "Remote execution · explicit transfer and budget controls",
            ),
        ] {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).size(22.0));
                    badge(
                        ui,
                        if self.location == i {
                            "SELECTED IN PREVIEW"
                        } else {
                            "EXAMPLE ENVIRONMENT"
                        },
                        if self.location == i { GREEN } else { MUTED },
                    );
                });
                muted(ui, desc);
                if ui
                    .selectable_label(self.location == i, "Use for simulated work")
                    .clicked()
                {
                    self.location = i;
                    self.notice = format!(
                        "Selected {name}. This is a simulation; no connection or data transfer occurs."
                    );
                }
            });
            ui.add_space(16.0);
        }
    }
    fn organization(&mut self, ui: &mut egui::Ui) {
        title(
            ui,
            "Avila Labs workspace",
            "Shared sources and project context, with explicit ownership.",
        );
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| { eyebrow(ui, "LIBRARY SOURCES"); ui.label("Public library + private organization packages"); muted(ui, "Publisher identity, validation evidence, and organization acceptance are separate records."); if primary(ui, "Browse private examples") { self.source = 2; self.category = 0; self.page = Page::Library; } });
            card(&mut cols[1], |ui| { eyebrow(ui, "PEOPLE AND AGENTS"); ui.label("You · project owner"); ui.label("Exploration assistant · optional"); muted(ui, "Both work against the same requirements. Agent proposals remain attributable and reviewable."); if ui.button("Configure exploration").clicked() { self.page = Page::Project; self.tab = Tab::Explore; } });
        });
        ui.add_space(18.0);
        card(ui, |ui| {
            eyebrow(ui, "PORTABILITY");
            ui.label("A project should remain inspectable outside this workspace.");
            muted(
                ui,
                "The complete product would support organization identities, access controls, private deployment, offline inspection, and evidence retention. This screen is illustrative.",
            );
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.dialog else {
            return;
        };
        let mut open = true;
        egui::Window::new(match kind { "create" => "Create a project", "baseline" => "Review a new baseline", "receipt" => "Illustrative execution receipt", "export" => "Evidence handoff preview", _ => "Method applicability" })
            .collapsible(false).resizable(false).default_width(510.0).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).open(&mut open).show(ctx, |ui| {
                match kind {
                    "create" => {
                        muted(ui, "FROM IRRADIATED COMPONENT ASSESSMENT");
                        ui.label("Project name"); ui.add(egui::TextEdit::singleline(&mut self.draft_title).desired_width(440.0));
                        ui.label("Private workspace · local storage · version-pinned example methods");
                        muted(ui, "The preview replaces the active example with a fresh in-memory project. Nothing is saved or downloaded.");
                        if ui.add_enabled(!self.draft_title.trim().is_empty() && self.running.is_none(), egui::Button::new("Create simulated project").fill(ORANGE).stroke(Stroke::NONE)).clicked() {
                            self.title = self.draft_title.trim().into(); self.candidate = 0; self.baseline = 0; self.assessed = [true, false, false]; self.available = [true, false, false]; self.completed = false;
                            self.events = vec!["Project created from example workflow · original baseline attached".into()];
                            self.page = Page::Project; self.tab = Tab::Overview; self.dialog = None; self.notice = "New simulated project created. Try Design or Explore next.".into();
                        }
                    }
                    "baseline" => {
                        ui.label(RichText::new(CANDIDATES[self.candidate].name).size(21.0));
                        ui.label("Three illustrative requirements pass. The requirements have not changed.");
                        muted(ui, "Acceptance records your decision; it does not qualify a method or change a technical verdict. This action changes only the preview session.");
                        if primary(ui, "Accept as preview baseline") { self.accept(); }
                    }
                    "receipt" => {
                        badge(ui, "EXAMPLE ONLY — NOT A REAL RECEIPT", ORANGE);
                        for (a, b) in [("Method", "Dose assessment capability"), ("Invocation", "demo-invocation-003"), ("Input revision", CANDIDATES[self.candidate].name), ("Execution", "Simulated completed step"), ("Verification", "Not performed in this mockup")] { ui.label(RichText::new(a).strong()); muted(ui, b); }
                    }
                    "export" => {
                        badge(ui, "SIMULATED HANDOFF", ORANGE);
                        ui.label("A portable evidence package would include:");
                        for item in ["Project question and exact requirement versions", "Design revision and input identities", "Method versions, execution receipts, and outputs", "Assessment, limitations, and baseline decision", "An independently usable verification path"] { ui.label(item); }
                        muted(ui, "No file is exported because this preview contains invented evidence.");
                    }
                    _ => {
                        badge(ui, "APPLICABILITY NOT ESTABLISHED", ORANGE);
                        ui.label("This preview contains no qualified method records.");
                        muted(ui, "In a real package, inspect the named method owner, context of use, validation corpus, parameter envelope, uncertainty treatment, and exclusions here.");
                        if ui.button("View library sources").clicked() { self.page = Page::Library; self.dialog = None; }
                    }
                }
                ui.add_space(10.0); if ui.button("Close").clicked() { self.dialog = None; }
            });
        if !open {
            self.dialog = None;
        }
    }
    fn capture(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        self.frames += 1;
        if self.frames == 8 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let captured = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(img) = captured {
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(e) = image::save_buffer(
                &path,
                &bytes,
                img.size[0] as u32,
                img.size[1] as u32,
                image::ExtendedColorType::Rgba8,
            ) {
                eprintln!("capture failed: {e}");
            } else {
                eprintln!("captured {path}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(60));
    }
}
impl Preview {
    fn render(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        if let Some(start) = self.running {
            self.progress = (start.elapsed().as_secs_f32() / 7.0).min(1.0);
            if self.progress >= 1.0 {
                self.finish_run(self.exploring);
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        egui::Panel::top("preview-banner").frame(egui::Frame::new().fill(Color32::from_rgb(41, 32, 26)).inner_margin(egui::Margin::symmetric(20, 9))).show(root, |ui| {
            ui.horizontal_wrapped(|ui| { ui.label(RichText::new("PRODUCT PREVIEW").strong().size(12.0).color(ORANGE)); muted(ui, "Interactive concept · all projects, results, and connections are simulated"); });
        });
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(if self.notice.is_empty() {
                            "Ready. Open a project or browse the library to begin."
                        } else {
                            &self.notice
                        })
                        .size(12.0)
                        .color(MUTED),
                    );
                });
            });
        self.sidebar(root);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(30))
            .show(root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("main-scroll")
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match self.page {
                            Page::Projects => self.home(ui),
                            Page::Library => self.library(ui),
                            Page::Compute => self.compute(ui),
                            Page::Organization => self.organization(ui),
                            Page::Project => self.project(ui),
                        }
                        ui.add_space(20.0);
                    });
            });
        self.dialogs(&ctx);
        self.capture(&ctx);
    }
}

impl eframe::App for Preview {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut Preview, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1200.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        output.textures_delta.clear();
        output
    }

    fn click(app: &mut Preview, ctx: &egui::Context, label: &str) {
        let _ = frame(app, ctx, vec![]);
        let output = frame(app, ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing visible control: {label}"));
        for pressed in [true, false] {
            let _ = frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn clickable_exploration_can_cancel_complete_and_baseline() {
        let ctx = egui::Context::default();
        style(&ctx);
        let mut app = Preview::new();
        click(&mut app, &ctx, "Explore alternatives");
        assert!(app.page == Page::Project && app.tab == Tab::Explore);
        click(&mut app, &ctx, "Start exploration");
        assert!(app.running.is_some());
        click(&mut app, &ctx, "Cancel simulation");
        assert!(app.running.is_none() && !app.completed && !app.assessed[2]);
        click(&mut app, &ctx, "Start exploration");
        app.running = Some(Instant::now() - Duration::from_secs(8));
        let _ = frame(&mut app, &ctx, vec![]);
        assert!(app.completed && app.assessed.iter().all(|v| *v));
        assert_eq!(app.candidate, 2);
        click(&mut app, &ctx, "Review as baseline");
        assert_eq!(app.dialog, Some("baseline"));
        click(&mut app, &ctx, "Accept as preview baseline");
        assert_eq!(app.baseline, 2);
        assert!(app.events.last().unwrap().contains("accepted"));
    }

    #[test]
    fn unassessed_and_failed_candidates_cannot_be_baselined() {
        let mut app = Preview::new();
        app.candidate = 2;
        app.accept();
        assert_eq!(app.baseline, 0);
        app.candidate = 1;
        app.start_run(false);
        // Navigating during a run cannot change which revision gets assessed.
        app.candidate = 2;
        app.finish_run(false);
        assert!(app.assessed[1] && !app.assessed[2]);
        app.accept();
        assert_eq!(app.baseline, 0);
    }
}
