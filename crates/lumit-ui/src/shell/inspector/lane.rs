//! Lane keyframe editing: the interpolation-coded key glyphs, lane
//! selection, the rigid time shift, and the interactive lane drag.

use super::*;

/// Draw a keyframe glyph for each of `keys` on the track portion of `row_rect`,
/// coding interpolation by shape the same way the graph editor does (note 2.3):
/// a square holds, a circle is bezier/eased, a diamond is linear. The linear
/// diamond is the common default and is drawn a touch larger than before so keys
/// read clearly at a glance.
pub(crate) fn draw_key_diamonds(
    ui: &egui::Ui,
    ctx: &RowCtx,
    row_rect: egui::Rect,
    keys: &[lumit_core::anim::Keyframe],
) {
    // In graph mode the lane side belongs to the curve — no diamonds there.
    if ctx.graph_mode {
        return;
    }
    let cy = row_rect.center().y;
    // The same displayed (zoomed, scrolled) axis as the layer bars, so a
    // property's diamonds stay under its layer's keys at any zoom.
    let x_of = |s: f64| ctx.track_left + ((s - ctx.view_start) * ctx.px_per_sec) as f32;
    let fill = ctx.theme.accent;
    let outline = egui::Stroke::new(1.0_f32, ctx.theme.surface_0);
    for k in keys {
        let x = x_of(ctx.off + k.time.to_f64());
        if x >= ctx.track_left - 1.0 && x <= ctx.track_left + ctx.track_w + 1.0 {
            let pos = egui::pos2(x, cy);
            match key_shape(k) {
                KeyShape::Square => {
                    ui.painter().rect(
                        egui::Rect::from_center_size(pos, egui::vec2(6.5, 6.5)),
                        1.0,
                        fill,
                        outline,
                        egui::StrokeKind::Inside,
                    );
                }
                KeyShape::Circle => {
                    ui.painter().circle(pos, 3.6, fill, outline);
                }
                KeyShape::Diamond => {
                    let d = 4.0;
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(x, cy - d),
                            egui::pos2(x + d, cy),
                            egui::pos2(x, cy + d),
                            egui::pos2(x - d, cy),
                        ],
                        fill,
                        outline,
                    ));
                }
            }
        }
    }
}

/// Draw one lane keyframe glyph: the interpolation-coded shape (note 2.3), an
/// accent ring around it when it is in the lane selection, and a brighter fill
/// when it is hot (hovered or being dragged). Shares the shapes with
/// `draw_key_diamonds`.
fn draw_lane_glyph(
    ui: &egui::Ui,
    ctx: &RowCtx,
    pos: egui::Pos2,
    k: &lumit_core::anim::Keyframe,
    selected: bool,
    hot: bool,
) {
    if selected {
        ui.painter()
            .circle_stroke(pos, 6.0, egui::Stroke::new(1.5_f32, ctx.theme.accent));
    }
    let fill = if hot {
        ctx.theme.text_primary
    } else {
        ctx.theme.accent
    };
    let outline = egui::Stroke::new(1.0_f32, ctx.theme.surface_0);
    let (x, cy) = (pos.x, pos.y);
    match key_shape(k) {
        KeyShape::Square => {
            ui.painter().rect(
                egui::Rect::from_center_size(pos, egui::vec2(6.5, 6.5)),
                1.0,
                fill,
                outline,
                egui::StrokeKind::Inside,
            );
        }
        KeyShape::Circle => {
            ui.painter().circle(pos, 3.6, fill, outline);
        }
        KeyShape::Diamond => {
            let d = 4.0;
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(x, cy - d),
                    egui::pos2(x + d, cy),
                    egui::pos2(x, cy + d),
                    egui::pos2(x - d, cy),
                ],
                fill,
                outline,
            ));
        }
    }
}

/// Apply a modifier-aware click to the lane keyframe selection (note 2.6): a
/// plain click replaces it with just this key, Ctrl/Cmd-click toggles this
/// key's membership, and Shift-click adds it — the usual list-select gestures.
pub(crate) fn lane_select_click(
    selection: &mut Vec<crate::app_state::LaneKeySel>,
    sel: crate::app_state::LaneKeySel,
    mods: egui::Modifiers,
) {
    if mods.command || mods.ctrl {
        if let Some(i) = selection.iter().position(|s| *s == sel) {
            selection.remove(i);
        } else {
            selection.push(sel);
        }
    } else if mods.shift {
        if !selection.contains(&sel) {
            selection.push(sel);
        }
    } else {
        selection.clear();
        selection.push(sel);
    }
}

/// Return `keys` with every keyframe whose time matches one of `move_times`
/// (within half a frame at `fps`) shifted by `delta` seconds — clamped to ≥ 0,
/// re-sorted, and de-duplicated by time (a key slid onto another's time keeps
/// the earlier one). The lane keyframe drag commit leans on this, once per
/// affected property, so a group of keys slides rigidly in one undo step.
pub(crate) fn shift_keys_time(
    keys: &[lumit_core::anim::Keyframe],
    move_times: &[lumit_core::Rational],
    delta: f64,
    fps: f64,
) -> Vec<lumit_core::anim::Keyframe> {
    let tol = 0.5 / fps.max(1.0);
    let mut out: Vec<lumit_core::anim::Keyframe> = keys
        .iter()
        .map(|k| {
            let moved = move_times
                .iter()
                .any(|t| (t.to_f64() - k.time.to_f64()).abs() < tol);
            if moved {
                lumit_core::anim::Keyframe {
                    time: rational_at((k.time.to_f64() + delta).max(0.0)),
                    ..*k
                }
            } else {
                *k
            }
        })
        .collect();
    out.sort_by_key(|k| k.time);
    out.dedup_by(|a, b| a.time == b.time);
    out
}

/// Interactive keyframe glyphs on a property row's lane (notes 2.1/2.6). Each
/// key becomes a small draggable target: a plain click selects just it,
/// Shift-click adds it and Ctrl-click toggles it in the lane selection, and
/// dragging a key slides every selected key in *time* (frame-snapped when the
/// magnet is on) — the lane has no value axis, so only time moves; a key's value
/// and tangents are shaped in the graph editor. The grabbed key's delta rides in
/// `app.lane_key_drag` for the live preview and lands in `app.lane_drag_commit`
/// on release, which `timeline_panel` turns into one Batch (a single undo step)
/// after the row loop. Every drawn glyph's screen position is recorded in
/// `app.lane_glyphs` so the timeline's cross-row marquee can hit it. `row` names
/// the property this lane belongs to; a linked Anchor/Position/Scale pair passes
/// its x channel and shows the union of both axes' keys. In graph mode the lane
/// belongs to the curve, so this no-ops (like `draw_key_diamonds`).
pub(crate) fn lane_keys(
    ui: &egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    row_rect: egui::Rect,
    row: crate::app_state::PropRow,
    keys: &[lumit_core::anim::Keyframe],
) {
    if ctx.graph_mode {
        return;
    }
    let layer = ctx.layer.id;
    let cy = row_rect.center().y;
    let x_of = |s: f64| ctx.track_left + ((s - ctx.view_start) * ctx.px_per_sec) as f32;
    let drag_delta = app.lane_key_drag.map(|d| d.delta()).unwrap_or(0.0);
    for (idx, k) in keys.iter().enumerate() {
        let sel = crate::app_state::LaneKeySel {
            layer,
            row,
            time: k.time,
        };
        let selected = app.lane_selection.contains(&sel);
        // A selected key rides the live drag delta; unselected keys stay put.
        let shown_t = k.time.to_f64() + if selected { drag_delta } else { 0.0 };
        let x = x_of(ctx.off + shown_t);
        // Off-screen keys can't be grabbed or marquee'd — skip like the drawer.
        if x < ctx.track_left - 1.0 || x > ctx.track_left + ctx.track_w + 1.0 {
            continue;
        }
        let pos = egui::pos2(x, cy);
        app.lane_glyphs
            .push(crate::app_state::LaneGlyph { sel, pos });
        let resp = ui.interact(
            egui::Rect::from_center_size(pos, egui::vec2(12.0, 14.0)),
            ui.id().with(("lanekey", layer, row, idx)),
            egui::Sense::click_and_drag(),
        );
        let hot = resp.hovered() || app.lane_key_drag.is_some_and(|d| d.grabbed == sel);
        draw_lane_glyph(ui, ctx, pos, k, selected, hot);
        if resp.clicked() {
            let mods = ui.input(|i| i.modifiers);
            lane_select_click(&mut app.lane_selection, sel, mods);
        }
        if resp.drag_started() {
            // Grabbing an unselected key collapses the selection to just it
            // (today's single-key drag, plus select) — the graph editor's rule.
            if !selected {
                app.lane_selection = vec![sel];
            }
            app.lane_key_drag = Some(crate::app_state::LaneKeyDrag {
                grabbed: sel,
                to: k.time.to_f64(),
            });
        }
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                let mut nt = ctx.view_start
                    + (p.x - ctx.track_left) as f64 / ctx.px_per_sec.max(1e-6)
                    - ctx.off;
                // The magnet (note 2.7) snaps the grabbed key to the nearest
                // whole frame — the same maths the graph editor uses.
                if app.magnet_snap {
                    let fps = ctx.fps.max(1.0);
                    nt = (nt * fps).round() / fps;
                }
                nt = nt.max(0.0);
                if let Some(d) = &mut app.lane_key_drag {
                    d.to = nt;
                }
            }
        }
        if resp.drag_stopped() {
            if let Some(d) = app.lane_key_drag.take() {
                app.lane_drag_commit = Some(d.delta());
            }
        }
    }
}
