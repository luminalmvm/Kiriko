//! The application shell: menu bar, docked panels, status line.
//!
//! Layout per docs/07-UI-SPEC.md (Edit workspace): Project left, Viewer centre,
//! Effect Controls / Effects & Presets right, Timeline across the bottom.

use crate::app_state::AppState;
use crate::theme::Theme;
use egui_dock::{DockArea, DockState, NodeIndex, Style as DockStyle};
use kiriko_core::model::ProjectItem;
use serde::{Deserialize, Serialize};

/// The dockable panels. Names are glossary names (docs/01-GLOSSARY.md §7).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    Project,
    Viewer,
    Timeline,
    EffectControls,
    EffectsAndPresets,
    Scopes,
}

impl Panel {
    fn title(&self) -> &'static str {
        match self {
            Panel::Project => "Project",
            Panel::Viewer => "Viewer",
            Panel::Timeline => "Timeline",
            Panel::EffectControls => "Effect controls",
            Panel::EffectsAndPresets => "Effects & presets",
            Panel::Scopes => "Scopes",
        }
    }
}

/// Build the default Edit workspace arrangement.
pub fn default_layout() -> DockState<Panel> {
    let mut state = DockState::new(vec![Panel::Viewer]);
    let surface = state.main_surface_mut();
    let [centre, _timeline] =
        surface.split_below(NodeIndex::root(), 0.65, vec![Panel::Timeline]);
    let [centre, _project] = surface.split_left(centre, 0.22, vec![Panel::Project]);
    let [_centre, _right] = surface.split_right(
        centre,
        0.78,
        vec![Panel::EffectControls, Panel::EffectsAndPresets, Panel::Scopes],
    );
    state
}

struct PanelViewer<'a> {
    theme: &'a Theme,
    app: &'a mut AppState,
}

impl egui_dock::TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        match tab {
            Panel::Viewer => viewer_panel(ui, self.theme, self.app),
            Panel::Project => project_panel(ui, self.theme, self.app),
            Panel::Timeline => timeline_panel(ui, self.theme, self.app),
            Panel::EffectControls => empty_hint(
                ui,
                self.theme,
                "No layer selected",
                "Select a layer to see its effect stack.",
            ),
            Panel::EffectsAndPresets => effects_panel(ui, self.theme),
            Panel::Scopes => empty_hint(
                ui,
                self.theme,
                "Scopes",
                "Waveform, vectorscope and histogram arrive with the render pipeline.",
            ),
        }
    }
}

/// The Viewer: neutral surround + the empty-project card (docs/07-UI-SPEC.md §13.2).
fn viewer_panel(ui: &mut egui::Ui, theme: &Theme, app: &mut AppState) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 0.0, theme.viewer_surround);

    let has_content = !app.store.snapshot().items.is_empty();

    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.32);
                if has_content {
                    ui.label(
                        egui::RichText::new("Footage display arrives in slice 5.")
                            .small()
                            .color(theme.text_disabled),
                    );
                    return;
                }
                egui::Frame::group(ui.style())
                    .fill(theme.surface_1)
                    .stroke(egui::Stroke::new(1.0, theme.hairline_strong))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(28, 20))
                    .show(ui, |ui| {
                        ui.set_max_width(340.0);
                        ui.label(
                            egui::RichText::new("Kiriko")
                                .heading()
                                .color(theme.text_primary),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("Start with footage or a composition.")
                                .color(theme.text_muted),
                        );
                        ui.add_space(12.0);
                        ui.vertical_centered_justified(|ui| {
                            let b = |t: &str| {
                                egui::Button::new(t).min_size(egui::vec2(0.0, 28.0))
                            };
                            if ui.add(b("Import footage")).clicked() {
                                app.import_footage_dialog();
                            }
                            if ui.add(b("New composition")).clicked() {
                                app.new_composition();
                            }
                            if ui.add(b("Open project")).clicked() {
                                app.open_dialog();
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Footage can be dropped anywhere in the window.",
                            )
                            .small()
                            .color(theme.text_disabled),
                        );
                    });
            });
        });
    });

    // Viewer bar placeholder (bottom): preview resolution + magnification stubs.
    let bar =
        egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 26.0), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
        egui::Frame::new()
            .fill(theme.surface_1)
            .stroke(egui::Stroke::new(1.0, theme.hairline))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Full").small().color(theme.text_secondary),
                    );
                    ui.label(egui::RichText::new("·").small().color(theme.text_disabled));
                    ui.label(
                        egui::RichText::new("Fit").small().color(theme.text_secondary),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new("sRGB display")
                                    .small()
                                    .color(theme.text_muted),
                            );
                        },
                    );
                });
            });
    });
}

fn project_panel(ui: &mut egui::Ui, theme: &Theme, app: &mut AppState) {
    let doc = app.store.snapshot();
    if doc.items.is_empty() {
        empty_hint(
            ui,
            theme,
            "No footage yet",
            "Drag files anywhere in the window, or use File → Import.",
        );
        return;
    }
    ui.add_space(4.0);
    let mut select = None;
    for item in &doc.items {
        let (kind, colour) = match item {
            ProjectItem::Footage(_) => ("footage", theme.text_muted),
            ProjectItem::Folder(_) => ("folder", theme.text_muted),
            ProjectItem::Composition(_) => ("comp", theme.accent),
        };
        let selected = app.selected_comp == Some(item.id());
        let row = ui.selectable_label(
            selected,
            egui::RichText::new(format!("{}  ", item.name())).color(theme.text_secondary),
        );
        let row = row.on_hover_text(kind);
        ui.painter().text(
            row.rect.right_center() + egui::vec2(-4.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            kind,
            egui::FontId::monospace(10.0),
            colour,
        );
        if row.clicked() {
            if let ProjectItem::Composition(_) = item {
                select = Some(item.id());
            }
        }
    }
    if let Some(id) = select {
        app.selected_comp = Some(id);
    }
}

fn timeline_panel(ui: &mut egui::Ui, theme: &Theme, app: &mut AppState) {
    let doc = app.store.snapshot();
    let comp = app.selected_comp.and_then(|id| doc.comp(id));
    let Some(comp) = comp else {
        empty_hint(
            ui,
            theme,
            "No composition open",
            "Create one with Composition → New, or drop footage here.",
        );
        return;
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&comp.name).color(theme.text_primary));
        ui.label(
            egui::RichText::new(format!(
                "{}×{}  {:.2} fps",
                comp.width,
                comp.height,
                comp.frame_rate.fps()
            ))
            .small()
            .color(theme.text_muted),
        );
    });
    ui.separator();
    if comp.layers.is_empty() {
        ui.label(
            egui::RichText::new("Drag footage here to create the first layer.")
                .small()
                .color(theme.text_muted),
        );
        return;
    }
    for layer in &comp.layers {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&layer.name).color(theme.text_secondary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{:.2}s – {:.2}s",
                        layer.in_point.0.to_f64(),
                        layer.out_point.0.to_f64()
                    ))
                    .monospace()
                    .small()
                    .color(theme.text_muted),
                );
            });
        });
    }
}

fn effects_panel(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(6.0);
    let mut search = String::new();
    ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text("Search effects and presets")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("The effect suite arrives in phase 3.")
            .small()
            .color(theme.text_muted),
    );
}

fn empty_hint(ui: &mut egui::Ui, theme: &Theme, title: &str, hint: &str) {
    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(title).color(theme.text_secondary));
        ui.add_space(2.0);
        ui.label(egui::RichText::new(hint).small().color(theme.text_muted));
    });
}

/// Persisted UI state (dock layout only; app state is runtime).
#[derive(Serialize, Deserialize)]
pub struct Shell {
    dock: DockState<Panel>,
    #[serde(skip, default)]
    theme: Theme,
    #[serde(skip, default)]
    app: AppState,
}

impl Default for Shell {
    fn default() -> Self {
        Self { dock: default_layout(), theme: Theme::dark(), app: AppState::default() }
    }
}

impl Shell {
    pub fn new(ctx: &egui::Context, restored: Option<Self>) -> Self {
        let shell = restored.unwrap_or_default();
        shell.theme.apply(ctx);
        ctx.style_mut(|s| s.visuals.panel_fill = shell.theme.surface_0);
        shell
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
        const REDO: KeyboardShortcut =
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
        const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
        // Order matters: consume the more-modified shortcut first.
        if ctx.input_mut(|i| i.consume_shortcut(&REDO)) {
            self.app.redo();
        } else if ctx.input_mut(|i| i.consume_shortcut(&UNDO)) {
            self.app.undo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SAVE)) {
            self.app.save();
        }
    }

    fn recovery_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.app.pending_recovery else { return };
        let n = pending.ops.len();
        let mut choice: Option<bool> = None;
        egui::Window::new("Recover changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "The last session ended without saving. {n} change{} can be restored.",
                    if n == 1 { "" } else { "s" }
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("Restore {n} change{}", if n == 1 { "" } else { "s" }))
                        .clicked()
                    {
                        choice = Some(true);
                    }
                    if ui.button("Open last save").clicked() {
                        choice = Some(false);
                    }
                });
            });
        if let Some(recover) = choice {
            self.app.resolve_recovery(recover);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.app.autosave_tick();
        self.shortcuts(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.app.project_title()));

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New project").clicked() {
                        self.app.new_project();
                        ui.close_menu();
                    }
                    if ui.button("Open project…").clicked() {
                        self.app.open_dialog();
                        ui.close_menu();
                    }
                    if ui.button("Import footage…").clicked() {
                        self.app.import_footage_dialog();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        self.app.save();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add_enabled(self.app.store.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.app.undo();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(self.app.store.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.app.redo();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Composition", |ui| {
                    if ui.button("New composition").clicked() {
                        self.app.new_composition();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Window", |ui| {
                    if ui.button("Reset workspace").clicked() {
                        self.dock = default_layout();
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Edit").small().color(self.theme.text_muted),
                    )
                    .on_hover_text("Workspace — presets arrive with the panel set");
                });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status = if self.app.dirty { "Unsaved changes" } else { "Ready" };
                ui.label(
                    egui::RichText::new(status).small().color(self.theme.text_muted),
                );
                if let Some(err) = self.app.error.clone() {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(&err).small().color(self.theme.warning),
                    );
                    if ui.small_button("Dismiss").clicked() {
                        self.app.error = None;
                    }
                }
            });
        });

        self.recovery_modal(ctx);

        let mut style = DockStyle::from_egui(&ctx.style());
        style.tab_bar.bg_fill = self.theme.surface_0;
        style.tab_bar.hline_color = self.theme.hairline;

        let Shell { dock, theme, app } = self;
        DockArea::new(dock)
            .style(style)
            .show(ctx, &mut PanelViewer { theme, app });
    }
}
