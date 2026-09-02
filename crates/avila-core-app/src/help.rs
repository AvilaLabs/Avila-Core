//! Guided help: contextual explanations, spotlight walkthroughs for the
//! workbench's use cases, and bundled answers. Everything here is static
//! guidance about the interface; nothing consults a model or a service, and
//! nothing describes a result the runner did not produce.

use eframe::egui;

use crate::{CORE_ORANGE, muted};

const TARGET_COUNT: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TourTarget {
    Brand,
    HelpButton,
    ThemeButton,
    ModeSwitch,
    CaseInput,
    SourceRoots,
    Capabilities,
    Options,
    RunButtons,
    Tabs,
    ReportPanel,
    SpecimenNavigation,
    SpecimenNotice,
    OverviewText,
}

impl TourTarget {
    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TourTargets {
    rects: [Option<egui::Rect>; TARGET_COUNT],
}

impl TourTargets {
    pub(crate) fn set(&mut self, target: TourTarget, rect: egui::Rect) {
        self.rects[target.index()] = Some(rect);
    }

    fn get(&self, target: TourTarget) -> Option<egui::Rect> {
        self.rects[target.index()]
    }
}

/// Where a step wants the interface to be before it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpView {
    Case(HelpTab),
    Specimen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTab {
    Overview,
    Integrity,
    Compile,
    Execute,
    Claims,
    Verdicts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuideKind {
    VerifyFrozenCase,
    PlanAChange,
    ExecuteAfresh,
    ReadAVerdict,
    CompileSpecimen,
}

impl GuideKind {
    pub(crate) const ALL: [Self; 5] = [
        Self::VerifyFrozenCase,
        Self::PlanAChange,
        Self::ExecuteAfresh,
        Self::ReadAVerdict,
        Self::CompileSpecimen,
    ];

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::VerifyFrozenCase => "Verify a frozen case",
            Self::PlanAChange => "See what a change would rerun",
            Self::ExecuteAfresh => "Execute the tools afresh",
            Self::ReadAVerdict => "Read a verdict honestly",
            Self::CompileSpecimen => "Compile a specimen contract",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::VerifyFrozenCase => {
                "Open a case, point it at its roots, and replay everything from committed receipts."
            }
            Self::PlanAChange => {
                "Plan before running: which steps are reused, which rerun, and why."
            }
            Self::ExecuteAfresh => {
                "Supply the executables, turn reuse off, and watch receipts being written and verified."
            }
            Self::ReadAVerdict => {
                "What a requirement verdict says, what its boundary is, and what it never claims."
            }
            Self::CompileSpecimen => {
                "Compile the embedded draft contract and read its findings, owners, and repairs."
            }
        }
    }

    pub(crate) fn by_name(name: &str) -> Option<Self> {
        let wanted = name.to_ascii_lowercase();
        Self::ALL.into_iter().find(|guide| {
            guide
                .title()
                .to_ascii_lowercase()
                .split_whitespace()
                .next()
                .is_some_and(|first| wanted == first)
                || guide.title().eq_ignore_ascii_case(name)
        })
    }

    const fn steps(self) -> &'static [TourStep] {
        match self {
            Self::VerifyFrozenCase => &VERIFY_STEPS,
            Self::PlanAChange => &PLAN_STEPS,
            Self::ExecuteAfresh => &EXECUTE_STEPS,
            Self::ReadAVerdict => &VERDICT_STEPS,
            Self::CompileSpecimen => &SPECIMEN_STEPS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TourStep {
    target: TourTarget,
    view: Option<HelpView>,
    title: &'static str,
    instruction: &'static str,
}

const VERIFY_STEPS: [TourStep; 7] = [
    TourStep {
        target: TourTarget::Brand,
        view: Some(HelpView::Case(HelpTab::Overview)),
        title: "Welcome to the Avila Core workbench",
        instruction: "Core compiles an evidence contract, runs or reuses the tools a case binds, and derives requirement verdicts from admitted evidence. This interface renders the runner's report; it computes nothing itself.",
    },
    TourStep {
        target: TourTarget::CaseInput,
        view: None,
        title: "Open a case",
        instruction: "A case is a directory with a package manifest: hashed documents, bound artifact identities, bound executables, and the steps that execute. Enter its path and press Open.",
    },
    TourStep {
        target: TourTarget::SourceRoots,
        view: None,
        title: "Point the requested roots at local paths",
        instruction: "The package names the roots its artifacts live under. Give each a path on this machine and every artifact is re-hashed against its bound identity. A root left empty is reported as not checked, never assumed.",
    },
    TourStep {
        target: TourTarget::RunButtons,
        view: None,
        title: "Run",
        instruction: "With nothing changed since the committed receipts, both steps are reused: their invocation identities match and their outputs verify, so nothing executes and no executable is needed. Runs happen on a background thread.",
    },
    TourStep {
        target: TourTarget::Tabs,
        view: Some(HelpView::Case(HelpTab::Overview)),
        title: "Six stages, one report",
        instruction: "Integrity, compilation, execution, claims, verdicts, and replay. Each tab shows one stage of the same report the command line prints; the Overview lists all six with their outcome badges.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Verdicts)),
        title: "A technical PASS has a boundary",
        instruction: "CASE-000 returns PASS because both admitted bounded intervals satisfy the two exact requirements. That verdict needs no professional review, and it does not claim qualification, practical suitability, certification, or regulatory approval.",
    },
    TourStep {
        target: TourTarget::HelpButton,
        view: None,
        title: "Help stays here",
        instruction: "Open this button, or press F1, for contextual guidance, bundled answers, and the other walkthroughs. Escape leaves a tour at any time.",
    },
];

const PLAN_STEPS: [TourStep; 5] = [
    TourStep {
        target: TourTarget::Options,
        view: Some(HelpView::Case(HelpTab::Execute)),
        title: "Reuse is the default",
        instruction: "Before running anything, the runner plans each step's invocation identity from the bound inputs, parameters, and executable digest and compares it with the committed receipt. Leave reuse on to see that comparison.",
    },
    TourStep {
        target: TourTarget::RunButtons,
        view: None,
        title: "Plan, do not run",
        instruction: "Plan performs the whole analysis and stops before execution: it reports which steps would be reused and which would rerun, and names every difference by change class.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Execute)),
        title: "Read the change classes",
        instruction: "Input bytes, input binding, parameters, capability, invocation, or a missing or incomplete receipt. A rerun propagates by content: a step whose inputs are byte-identical to what its receipt recorded stays reused even if an earlier step ran again.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Claims)),
        title: "A mismatch is a reviewed change",
        instruction: "When a change reaches the generated claims, they no longer match the committed claims.json and the run is rejected. Re-freezing the expectations is a human decision made after reading the differences, never a mechanical update.",
    },
    TourStep {
        target: TourTarget::HelpButton,
        view: None,
        title: "Try it on this case",
        instruction: "Change one input in a scratch copy of a root, rebind its digest in the package, and Plan again. A rulepack change reaches classification only; a spectrum change reaches activation first.",
    },
];

const EXECUTE_STEPS: [TourStep; 5] = [
    TourStep {
        target: TourTarget::Capabilities,
        view: Some(HelpView::Case(HelpTab::Execute)),
        title: "Executables are bound by digest",
        instruction: "Each capability the package binds is an exact executable identified by the SHA-256 of its bytes. Point each at a local path; a different build is refused before it runs.",
    },
    TourStep {
        target: TourTarget::Options,
        view: None,
        title: "Turn reuse off",
        instruction: "With reuse off every declared step executes afresh in its own workspace: verified inputs are staged at the layout the tool expects, the environment is cleared, and only declared outputs are collected.",
    },
    TourStep {
        target: TourTarget::RunButtons,
        view: None,
        title: "Run",
        instruction: "The ACTINV build takes a few seconds and Aftermatter a few milliseconds. The elapsed time is shown while the background thread works.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Execute)),
        title: "Receipts, verified from bytes",
        instruction: "Each step shows its receipt identity, exit status, staged inputs, and outputs. The runner re-reads the receipt it wrote and re-hashes everything it names; a green REPRODUCES BOUND IDENTITY badge means the fresh bytes equal the frozen ones.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Verdicts)),
        title: "Same verdicts, now from fresh evidence",
        instruction: "The claims were extracted from the outputs the tools just produced, not typed in. The verdicts still name the pending review; a fresh run changes provenance, not authority.",
    },
];

const VERDICT_STEPS: [TourStep; 4] = [
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Verdicts)),
        title: "One verdict per requirement",
        instruction: "PASS, FAIL, INCONCLUSIVE, or NOT EVALUATED, with the exact rule that decided it. A bounded comparison uses the admitted interval, never a rounded display value.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Verdicts)),
        title: "Open the boundary",
        instruction: "Every verdict carries the semantic profile, compiler, evaluator, compiled snapshot, and claims identities it holds under, and whether the review attestation was verified. Outside that boundary the verdict says nothing.",
    },
    TourStep {
        target: TourTarget::ReportPanel,
        view: Some(HelpView::Case(HelpTab::Claims)),
        title: "What the evidence is",
        instruction: "Claims come from executed or reused outputs with the producer's digest, or are carried as recorded attestations when a step did not run. The identity binding shows that every claim is tied to a declared artifact.",
    },
    TourStep {
        target: TourTarget::OverviewText,
        view: Some(HelpView::Case(HelpTab::Overview)),
        title: "What it never claims",
        instruction: "No badge here is scientific truth, certification, or regulatory approval. The notice under the overview states the boundary in words; the text summary is identical to the command line's.",
    },
];

const SPECIMEN_STEPS: [TourStep; 4] = [
    TourStep {
        target: TourTarget::ModeSwitch,
        view: Some(HelpView::Specimen),
        title: "Switch to the specimen compiler",
        instruction: "The second mode compiles an embedded draft contract through the same compiler the case runner uses, without running anything.",
    },
    TourStep {
        target: TourTarget::SpecimenNotice,
        view: Some(HelpView::Specimen),
        title: "An honest draft",
        instruction: "The specimen leaves every parameter and seed undefined on purpose, so it compiles to a rejected draft whose only findings are missing values owned by the requester.",
    },
    TourStep {
        target: TourTarget::SpecimenNavigation,
        view: Some(HelpView::Specimen),
        title: "Findings with owners and repairs",
        instruction: "Open Findings. Every finding carries a stable code, a JSON Pointer location, the party that owns the fix, and typed repair candidates where a bounded repair exists.",
    },
    TourStep {
        target: TourTarget::ModeSwitch,
        view: Some(HelpView::Specimen),
        title: "Back to the workbench",
        instruction: "The case workbench is where compiled contracts meet executed tools. Switch back whenever you want to run or plan a case.",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveTour {
    guide: GuideKind,
    step_index: usize,
}

#[derive(Debug, Default)]
pub(crate) struct GuidedHelp {
    center_open: bool,
    active_tour: Option<ActiveTour>,
    question: String,
    answer_index: Option<usize>,
}

impl GuidedHelp {
    pub(crate) fn toggle_center(&mut self) {
        self.center_open = !self.center_open;
        if self.center_open {
            self.active_tour = None;
        }
    }

    pub(crate) fn start_tour(&mut self, guide: GuideKind) {
        self.active_tour = Some(ActiveTour {
            guide,
            step_index: 0,
        });
        self.center_open = false;
    }

    pub(crate) fn requested_view(&self) -> Option<HelpView> {
        self.active_step().and_then(|step| step.view)
    }

    pub(crate) fn show_center(&mut self, context: &egui::Context, current: HelpView) {
        if !self.center_open || self.active_tour.is_some() {
            return;
        }
        let mut open = true;
        let mut guide_to_start = None;
        egui::Window::new("Help and walkthroughs")
            .id(egui::Id::new("avila-core-help-center"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(440.0)
            .min_width(360.0)
            .max_width(600.0)
            .constrain_to(context.content_rect())
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new("CONTEXTUAL HELP")
                        .small()
                        .strong()
                        .color(CORE_ORANGE),
                );
                let (title, body) = view_help(current);
                ui.heading(title);
                ui.label(body);

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Walk me through it");
                for guide in GuideKind::ALL {
                    let response = ui.add(
                        egui::Button::new(egui::RichText::new(guide.title()).strong())
                            .min_size(egui::vec2(ui.available_width(), 34.0)),
                    );
                    if response.clicked() {
                        guide_to_start = Some(guide);
                    }
                    ui.small(guide.description());
                    ui.add_space(5.0);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.heading("Ask bundled help");
                ui.small(
                    "Answers come from reviewed guidance bundled with the application. No model or service is contacted.",
                );
                let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.question)
                        .hint_text("Why is the verdict NOT EVALUATED?")
                        .desired_width(f32::INFINITY),
                );
                if response.changed() || (response.has_focus() && enter_pressed) {
                    self.answer_index = best_answer(&self.question);
                }
                if self.question.trim().is_empty() {
                    ui.small("Common questions:");
                    for (index, entry) in FAQ.iter().enumerate().take(5) {
                        if ui.link(entry.question).clicked() {
                            self.question = entry.question.to_owned();
                            self.answer_index = Some(index);
                        }
                    }
                } else if let Some(index) = self.answer_index {
                    let entry = FAQ[index];
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong(entry.question);
                        ui.label(entry.answer);
                    });
                } else {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.strong("No bundled answer matched that question.");
                        ui.label(
                            "Try asking about verdicts, reuse, receipts, refused runs, mismatches, roots, or the interface itself.",
                        );
                    });
                }
            });
        self.center_open = open;
        if let Some(guide) = guide_to_start {
            self.start_tour(guide);
        }
    }

    pub(crate) fn show_tour(&mut self, context: &egui::Context, targets: &TourTargets) {
        let Some(active) = self.active_tour else {
            return;
        };
        let steps = active.guide.steps();
        let step = steps[active.step_index];
        if context.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.active_tour = None;
            return;
        }

        let dark = context.theme() == egui::Theme::Dark;
        let screen = context.content_rect();
        let fallback = egui::Rect::from_center_size(
            screen.center(),
            egui::vec2(screen.width().min(520.0), screen.height().min(260.0)),
        );
        let spotlight = targets
            .get(step.target)
            .unwrap_or(fallback)
            .expand(8.0)
            .intersect(screen);

        let painter = context.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("avila-core-tour-dimmer"),
        ));
        let dim_color = if dark {
            egui::Color32::from_black_alpha(205)
        } else {
            egui::Color32::from_black_alpha(130)
        };
        for rect in dim_rectangles(screen, spotlight) {
            if rect.is_positive() {
                painter.rect_filled(rect, 0.0, dim_color);
            }
        }
        painter.rect_stroke(
            spotlight,
            9.0,
            egui::Stroke::new(3.0, CORE_ORANGE),
            egui::StrokeKind::Outside,
        );

        let callout_position = callout_position(screen, spotlight);
        let mut go_back = false;
        let mut go_next = false;
        let mut close = false;
        egui::Area::new(egui::Id::new("avila-core-tour-callout"))
            .order(egui::Order::Tooltip)
            .fixed_pos(callout_position)
            .constrain_to(screen)
            .show(context, |ui| {
                egui::Frame::new()
                    .fill(if dark {
                        egui::Color32::from_rgb(28, 28, 28)
                    } else {
                        egui::Color32::WHITE
                    })
                    .stroke(egui::Stroke::new(1.0, CORE_ORANGE))
                    .corner_radius(10)
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 2,
                        color: egui::Color32::from_black_alpha(150),
                    })
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(active.guide.title().to_uppercase())
                                    .small()
                                    .strong()
                                    .color(CORE_ORANGE),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    close = ui
                                        .button(egui::RichText::new("×").size(18.0))
                                        .on_hover_text("End tour")
                                        .clicked();
                                },
                            );
                        });
                        ui.add(
                            egui::ProgressBar::new(
                                (active.step_index + 1) as f32 / steps.len() as f32,
                            )
                            .desired_width(ui.available_width()),
                        );
                        ui.heading(step.title);
                        ui.label(step.instruction);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "The highlighted controls stay live; use them now if useful, then continue.",
                            )
                            .small()
                            .color(muted(ui)),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            go_back = ui
                                .add_enabled(active.step_index > 0, egui::Button::new("Back"))
                                .clicked();
                            ui.label(format!(
                                "Step {} of {}",
                                active.step_index + 1,
                                steps.len()
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    go_next = ui
                                        .button(if active.step_index + 1 == steps.len() {
                                            "Finish"
                                        } else {
                                            "Next"
                                        })
                                        .clicked();
                                },
                            );
                        });
                    });
            });

        go_back |= context.input(|input| input.key_pressed(egui::Key::ArrowLeft));
        go_next |= context.input(|input| input.key_pressed(egui::Key::ArrowRight));
        if close {
            self.active_tour = None;
        } else if go_back && active.step_index > 0 {
            self.active_tour = Some(ActiveTour {
                step_index: active.step_index - 1,
                ..active
            });
        } else if go_next {
            if active.step_index + 1 == steps.len() {
                self.active_tour = None;
            } else {
                self.active_tour = Some(ActiveTour {
                    step_index: active.step_index + 1,
                    ..active
                });
            }
        }
    }

    fn active_step(&self) -> Option<TourStep> {
        let active = self.active_tour?;
        active.guide.steps().get(active.step_index).copied()
    }
}

fn view_help(view: HelpView) -> (&'static str, &'static str) {
    match view {
        HelpView::Case(HelpTab::Overview) => (
            "Overview",
            "The six stages of one run with their outcome badges. Every badge is read from the runner's report; the notice below the card states what no badge ever claims.",
        ),
        HelpView::Case(HelpTab::Integrity) => (
            "Integrity",
            "Package documents and bound artifacts re-hashed against their declared identities. Not checked means the root was not supplied; mismatch or missing under a supplied root stops the run.",
        ),
        HelpView::Case(HelpTab::Compile) => (
            "Compile",
            "The contract compiled against its registry snapshot: steps in dependency order, requirements, and the snapshot identity every claim must name. A rejected compilation lists findings with owners and repairs.",
        ),
        HelpView::Case(HelpTab::Execute) => (
            "Execute",
            "Each declared step is reused from its committed receipt, executed afresh, planned, or not run. Changes since the committed receipt are named by class; receipts are verified from bytes.",
        ),
        HelpView::Case(HelpTab::Claims) => (
            "Claims",
            "The generated claims document: attestations from package identities and claims extracted from executed or reused outputs, compared canonically with the committed claims.json and bound to the package.",
        ),
        HelpView::Case(HelpTab::Verdicts) => (
            "Verdicts",
            "One four-state technical verdict per requirement with the rule that decided it, the evidence it used, and the complete boundary it holds under. Optional agent practicality routing is shown separately and cannot alter the verdict.",
        ),
        HelpView::Specimen => (
            "Specimen compiler",
            "Compiles the embedded draft contract and renders its findings. The specimen is deliberately incomplete; it demonstrates the diagnostic contract, not a result.",
        ),
    }
}

#[derive(Debug, Clone, Copy)]
struct FaqEntry {
    question: &'static str,
    keywords: &'static [&'static str],
    answer: &'static str,
}

const FAQ: [FaqEntry; 8] = [
    FaqEntry {
        question: "Why is the verdict NOT EVALUATED?",
        keywords: &[
            "not evaluated",
            "verdict",
            "qualification",
            "evidence",
            "applicability",
        ],
        answer: "Core could not apply the requirement rule to admitted evidence. Inspect the named reason and boundary: required evidence may be missing or inadmissible, a qualification term may be false, or an upstream result may be unavailable. Review and presentation state never suppress a technical PASS or FAIL.",
    },
    FaqEntry {
        question: "What does REUSED mean?",
        keywords: &["reused", "reuse", "memo", "cache", "skip"],
        answer: "The runner planned the step's invocation identity from the current bound inputs, parameters, and executable digest; it equals the committed receipt's, the receipt completed with exit status zero, and every recorded output still verifies at a bound artifact identity. Under a deterministic declaration that is exactly what a rerun would produce, so nothing runs. Turn reuse off to execute afresh.",
    },
    FaqEntry {
        question: "What does a receipt prove?",
        keywords: &["receipt", "prove", "provenance", "exit"],
        answer: "That a named executable, identified by digest, ran over named bytes with a recorded invocation and produced named bytes. Receipts are re-read from disk and every file they name is re-hashed. A receipt does not prove scientific correctness, qualification, review, or regulatory suitability.",
    },
    FaqEntry {
        question: "Why was execution refused?",
        keywords: &[
            "refused",
            "refuse",
            "digest",
            "mismatch",
            "executable",
            "unchecked",
        ],
        answer: "The runner refuses to execute over bytes it has not verified, with an executable whose digest differs from the bound identity, through an adapter whose capability type differs from the compiled step, or for a review step. Supply the missing root, or the exact build the package binds.",
    },
    FaqEntry {
        question: "Why does the run say MISMATCH?",
        keywords: &[
            "mismatch",
            "differ",
            "drift",
            "committed",
            "expectation",
            "bless",
        ],
        answer: "The generated claims or campaign report differ from the committed expectations, or a fresh receipt drifts from the committed one. Something changed: an input, an executable, a parameter, or the contract. Read the change classes on the Execute tab; re-freezing the expectations is a reviewed decision.",
    },
    FaqEntry {
        question: "What are source roots and capabilities?",
        keywords: &[
            "root",
            "roots",
            "capability",
            "capabilities",
            "path",
            "supply",
        ],
        answer: "Roots are named directories under which the package's artifacts are found and re-hashed; capabilities are named executables the package binds by digest. Both are named by the package and supplied by you. An omitted root is not checked; an omitted capability leaves its step not run or reused.",
    },
    FaqEntry {
        question: "Does the interface compute anything?",
        keywords: &["interface", "compute", "calculate", "gui", "app", "ui"],
        answer: "No. Every badge, number, and state is read from the run report the runner crate produces, the same report the command line prints. The interface can request a plan or a run and render the outcome; it has no semantics of its own.",
    },
    FaqEntry {
        question: "How do I switch to light mode?",
        keywords: &["light", "dark", "theme", "mode", "color"],
        answer: "Use the theme button in the header. The choice affects only the rendering; it is not remembered between launches yet.",
    },
];

fn best_answer(question: &str) -> Option<usize> {
    let lowered = question.to_ascii_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for (index, entry) in FAQ.iter().enumerate() {
        let score = entry
            .keywords
            .iter()
            .filter(|keyword| lowered.contains(*keyword))
            .count();
        if score > 0 && best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((index, score));
        }
    }
    best.map(|(index, _)| index)
}

fn dim_rectangles(screen: egui::Rect, spotlight: egui::Rect) -> [egui::Rect; 4] {
    [
        egui::Rect::from_min_max(screen.min, egui::pos2(screen.max.x, spotlight.min.y)),
        egui::Rect::from_min_max(egui::pos2(screen.min.x, spotlight.max.y), screen.max),
        egui::Rect::from_min_max(
            egui::pos2(screen.min.x, spotlight.min.y),
            egui::pos2(spotlight.min.x, spotlight.max.y),
        ),
        egui::Rect::from_min_max(
            egui::pos2(spotlight.max.x, spotlight.min.y),
            egui::pos2(screen.max.x, spotlight.max.y),
        ),
    ]
}

fn callout_position(screen: egui::Rect, spotlight: egui::Rect) -> egui::Pos2 {
    let width = 392.0;
    let height = 300.0;
    let margin = 16.0;
    let x = if spotlight.max.x + margin + width <= screen.max.x {
        spotlight.max.x + margin
    } else if spotlight.min.x - margin - width >= screen.min.x {
        spotlight.min.x - margin - width
    } else {
        (screen.max.x - width - margin).max(screen.min.x + margin)
    };
    let y = if spotlight.min.y + height <= screen.max.y {
        spotlight.min.y
    } else if spotlight.max.y - height >= screen.min.y {
        spotlight.max.y - height
    } else {
        (screen.max.y - height - margin).max(screen.min.y + margin)
    };
    egui::pos2(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guide_has_steps_and_answers_match_keywords() {
        for guide in GuideKind::ALL {
            assert!(!guide.steps().is_empty(), "{}", guide.title());
        }
        assert_eq!(best_answer("why is it NOT EVALUATED"), Some(0));
        assert_eq!(best_answer("what is reused"), Some(1));
        assert_eq!(best_answer("light theme"), Some(7));
        assert_eq!(best_answer("zzz"), None);
        assert_eq!(
            GuideKind::by_name("verify"),
            Some(GuideKind::VerifyFrozenCase)
        );
        assert_eq!(
            GuideKind::by_name("Execute the tools afresh"),
            Some(GuideKind::ExecuteAfresh)
        );
    }

    #[test]
    fn dimming_covers_everything_but_the_spotlight() {
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 100.0));
        let spotlight = egui::Rect::from_min_max(egui::pos2(20.0, 30.0), egui::pos2(60.0, 50.0));
        let total: f32 = dim_rectangles(screen, spotlight)
            .iter()
            .map(|rect| rect.area())
            .sum();
        assert!((total - (screen.area() - spotlight.area())).abs() < 1e-3);
        let position = callout_position(screen, spotlight);
        assert!(screen.contains(position));
    }
}
