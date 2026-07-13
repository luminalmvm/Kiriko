//! The application shell: menu bar, docked panels, status line.
//!
//! Layout per docs/07-UI-SPEC.md (Edit workspace): Project left, Viewer centre,
//! Effect Controls / Effects & Presets right, Timeline across the bottom.

use crate::app_state::AppState;
use crate::splash::{BootLine, Splash};
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
    let [centre, _timeline] = surface.split_below(NodeIndex::root(), 0.65, vec![Panel::Timeline]);
    let [centre, _project] = surface.split_left(centre, 0.22, vec![Panel::Project]);
    let [_centre, _right] = surface.split_right(
        centre,
        0.78,
        vec![
            Panel::EffectControls,
            Panel::EffectsAndPresets,
            Panel::Scopes,
        ],
    );
    state
}

struct PanelViewer<'a> {
    theme: &'a Theme,
    app: &'a mut AppState,
    preview_display: Option<(egui::TextureId, egui::Vec2)>,
}

impl egui_dock::TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        match tab {
            Panel::Viewer => viewer_panel(ui, self.theme, self.app, self.preview_display),
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
#[cfg_attr(not(feature = "media"), allow(unused_variables))]
fn viewer_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut AppState,
    tex: Option<(egui::TextureId, egui::Vec2)>,
) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 0.0, theme.viewer_surround);

    #[cfg(feature = "media")]
    if app.preview_item.is_some() {
        viewer_footage(ui, theme, app, tex, rect);
        return;
    }

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
                    .stroke(egui::Stroke::new(1.0_f32, theme.hairline_strong))
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
                            let b = |t: &str| egui::Button::new(t).min_size(egui::vec2(0.0, 28.0));
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
                            egui::RichText::new("Footage can be dropped anywhere in the window.")
                                .small()
                                .color(theme.text_disabled),
                        );
                    });
            });
        });
    });

    // Viewer bar placeholder (bottom): preview resolution + magnification stubs.
    let bar = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 26.0), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
        egui::Frame::new()
            .fill(theme.surface_1)
            .stroke(egui::Stroke::new(1.0_f32, theme.hairline))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Full")
                            .small()
                            .color(theme.text_secondary),
                    );
                    ui.label(egui::RichText::new("·").small().color(theme.text_disabled));
                    ui.label(
                        egui::RichText::new("Fit")
                            .small()
                            .color(theme.text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("sRGB display")
                                .small()
                                .color(theme.text_muted),
                        );
                    });
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
            match item {
                ProjectItem::Composition(_) => select = Some(item.id()),
                ProjectItem::Footage(_) => {
                    app.preview_item = Some(item.id());
                    app.preview_frame = 0;
                    #[cfg(feature = "media")]
                    {
                        app.refresh_preview();
                        app.request_preview_audio();
                    }
                }
                _ => {}
            }
        }
        #[cfg(feature = "media")]
        if let ProjectItem::Footage(_) = item {
            use crate::app_state::media::MediaStatus;
            ui.indent(item.id(), |ui| match app.media.map.get(&item.id()) {
                Some(MediaStatus::Probing) => {
                    ui.label(
                        egui::RichText::new("probing…")
                            .small()
                            .color(theme.text_disabled),
                    );
                }
                Some(MediaStatus::Ready { probe, frames, vfr }) => {
                    let mut line = String::new();
                    if let Some(v) = &probe.video {
                        line.push_str(&format!(
                            "{}×{} · {:.2} fps · {} frames",
                            v.width,
                            v.height,
                            v.fps(),
                            frames
                        ));
                    } else if let Some(a) = &probe.audio {
                        line.push_str(&format!("{} Hz · {} ch", a.sample_rate, a.channels));
                    }
                    line.push_str(&format!(" · {:.1} s", probe.duration_seconds));
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(line)
                                .monospace()
                                .small()
                                .color(theme.text_muted),
                        );
                        if *vfr {
                            ui.label(
                                egui::RichText::new("VFR")
                                    .monospace()
                                    .small()
                                    .color(theme.warning),
                            )
                            .on_hover_text("Variable frame rate: conformed to the median rate");
                        }
                    });
                }
                Some(MediaStatus::Failed(e)) => {
                    ui.label(
                        egui::RichText::new(format!("unreadable: {e}"))
                            .small()
                            .color(theme.warning),
                    );
                }
                None => {}
            });
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
    use kiriko_core::anim::Animation;
    use kiriko_core::model::TransformProp;
    let comp_id = comp.id;
    let mut pending: Option<kiriko_core::Op> = None;
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
        ui.indent(("transform", layer.id), |ui| {
            ui.collapsing(
                egui::RichText::new("Transform")
                    .small()
                    .color(theme.text_muted),
                |ui| {
                    egui::Grid::new(("txgrid", layer.id))
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 2.0))
                        .show(ui, |ui| {
                            let rows: [(&str, TransformProp, f64); 6] = [
                                ("Position x", TransformProp::PositionX, 1.0),
                                ("Position y", TransformProp::PositionY, 1.0),
                                ("Scale x %", TransformProp::ScaleX, 0.5),
                                ("Scale y %", TransformProp::ScaleY, 0.5),
                                ("Rotation °", TransformProp::Rotation, 0.5),
                                ("Opacity %", TransformProp::Opacity, 0.5),
                            ];
                            for (label, prop, speed) in rows {
                                ui.label(
                                    egui::RichText::new(label).small().color(theme.text_muted),
                                );
                                let slot = layer.transform.get(prop);
                                if slot.is_animated() {
                                    ui.label(
                                        egui::RichText::new("animated")
                                            .monospace()
                                            .small()
                                            .color(theme.accent),
                                    );
                                } else {
                                    let committed = slot.value_at(0.0);
                                    let mut value = match app.prop_edit {
                                        Some((l, p, v)) if l == layer.id && p == prop => v,
                                        _ => committed,
                                    };
                                    let resp = ui.add(
                                        egui::DragValue::new(&mut value)
                                            .speed(speed)
                                            .max_decimals(2),
                                    );
                                    if resp.dragged() || resp.has_focus() {
                                        app.prop_edit = Some((layer.id, prop, value));
                                    }
                                    if resp.drag_stopped() || resp.lost_focus() {
                                        if (value - committed).abs() > f64::EPSILON {
                                            pending = Some(kiriko_core::Op::SetTransformProperty {
                                                comp: comp_id,
                                                layer: layer.id,
                                                prop,
                                                animation: Animation::Static(value),
                                            });
                                        }
                                        app.prop_edit = None;
                                    }
                                }
                                ui.end_row();
                            }
                        });
                },
            );
        });
    }
    if let Some(op) = pending {
        app.commit(op);
    }
}

/// Footage preview: the frame fit to the surround, scrub bar, resolution picker.
#[cfg(feature = "media")]
fn viewer_footage(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut AppState,
    tex: Option<(egui::TextureId, egui::Vec2)>,
    rect: egui::Rect,
) {
    use crate::app_state::media::MediaStatus;
    let Some(id) = app.preview_item else { return };
    let frames = match app.media.map.get(&id) {
        Some(MediaStatus::Ready { frames, .. }) => *frames,
        _ => 0,
    };

    let bar_h = 30.0;
    let image_area = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - bar_h));

    if let Some((id, size)) = tex {
        if size.x > 0.0 && size.y > 0.0 {
            let scale = (image_area.width() / size.x)
                .min(image_area.height() / size.y)
                .min(1.0);
            let draw = egui::Rect::from_center_size(image_area.center(), size * scale);
            ui.painter().image(
                id,
                draw,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    } else {
        ui.painter().text(
            image_area.center(),
            egui::Align2::CENTER_CENTER,
            if frames == 0 {
                "probing…"
            } else {
                "decoding…"
            },
            egui::FontId::monospace(10.0),
            theme.text_disabled,
        );
    }

    // Viewer bar: resolution picker · scrub · frame readout (07-UI-SPEC §2).
    let bar = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - bar_h), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
        egui::Frame::new()
            .fill(theme.surface_1)
            .stroke(egui::Stroke::new(1.0_f32, theme.hairline))
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let labels = ["Full", "Half", "Third", "Quarter"];
                    let current = labels[(app.preview_divisor as usize - 1).min(3)];
                    egui::ComboBox::from_id_salt("preview-res")
                        .selected_text(current)
                        .width(72.0)
                        .show_ui(ui, |ui| {
                            for (i, label) in labels.iter().enumerate() {
                                let div = i as u32 + 1;
                                if ui
                                    .selectable_label(app.preview_divisor == div, *label)
                                    .clicked()
                                {
                                    app.preview_divisor = div;
                                    app.refresh_preview();
                                }
                            }
                        });
                    if frames > 1 {
                        let mut frame = app.preview_frame;
                        let slider = egui::Slider::new(&mut frame, 0..=frames - 1)
                            .show_value(false)
                            .handle_shape(egui::style::HandleShape::Rect { aspect_ratio: 0.4 });
                        let response =
                            ui.add_sized(egui::vec2(ui.available_width() - 96.0, 18.0), slider);
                        if response.changed() {
                            app.preview_frame = frame;
                            app.refresh_preview();
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                app.preview_frame,
                                frames.saturating_sub(1)
                            ))
                            .monospace()
                            .small()
                            .color(theme.text_muted),
                        );
                    });
                });
            });
    });
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

/// GPU display path (slice 5 completion): decoded sRGB bytes → linear fp16
/// working texture → display texture registered with egui. Falls back to the
/// CPU/egui-texture path when no wgpu render state exists.
#[cfg(feature = "media")]
pub struct GpuViewer {
    ctx: kiriko_gpu::GpuContext,
    engine: kiriko_gpu::ColourEngine,
    render_state: egui_wgpu::RenderState,
    /// Keep the display texture alive while egui samples it.
    current: Option<(egui_wgpu::wgpu::Texture, egui::TextureId)>,
}

#[cfg(feature = "media")]
impl GpuViewer {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        let ctx = kiriko_gpu::GpuContext::from_parts(
            render_state.device.clone(),
            render_state.queue.clone(),
        );
        let engine = kiriko_gpu::ColourEngine::new(&ctx);
        Self {
            ctx,
            engine,
            render_state,
            current: None,
        }
    }

    /// Upload a decoded frame through the colour pipeline; returns the egui
    /// texture id + size to paint.
    fn present(&mut self, rgba: &[u8], w: u32, h: u32) -> (egui::TextureId, egui::Vec2) {
        let src = self.engine.upload_srgb8(&self.ctx, rgba, w, h);
        let linear = self.engine.linearise(&self.ctx, &src);
        let shown = self.engine.display(&self.ctx, &linear);
        let view = shown.create_view(&Default::default());
        let id = self.render_state.renderer.write().register_native_texture(
            &self.ctx.device,
            &view,
            egui_wgpu::wgpu::FilterMode::Linear,
        );
        if let Some((_, old)) = self.current.replace((shown, id)) {
            self.render_state.renderer.write().free_texture(&old);
        }
        (id, egui::vec2(w as f32, h as f32))
    }
}

/// Persisted UI state (dock layout only; app state is runtime).
#[derive(Serialize, Deserialize)]
pub struct Shell {
    dock: DockState<Panel>,
    #[serde(skip, default)]
    theme: Theme,
    #[serde(skip, default)]
    app: AppState,
    /// Boot splash (K-008); None once the application window has expanded.
    #[serde(skip, default)]
    splash: Option<Splash>,
    /// Current Viewer frame texture (uploaded on the UI thread from
    /// background-decoded pixels; a memcpy, not a decode — K-017 holds).
    #[serde(skip, default)]
    preview_tex: Option<egui::TextureHandle>,
    /// What the Viewer paints: id + pixel size (GPU path or CPU fallback).
    #[serde(skip, default)]
    preview_display: Option<(egui::TextureId, egui::Vec2)>,
    #[cfg(feature = "media")]
    #[serde(skip, default)]
    gpu: Option<GpuViewer>,
    /// Native macOS menu bar; None on other platforms (07-UI-SPEC).
    #[cfg(target_os = "macos")]
    #[serde(skip, default)]
    native_menu: Option<crate::native_menu::NativeMenu>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            dock: default_layout(),
            theme: Theme::dark(),
            app: AppState::default(),
            splash: None,
            preview_tex: None,
            preview_display: None,
            #[cfg(feature = "media")]
            gpu: None,
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }
}

impl Shell {
    pub fn new(
        ctx: &egui::Context,
        restored: Option<Self>,
        boot_notes: Vec<String>,
        #[cfg(feature = "media")] render_state: Option<egui_wgpu::RenderState>,
    ) -> Self {
        let workspace_restored = restored.is_some();
        let mut shell = restored.unwrap_or_default();
        shell.theme.apply(ctx);
        ctx.style_mut(|s| s.visuals.panel_fill = shell.theme.surface_0);

        // The boot log (K-008): every line reflects real initialisation state.
        let mut lines = vec![
            BootLine::ok("Theme: aizome-dark"),
            BootLine::ok(if workspace_restored {
                "Workspace: restored"
            } else {
                "Workspace: default (Edit)"
            }),
            BootLine::ok("Document store: ready"),
            BootLine::ok("Recovery journal: clean"),
        ];
        lines.extend(boot_notes.into_iter().map(BootLine::ok));
        #[cfg(feature = "media")]
        lines.push(BootLine::ok(format!(
            "Media engine: FFmpeg (libavformat {})",
            kiriko_media::ffmpeg_version()
        )));
        #[cfg(feature = "media")]
        match render_state {
            Some(rs) => {
                shell.gpu = Some(GpuViewer::new(rs));
                lines.push(BootLine::ok(
                    "Colour pipeline: GPU (sRGB → linear fp16 → display)",
                ));
            }
            None => lines.push(BootLine {
                text: "Colour pipeline: CPU fallback (no wgpu render state)".into(),
                failed: true,
            }),
        }
        lines.push(BootLine::ok(
            "Effects: none registered — suite arrives in phase 3",
        ));

        #[cfg(target_os = "macos")]
        {
            match crate::native_menu::NativeMenu::install() {
                Ok(menu) => {
                    shell.native_menu = Some(menu);
                    lines.push(BootLine::ok("Menu bar: native (macOS)"));
                }
                Err(e) => lines.push(BootLine {
                    text: format!("Menu bar: in-window fallback ({e})"),
                    failed: true,
                }),
            }
        }

        shell.splash = Some(Splash::new(lines));
        shell
    }

    #[cfg(target_os = "macos")]
    fn native_menu_frame(&mut self) {
        use crate::native_menu::MenuAction;
        let Some(menu) = &self.native_menu else {
            return;
        };
        for action in menu.poll() {
            match action {
                MenuAction::NewProject => self.app.new_project(),
                MenuAction::OpenProject => self.app.open_dialog(),
                MenuAction::ImportFootage => self.app.import_footage_dialog(),
                MenuAction::Save => self.app.save(),
                MenuAction::Undo => self.app.undo(),
                MenuAction::Redo => self.app.redo(),
                MenuAction::NewComposition => self.app.new_composition(),
                MenuAction::ResetWorkspace => self.dock = default_layout(),
            }
        }
        menu.sync(self.app.store.can_undo(), self.app.store.can_redo());
    }

    #[cfg(not(target_os = "macos"))]
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
        let Some(pending) = &self.app.pending_recovery else {
            return;
        };
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
                    if ui
                        .button(format!(
                            "Restore {n} change{}",
                            if n == 1 { "" } else { "s" }
                        ))
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
        if let Some(splash) = &self.splash {
            if crate::splash::show(ctx, &self.theme, splash) {
                // Boot finished: the splash window becomes the application window.
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1440.0, 900.0)));
                self.splash = None;
            }
            return;
        }
        self.app.autosave_tick();
        #[cfg(feature = "media")]
        {
            self.app.poll_audio();
            // Transport keys (07-UI-SPEC keymap; shuttle speeds arrive with
            // the ring buffer — J/left step back, L plays, K/Space pause).
            if self.app.preview_item.is_some() && !ctx.wants_keyboard_input() {
                let (space, k, l, left, right, j, home) = ctx.input(|i| {
                    (
                        i.key_pressed(egui::Key::Space),
                        i.key_pressed(egui::Key::K),
                        i.key_pressed(egui::Key::L),
                        i.key_pressed(egui::Key::ArrowLeft),
                        i.key_pressed(egui::Key::ArrowRight),
                        i.key_pressed(egui::Key::J),
                        i.key_pressed(egui::Key::Home),
                    )
                });
                if space {
                    self.app.toggle_play();
                }
                if k && self.app.is_playing() {
                    self.app.toggle_play();
                }
                if l && !self.app.is_playing() {
                    self.app.toggle_play();
                }
                let step: i64 = i64::from(right) - i64::from(left || j);
                if step != 0 || home {
                    if self.app.is_playing() {
                        self.app.toggle_play(); // stepping implies pause
                    }
                    let frame = if home {
                        0
                    } else {
                        self.app.preview_frame.saturating_add_signed(step as isize)
                    };
                    self.app.preview_frame = frame;
                    self.app.refresh_preview();
                }
            }
            if self.app.is_playing() {
                if let (Some(clock), Some(fps)) =
                    (self.app.playback_clock(), self.app.preview_fps())
                {
                    let frame = (clock * fps) as usize;
                    if frame != self.app.preview_frame {
                        self.app.preview_frame = frame;
                        self.app.refresh_preview();
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            self.app.media.poll();
            if self.app.media.any_probing() {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
            if self.app.preview_item.is_some() && self.preview_display.is_none() {
                // Selection made before probe finished: retry until Ready.
                self.app.refresh_preview();
            }
            let mut newest = None;
            while let Ok(result) = self.app.preview_engine.results.try_recv() {
                newest = Some(result);
            }
            match newest {
                Some(Ok(px)) if Some(px.item) == self.app.preview_item => {
                    if let Some(gpu) = &mut self.gpu {
                        self.preview_display = Some(gpu.present(&px.rgba, px.width, px.height));
                    } else {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [px.width as usize, px.height as usize],
                            &px.rgba,
                        );
                        let tex =
                            ctx.load_texture("viewer-frame", image, egui::TextureOptions::LINEAR);
                        self.preview_display = Some((tex.id(), tex.size_vec2()));
                        self.preview_tex = Some(tex);
                    }
                }
                Some(Err(e)) => self.app.error = Some(format!("preview: {e}")),
                _ => {}
            }
        }
        #[cfg(target_os = "macos")]
        self.native_menu_frame();
        #[cfg(not(target_os = "macos"))]
        self.shortcuts(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.app.project_title()));

        #[cfg(not(target_os = "macos"))]
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
                        egui::RichText::new("Edit")
                            .small()
                            .color(self.theme.text_muted),
                    )
                    .on_hover_text("Workspace — presets arrive with the panel set");
                });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status = if self.app.dirty {
                    "Unsaved changes"
                } else {
                    "Ready"
                };
                ui.label(
                    egui::RichText::new(status)
                        .small()
                        .color(self.theme.text_muted),
                );
                if let Some(err) = self.app.error.clone() {
                    ui.separator();
                    ui.label(egui::RichText::new(&err).small().color(self.theme.warning));
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

        let Shell {
            dock,
            theme,
            app,
            preview_display,
            ..
        } = self;
        DockArea::new(dock).style(style).show(
            ctx,
            &mut PanelViewer {
                theme,
                app,
                preview_display: *preview_display,
            },
        );
    }
}
