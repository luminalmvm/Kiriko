//! Transform property rows: the per-property left-column rows, the
//! linked/​combined pair rows, and the row-selection range helpers.

use super::*;

/// Record this property row in the frame's draw order and, when it is clicked,
/// apply the usual list-select gestures to `selected_props` (note 2.6b): plain
/// click picks just this row, Ctrl/Cmd-click toggles it, and Shift-click marks
/// it as the range target (resolved after the whole row loop, since the rows
/// below it aren't drawn yet). Returns whether a *plain* click landed, so the
/// caller can also open the row's curve only on a plain click — a Ctrl/Shift
/// row-select must not re-graph the channel. Drives the highlight; the graph
/// still follows the anchor.
pub(crate) fn prop_row_select(
    app: &mut AppState,
    ui: &egui::Ui,
    row_rect: egui::Rect,
    sel: crate::app_state::PropSel,
) -> bool {
    app.prop_row_order.push(sel);
    if !row_click(ui, row_rect) {
        return false;
    }
    let mods = ui.input(|i| i.modifiers);
    if mods.command || mods.ctrl {
        if let Some(i) = app.selected_props.iter().position(|s| *s == sel) {
            app.selected_props.remove(i);
        } else {
            app.selected_props.push(sel);
        }
        app.selected_prop = Some(sel);
        false
    } else if mods.shift {
        app.prop_range_target = Some(sel);
        false
    } else {
        app.selected_prop = Some(sel);
        app.selected_props = vec![sel];
        true
    }
}

/// The property rows a Shift-click selects (note 2.6b): the inclusive range, in
/// draw order, from the `anchor` to the clicked `target`. When the anchor isn't
/// in `order` (a first selection, or it sat on another layer's rows) fall back
/// to just the target. Returns (the set, whether the target should also become
/// the anchor). Pure, so the range maths is unit-tested.
pub(crate) fn prop_range(
    order: &[crate::app_state::PropSel],
    anchor: Option<crate::app_state::PropSel>,
    target: crate::app_state::PropSel,
) -> (Vec<crate::app_state::PropSel>, bool) {
    let ai = anchor.and_then(|a| order.iter().position(|s| *s == a));
    let ti = order.iter().position(|s| *s == target);
    match (ai, ti) {
        (Some(ai), Some(ti)) => {
            let (lo, hi) = (ai.min(ti), ai.max(ti));
            (order[lo..=hi].to_vec(), false)
        }
        _ => (vec![target], true),
    }
}

/// New (scale_x, scale_y) when the linked Scale control is dragged so x becomes
/// `new_x`, keeping the x:y ratio. A ~zero old x has no defined ratio, so both
/// take the new value (uniform).
pub(crate) fn linked_scale(old_x: f64, old_y: f64, new_x: f64) -> (f64, f64) {
    if old_x.abs() < 1e-9 {
        (new_x, new_x)
    } else {
        (new_x, old_y * new_x / old_x)
    }
}

/// A collapsible sub-group header inside a layer's twirl-down ("Transform",
/// "Effects", …): a disclosure triangle and label, indented under the layer and
/// full width so it reads as a band. Persists and returns its open state. The
/// band is always drawn (a subtle themed strip, brighter on hover) so the
/// section title reads as its own bar.
pub(crate) fn group_header_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    id: egui::Id,
    default_open: bool,
    viewport: egui::Rect,
) -> bool {
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(default_open);
    // The header lives in the outline, but the ui's clip is the lanes and egui
    // hit-tests against rect ∩ clip — so widen the clip or the twirl won't click.
    let (rect, resp) = {
        let saved = ui.clip_rect();
        ui.set_clip_rect(viewport);
        let r =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::click());
        ui.set_clip_rect(saved);
        r
    };
    // A group header sits in the outline column (left of the lanes). set_clip_rect
    // replaces the lane clip; with_clip_rect would intersect it and hide the row.
    let mut p = ui.painter().clone();
    p.set_clip_rect(viewport);
    p.rect_filled(
        rect,
        0.0,
        if resp.hovered() {
            theme.surface_2
        } else {
            theme.surface_1
        },
    );
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let cy = rect.center().y;
    let tx = rect.left() + 22.0;
    let tri = egui::Rect::from_center_size(egui::pos2(tx, cy), egui::vec2(12.0, 12.0));
    crate::icons::disclosure(&p, tri, open, theme.text_muted);
    p.text(
        egui::pos2(tx + 10.0, cy),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        theme.text_secondary,
    );
    open
}

/// The layer's transform properties as full-width timeline rows (K-072): each
/// row shows its stopwatch/name/value in the left column and its own keyframes
/// as diamonds on the track to the right; clicking a row's name graphs it.
/// Scale x/y share one row with a ratio lock (default on); unlocking splits
/// them into two independent rows with a relink control. Anchor and Position
/// x/y are linked by default too, but only as row furniture — one row carries
/// two independent values (AE-style), never coupling them like Scale's ratio.
#[allow(clippy::too_many_arguments)]
pub(crate) fn transform_property_rows(
    ui: &mut egui::Ui,
    theme: &Theme,
    app: &mut AppState,
    comp: &lumit_core::model::Composition,
    comp_id: uuid::Uuid,
    layer: &lumit_core::model::Layer,
    _name_w: f32,
    track_left: f32,
    track_w: f32,
    px_per_sec: f64,
    view_start: f64,
    viewport: egui::Rect,
    pending: &mut Option<lumit_core::Op>,
) {
    use lumit_core::model::{LayerKind, TransformProp};

    let is_camera = matches!(layer.kind, LayerKind::Camera { .. });
    let three_d = layer.switches.three_d || is_camera;
    let fps = comp.frame_rate.fps().max(1.0);
    let ctx = RowCtx {
        theme,
        comp_id,
        comp,
        layer,
        lt: app.preview_frame as f64 / fps - layer.start_offset.0.to_f64(),
        off: layer.start_offset.0.to_f64(),
        fps,
        viewport,
        track_left,
        track_w,
        px_per_sec,
        view_start,
        graph_mode: app.timeline_graph_mode,
        selected_prop: app.selected_prop,
        selected_props: app.selected_props.clone(),
    };

    // Footage speed is a keyframable property too (K-072): its own row above
    // the transform, its keys building the retime's speed lens.
    if let LayerKind::Footage { retime, .. } = &layer.kind {
        speed_property_row(ui, app, &ctx, retime, pending);
    }

    // Anchor and Position: x and y share one row by default, AE-style. Unlike
    // Scale's ratio lock the two values never couple — linking only merges the
    // row furniture (one stopwatch, one navigator, one lane). The chain
    // button splits them into today's separate rows, per layer.
    if !is_camera {
        linked_pair_block(
            ui,
            app,
            &ctx,
            "anchor-unlink",
            "Anchor",
            (TransformProp::AnchorX, TransformProp::AnchorY),
            pending,
        );
    }
    linked_pair_block(
        ui,
        app,
        &ctx,
        "pos-unlink",
        "Position",
        (TransformProp::PositionX, TransformProp::PositionY),
        pending,
    );

    // Scale with a ratio lock (default on). Locked: one row edits both, keeping
    // the ratio. Unlocked: two independent rows plus a relink control.
    let scale_id = ui.id().with(("scale-unlink", layer.id));
    let mut unlinked = ui.data(|d| d.get_temp::<bool>(scale_id)).unwrap_or(false);
    if unlinked {
        prop_row(
            ui,
            app,
            &ctx,
            "Scale x %",
            TransformProp::ScaleX,
            0.5,
            pending,
        );
        prop_row(
            ui,
            app,
            &ctx,
            "Scale y %",
            TransformProp::ScaleY,
            0.5,
            pending,
        );
        if link_toggle_row(
            ui,
            &ctx,
            "Link scale",
            "Re-lock the x:y ratio and edit scale as one value",
        ) {
            unlinked = false;
        }
    } else {
        combined_scale_row(ui, app, &ctx, pending, &mut unlinked);
    }
    ui.data_mut(|d| d.insert_temp(scale_id, unlinked));

    prop_row(
        ui,
        app,
        &ctx,
        "Rotation °",
        TransformProp::Rotation,
        0.5,
        pending,
    );
    prop_row(
        ui,
        app,
        &ctx,
        "Opacity %",
        TransformProp::Opacity,
        0.5,
        pending,
    );
    if three_d {
        prop_row(
            ui,
            app,
            &ctx,
            "Position z",
            TransformProp::PositionZ,
            1.0,
            pending,
        );
        prop_row(
            ui,
            app,
            &ctx,
            "Rotation x °",
            TransformProp::RotationX,
            0.5,
            pending,
        );
        prop_row(
            ui,
            app,
            &ctx,
            "Rotation y °",
            TransformProp::RotationY,
            0.5,
            pending,
        );
    }
}

/// One linked-by-default x/y pair (Anchor, Position): a single two-value row,
/// or — once unlinked via the chain button — two independent rows plus a
/// relink control. The choice is per layer, kept in ui temp data under
/// `(key, layer.id)`, and purely presentational: no document state changes.
pub(crate) fn linked_pair_block(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    key: &'static str,
    label: &str,
    props: (
        lumit_core::model::TransformProp,
        lumit_core::model::TransformProp,
    ),
    pending: &mut Option<lumit_core::Op>,
) {
    let id = ui.id().with((key, ctx.layer.id));
    let mut unlinked = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let lower = label.to_lowercase();
    if unlinked {
        prop_row(ui, app, ctx, &format!("{label} x"), props.0, 1.0, pending);
        prop_row(ui, app, ctx, &format!("{label} y"), props.1, 1.0, pending);
        if link_toggle_row(
            ui,
            ctx,
            &format!("Link {lower}"),
            &format!("Rejoin {lower} x and y on one row (values stay independent)"),
        ) {
            unlinked = false;
        }
    } else {
        linked_pair_row(ui, app, ctx, label, props, pending, &mut unlinked);
    }
    ui.data_mut(|d| d.insert_temp(id, unlinked));
}

/// One generic property row.
pub(crate) fn prop_row(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    label: &str,
    prop: lumit_core::model::TransformProp,
    speed: f64,
    pending: &mut Option<lumit_core::Op>,
) {
    use lumit_core::anim::Animation;
    let slot = ctx.layer.transform.get(prop);
    let is_graphed = app.selected_layer == Some(ctx.layer.id)
        && !app.graph_retime
        && app.graph_prop == Some(prop);
    let sel_row = crate::app_state::PropRow::Transform(prop);
    let (row_rect, mut c) = row_frame(ui, ctx, is_graphed || ctx.is_selected(sel_row));
    prop_row_select(
        app,
        ui,
        row_rect,
        crate::app_state::PropSel {
            layer: ctx.layer.id,
            row: sel_row,
        },
    );

    if let Some(animation) = stopwatch(&mut c, ctx.theme, slot, ctx.lt) {
        *pending = Some(lumit_core::Op::SetTransformProperty {
            comp: ctx.comp_id,
            layer: ctx.layer.id,
            prop,
            animation,
        });
    }
    keyframe_nav(&mut c, app, ctx, prop, slot, pending);
    let name_clicked = c
        .add(
            egui::Label::new(egui::RichText::new(label).small().color(if is_graphed {
                ctx.theme.accent
            } else {
                ctx.theme.text_muted
            }))
            .sense(egui::Sense::click()),
        )
        .clicked();
    // A plain click on the name opens the curve; a Ctrl/Shift-click is a
    // list-select gesture (handled above) and must not re-graph the channel.
    if name_clicked && !ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl) {
        app.selected_layer = Some(ctx.layer.id);
        app.graph_prop = Some(prop);
        app.graph_retime = false; // switching to a transform property
        app.graph_reset_fit(); // a fresh channel starts fitted
    }
    axis_drag_value(&mut c, app, ctx, prop, speed, pending);
    if let Animation::Keyframed(keys) = &slot.animation {
        lane_keys(
            ui,
            app,
            ctx,
            row_rect,
            crate::app_state::PropRow::Transform(prop),
            keys,
        );
    }
}

/// One axis's value box with the shared commit rules (prop_row and the linked
/// Anchor/Position rows both use it): dragging edits live through
/// `app.prop_edit`; on release, a typed value with a marquee multi-selection
/// on this exact channel sets every selected keyframe to that value —
/// absolute, one undo step — while any other commit upserts a key at the
/// playhead (animated) or replaces the static value.
pub(crate) fn axis_drag_value(
    c: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    prop: lumit_core::model::TransformProp,
    speed: f64,
    pending: &mut Option<lumit_core::Op>,
) {
    use lumit_core::anim::Animation;
    let slot = ctx.layer.transform.get(prop);
    let committed = slot.value_at(ctx.lt);
    let mut value = match app.prop_edit {
        Some((l, p, v)) if l == ctx.layer.id && p == prop => v,
        _ => committed,
    };
    let resp = c.add(
        egui::DragValue::new(&mut value)
            .speed(speed)
            .max_decimals(2),
    );
    if resp.dragged() || resp.has_focus() {
        app.prop_edit = Some((ctx.layer.id, prop, value));
    }
    if resp.drag_stopped() || resp.lost_focus() {
        // A typed value (the field was focused, not dragged) with a
        // marquee multi-selection on this exact channel sets every
        // selected keyframe to that value — absolute, one undo step.
        // Dragging the field keeps its usual single-value behaviour.
        let multi_set = if resp.drag_stopped() {
            None
        } else if let Animation::Keyframed(keys) = &slot.animation {
            graph_multi_selection(app, ctx.layer.id, prop, keys).map(|sel| {
                let mut new_keys = keys.clone();
                let changed = set_selected_values(&mut new_keys, &sel, value);
                (new_keys, changed)
            })
        } else {
            None
        };
        if let Some((new_keys, changed)) = multi_set {
            if changed {
                *pending = Some(lumit_core::Op::SetTransformProperty {
                    comp: ctx.comp_id,
                    layer: ctx.layer.id,
                    prop,
                    animation: Animation::Keyframed(new_keys),
                });
            }
        } else if (value - committed).abs() > f64::EPSILON {
            let animation = if slot.is_animated() {
                Animation::Keyframed(upsert_key(slot, ctx.lt, value))
            } else {
                Animation::Static(value)
            };
            *pending = Some(lumit_core::Op::SetTransformProperty {
                comp: ctx.comp_id,
                layer: ctx.layer.id,
                prop,
                animation,
            });
        }
        app.prop_edit = None;
    }
}

/// A Batch op setting two transform properties as one undo step — how every
/// linked two-axis row (Scale, Position, Anchor) commits both axes together.
pub(crate) fn two_prop_batch(
    comp: uuid::Uuid,
    layer: uuid::Uuid,
    x: (
        lumit_core::model::TransformProp,
        lumit_core::anim::Animation,
    ),
    y: (
        lumit_core::model::TransformProp,
        lumit_core::anim::Animation,
    ),
) -> lumit_core::Op {
    lumit_core::Op::Batch {
        ops: vec![
            lumit_core::Op::SetTransformProperty {
                comp,
                layer,
                prop: x.0,
                animation: x.1,
            },
            lumit_core::Op::SetTransformProperty {
                comp,
                layer,
                prop: y.0,
                animation: y.1,
            },
        ],
    }
}

/// The combined "Scale %" row (ratio locked): edits both axes keeping the
/// ratio, with a chain-link button to unlink. Sets `*unlinked` = true when
/// unlinked.
pub(crate) fn combined_scale_row(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    pending: &mut Option<lumit_core::Op>,
    unlinked: &mut bool,
) {
    use lumit_core::anim::Animation;
    use lumit_core::model::TransformProp;
    let sx = ctx.layer.transform.get(TransformProp::ScaleX);
    let sy = ctx.layer.transform.get(TransformProp::ScaleY);
    let is_graphed = app.selected_layer == Some(ctx.layer.id)
        && !app.graph_retime
        && matches!(
            app.graph_prop,
            Some(TransformProp::ScaleX | TransformProp::ScaleY)
        );
    // The linked Scale row selects as its x axis (both move together).
    let sel_row = crate::app_state::PropRow::Transform(TransformProp::ScaleX);
    let (row_rect, mut c) = row_frame(ui, ctx, is_graphed || ctx.is_selected(sel_row));
    prop_row_select(
        app,
        ui,
        row_rect,
        crate::app_state::PropSel {
            layer: ctx.layer.id,
            row: sel_row,
        },
    );

    // Stopwatch drives both axes together (drawn, like every other row).
    let animated = sx.is_animated() || sy.is_animated();
    let hover = if animated {
        "Remove animation"
    } else {
        "Animate both scale axes"
    };
    if stopwatch_button(&mut c, ctx.theme, animated, hover) {
        let (ax, ay) = if animated {
            (
                Animation::Static(sx.value_at(ctx.lt)),
                Animation::Static(sy.value_at(ctx.lt)),
            )
        } else {
            (
                Animation::Keyframed(upsert_key(sx, ctx.lt, sx.value_at(ctx.lt))),
                Animation::Keyframed(upsert_key(sy, ctx.lt, sy.value_at(ctx.lt))),
            )
        };
        *pending = Some(two_prop_batch(
            ctx.comp_id,
            ctx.layer.id,
            (TransformProp::ScaleX, ax),
            (TransformProp::ScaleY, ay),
        ));
    }
    // The ◄ ◆ ► navigator, driving both axes (note-2.5 fix) — shown once the row
    // is animated, matching every other transform row.
    keyframe_nav_scale(&mut c, app, ctx, sx, sy, pending);
    let name_clicked = c
        .add(
            egui::Label::new(egui::RichText::new("Scale %").small().color(if is_graphed {
                ctx.theme.accent
            } else {
                ctx.theme.text_muted
            }))
            .sense(egui::Sense::click()),
        )
        .clicked();
    if name_clicked && !ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl) {
        app.selected_layer = Some(ctx.layer.id);
        app.graph_prop = Some(TransformProp::ScaleX);
        app.graph_retime = false; // switching to a transform property
        app.graph_reset_fit(); // a fresh channel starts fitted
    }
    if icon_button(&mut c, ctx.theme, Icon::Link, true)
        .on_hover_text("Unlink scale (edit x and y separately)")
        .clicked()
    {
        *unlinked = true;
    }
    {
        let old_x = sx.value_at(ctx.lt);
        let old_y = sy.value_at(ctx.lt);
        let mut value = match app.prop_edit {
            Some((l, p, v)) if l == ctx.layer.id && p == TransformProp::ScaleX => v,
            _ => old_x,
        };
        let resp = c.add(egui::DragValue::new(&mut value).speed(0.5).max_decimals(2));
        if resp.dragged() || resp.has_focus() {
            app.prop_edit = Some((ctx.layer.id, TransformProp::ScaleX, value));
            // Both axes move together, so the live preview needs both (else it
            // shows only x scaling until release).
            let (nx, ny) = linked_scale(old_x, old_y, value);
            app.scale_preview = Some((ctx.layer.id, nx, ny));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            if (value - old_x).abs() > f64::EPSILON {
                let (nx, ny) = linked_scale(old_x, old_y, value);
                let ax = if sx.is_animated() {
                    Animation::Keyframed(upsert_key(sx, ctx.lt, nx))
                } else {
                    Animation::Static(nx)
                };
                let ay = if sy.is_animated() {
                    Animation::Keyframed(upsert_key(sy, ctx.lt, ny))
                } else {
                    Animation::Static(ny)
                };
                *pending = Some(two_prop_batch(
                    ctx.comp_id,
                    ctx.layer.id,
                    (TransformProp::ScaleX, ax),
                    (TransformProp::ScaleY, ay),
                ));
            }
            app.prop_edit = None;
            app.scale_preview = None;
        }
    }
    // Lane: the union of both axes' keys, one glyph per time (a linked pair
    // keys both axes together). This is a linked row, so record it — a lane drag
    // on it moves both axes' keys sharing a time (notes 2.1/2.6).
    let mut keys: Vec<lumit_core::anim::Keyframe> = Vec::new();
    for slot in [sx, sy] {
        if let Animation::Keyframed(k) = &slot.animation {
            keys.extend(k.iter().cloned());
        }
    }
    keys.sort_by_key(|k| k.time);
    keys.dedup_by(|a, b| a.time == b.time);
    app.lane_linked.push((ctx.layer.id, TransformProp::ScaleX));
    lane_keys(
        ui,
        app,
        ctx,
        row_rect,
        crate::app_state::PropRow::Transform(TransformProp::ScaleX),
        &keys,
    );
}

/// A thin row holding a relink button ("Link scale", "Link position", …);
/// true when clicked.
pub(crate) fn link_toggle_row(ui: &mut egui::Ui, ctx: &RowCtx, label: &str, hover: &str) -> bool {
    let (_row_rect, mut c) = row_frame(ui, ctx, false);
    let clicked = icon_button(&mut c, ctx.theme, Icon::Link, false)
        .on_hover_text(hover)
        .clicked();
    c.label(
        egui::RichText::new(label)
            .small()
            .color(ctx.theme.text_muted),
    );
    clicked
}

/// One linked Anchor/Position row: two independent value boxes (x then y) on
/// a single row, AE-style. Unlike Scale's ratio lock, nothing couples the
/// values — the link only merges the row furniture: one stopwatch animates or
/// freezes both axes as one undo step, one navigator walks the union of both
/// axes' keys (its diamond keys or clears both at the playhead), the name
/// graphs the x channel, and the lane shows both axes' diamonds. The chain
/// button sets `*unlinked` = true to split into separate rows.
pub(crate) fn linked_pair_row(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    label: &str,
    props: (
        lumit_core::model::TransformProp,
        lumit_core::model::TransformProp,
    ),
    pending: &mut Option<lumit_core::Op>,
    unlinked: &mut bool,
) {
    use lumit_core::anim::Animation;
    let (px, py) = props;
    let sx = ctx.layer.transform.get(px);
    let sy = ctx.layer.transform.get(py);
    let lower = label.to_lowercase();
    let is_graphed = app.selected_layer == Some(ctx.layer.id)
        && !app.graph_retime
        && (app.graph_prop == Some(px) || app.graph_prop == Some(py));
    // The linked pair selects as its x channel (both share the row furniture).
    let sel_row = crate::app_state::PropRow::Transform(px);
    let (row_rect, mut c) = row_frame(ui, ctx, is_graphed || ctx.is_selected(sel_row));
    prop_row_select(
        app,
        ui,
        row_rect,
        crate::app_state::PropSel {
            layer: ctx.layer.id,
            row: sel_row,
        },
    );

    // Stopwatch drives both axes together as one undo step.
    let animated = sx.is_animated() || sy.is_animated();
    let hover = if animated {
        "Remove animation".to_owned()
    } else {
        format!("Animate both {lower} axes")
    };
    if stopwatch_button(&mut c, ctx.theme, animated, &hover) {
        let (ax, ay) = if animated {
            (
                Animation::Static(sx.value_at(ctx.lt)),
                Animation::Static(sy.value_at(ctx.lt)),
            )
        } else {
            (
                Animation::Keyframed(upsert_key(sx, ctx.lt, sx.value_at(ctx.lt))),
                Animation::Keyframed(upsert_key(sy, ctx.lt, sy.value_at(ctx.lt))),
            )
        };
        *pending = Some(two_prop_batch(
            ctx.comp_id,
            ctx.layer.id,
            (px, ax),
            (py, ay),
        ));
    }

    // The keyframe navigator over the union of both axes' keys: the arrows
    // jump to the nearest key on either axis; the diamond keys or clears both
    // axes at the playhead in one undo step.
    let tol = 0.5 / ctx.fps.max(1.0); // within half a frame counts as "on" it
    let times = union_key_times(sx, sy, tol);
    if !times.is_empty() {
        let (prev, on_key, next) = key_nav_targets(&times, ctx.lt, tol);
        let small = |i: Icon| egui::Button::new(crate::icons::text(i, 11.0)).frame(false);
        let mut jump_to: Option<f64> = None;
        if c.add_enabled(prev.is_some(), small(Icon::PrevKeyframe))
            .on_hover_text("Previous keyframe")
            .clicked()
        {
            jump_to = prev;
        }
        if c.add(small(if on_key {
            Icon::Keyframe
        } else {
            Icon::KeyframeAdd
        }))
        .on_hover_text(if on_key {
            "Remove keyframe here (both axes)"
        } else {
            "Add keyframe here (both axes)"
        })
        .clicked()
        {
            *pending = Some(two_prop_batch(
                ctx.comp_id,
                ctx.layer.id,
                (px, toggle_key_at(sx, ctx.lt, tol, on_key)),
                (py, toggle_key_at(sy, ctx.lt, tol, on_key)),
            ));
        }
        if c.add_enabled(next.is_some(), small(Icon::NextKeyframe))
            .on_hover_text("Next keyframe")
            .clicked()
        {
            jump_to = next;
        }
        if let Some(kt) = jump_to {
            app.preview_frame = ((kt + ctx.off) * ctx.fps).round().max(0.0) as usize;
            #[cfg(feature = "media")]
            app.refresh_preview();
        }
    }

    // The name graphs the x channel (like Scale graphs ScaleX) — plain click
    // only; Ctrl/Shift-click is a list-select gesture handled above.
    let name_clicked = c
        .add(
            egui::Label::new(egui::RichText::new(label).small().color(if is_graphed {
                ctx.theme.accent
            } else {
                ctx.theme.text_muted
            }))
            .sense(egui::Sense::click()),
        )
        .clicked();
    if name_clicked && !ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl) {
        app.selected_layer = Some(ctx.layer.id);
        app.graph_prop = Some(px);
        app.graph_retime = false; // switching to a transform property
        app.graph_view_y = None; // re-fit for the newly graphed channel
    }
    if icon_button(&mut c, ctx.theme, Icon::Link, true)
        .on_hover_text(format!("Unlink {lower} (x and y on separate rows)"))
        .clicked()
    {
        *unlinked = true;
    }
    // Two independent value boxes: x then y, each editing only its own axis.
    axis_drag_value(&mut c, app, ctx, px, 1.0, pending);
    axis_drag_value(&mut c, app, ctx, py, 1.0, pending);

    // Lane: the union of both axes' keys, one glyph per time. A linked row —
    // record it so a lane drag moves both axes' keys sharing a time (2.1/2.6).
    let mut keys: Vec<lumit_core::anim::Keyframe> = Vec::new();
    for slot in [sx, sy] {
        if let Animation::Keyframed(k) = &slot.animation {
            keys.extend(k.iter().cloned());
        }
    }
    keys.sort_by_key(|k| k.time);
    keys.dedup_by(|a, b| a.time == b.time);
    app.lane_linked.push((ctx.layer.id, px));
    lane_keys(
        ui,
        app,
        ctx,
        row_rect,
        crate::app_state::PropRow::Transform(px),
        &keys,
    );
}
