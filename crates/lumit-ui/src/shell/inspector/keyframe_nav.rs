//! Keyframe navigators: the stopwatch toggle, the previous/next-key
//! navigators, and the small key-time helper functions they share.

use super::*;

/// Every keyframe time (layer-local seconds) across a layer's animated
/// properties — for the timeline's keyframe glyphs.
pub(crate) fn layer_keyframe_times(layer: &lumit_core::model::Layer) -> Vec<f64> {
    use lumit_core::anim::Animation;
    use lumit_core::model::{LayerKind, TransformProp};
    let mut times = Vec::new();
    let mut collect = |anim: &Animation| {
        if let Animation::Keyframed(keys) = anim {
            times.extend(keys.iter().map(|k| k.time.to_f64()));
        }
    };
    for prop in [
        TransformProp::AnchorX,
        TransformProp::AnchorY,
        TransformProp::PositionX,
        TransformProp::PositionY,
        TransformProp::PositionZ,
        TransformProp::ScaleX,
        TransformProp::ScaleY,
        TransformProp::Rotation,
        TransformProp::RotationX,
        TransformProp::RotationY,
        TransformProp::Opacity,
    ] {
        collect(&layer.transform.get(prop).animation);
    }
    if let LayerKind::Camera { zoom } = &layer.kind {
        collect(&zoom.animation);
    }
    times
}

/// The stopwatch toggle. Returns the new Animation if clicked (animate at the
/// playhead / freeze to the current value), else None.
/// A drawn, clickable stopwatch — a filled dot when animated, a ring when not.
/// Replaces the old `⏱`/`◦` glyph (egui's fonts can't render the emoji, so it
/// vanished), and clips like any child-ui widget. Returns true on click.
pub(crate) fn stopwatch_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    animated: bool,
    hover: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
    let color = if resp.hovered() {
        theme.text_primary
    } else if animated {
        theme.accent
    } else {
        theme.text_muted
    };
    crate::icons::stopwatch(ui.painter(), rect.center(), 4.5, animated, color);
    resp.on_hover_text(hover).clicked()
}

pub(crate) fn stopwatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    slot: &lumit_core::anim::Property,
    lt: f64,
) -> Option<lumit_core::anim::Animation> {
    use lumit_core::anim::{Animation, Keyframe, SideInterp};
    let animated = slot.is_animated();
    let hover = if animated {
        "Remove animation (freeze current value)"
    } else {
        "Animate: keyframe at the playhead"
    };
    if stopwatch_button(ui, theme, animated, hover) {
        Some(if animated {
            Animation::Static(slot.value_at(lt))
        } else {
            Animation::Keyframed(vec![Keyframe {
                time: rational_at(lt),
                value: slot.value_at(lt),
                interp_in: SideInterp::Linear,
                interp_out: SideInterp::Linear,
            }])
        })
    } else {
        None
    }
}

/// AE-style keyframe navigator for an animated property, shown next to the
/// stopwatch: ◄ jumps the playhead to the previous keyframe, the diamond adds a
/// keyframe at the playhead (filled ◆ when one is already there — clicking then
/// removes it), ► jumps to the next keyframe.
pub(crate) fn keyframe_nav(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    prop: lumit_core::model::TransformProp,
    slot: &lumit_core::anim::Property,
    pending: &mut Option<lumit_core::Op>,
) {
    use lumit_core::anim::Animation;
    let Animation::Keyframed(keys) = &slot.animation else {
        return;
    };
    let tol = 0.5 / ctx.fps.max(1.0); // within half a frame counts as "on" it
                                      // Iconoir glyphs (K-085): the old ◄ ◆ ► characters aren't in the UI fonts
                                      // and rendered as blanks. No colour is set, so disabled buttons dim.
    let small = |i: Icon| egui::Button::new(crate::icons::text(i, 11.0)).frame(false);
    let mut jump_to: Option<f64> = None;

    let has_prev = keys.iter().any(|k| k.time.to_f64() < ctx.lt - tol);
    if ui
        .add_enabled(has_prev, small(Icon::PrevKeyframe))
        .on_hover_text("Previous keyframe")
        .clicked()
    {
        jump_to = keys
            .iter()
            .rev()
            .find(|k| k.time.to_f64() < ctx.lt - tol)
            .map(|k| k.time.to_f64());
    }

    let on_key = keys.iter().any(|k| (k.time.to_f64() - ctx.lt).abs() < tol);
    if ui
        .add(small(if on_key {
            Icon::KeyframeFilled
        } else {
            Icon::Keyframe
        }))
        .on_hover_text(if on_key {
            "Remove keyframe here"
        } else {
            "Add keyframe here"
        })
        .clicked()
    {
        let animation = if on_key {
            let kept: Vec<_> = keys
                .iter()
                .filter(|k| (k.time.to_f64() - ctx.lt).abs() >= tol)
                .cloned()
                .collect();
            if kept.is_empty() {
                Animation::Static(slot.value_at(ctx.lt))
            } else {
                Animation::Keyframed(kept)
            }
        } else {
            Animation::Keyframed(upsert_key(slot, ctx.lt, slot.value_at(ctx.lt)))
        };
        *pending = Some(lumit_core::Op::SetTransformProperty {
            comp: ctx.comp_id,
            layer: ctx.layer.id,
            prop,
            animation,
        });
    }

    let has_next = keys.iter().any(|k| k.time.to_f64() > ctx.lt + tol);
    if ui
        .add_enabled(has_next, small(Icon::NextKeyframe))
        .on_hover_text("Next keyframe")
        .clicked()
    {
        jump_to = keys
            .iter()
            .find(|k| k.time.to_f64() > ctx.lt + tol)
            .map(|k| k.time.to_f64());
    }

    if let Some(kt) = jump_to {
        app.preview_frame = ((kt + ctx.off) * ctx.fps).round().max(0.0) as usize;
        #[cfg(feature = "media")]
        app.refresh_preview();
    }
}

/// The keyframe navigator for the linked Scale row — the scale twin of
/// [`keyframe_nav`], which drives a single property. Prev/next jump across the
/// *union* of both axes' key times, and the diamond adds or removes a keyframe
/// on **both** axes at once (one `two_prop_batch`), so the linked pair keeps
/// matching keys. Without this the animated Scale row showed a stopwatch but no
/// ◄ ◆ ► navigator, unlike every other transform row (the note-2.5 bug).
pub(crate) fn keyframe_nav_scale(
    ui: &mut egui::Ui,
    app: &mut AppState,
    ctx: &RowCtx,
    sx: &lumit_core::anim::Property,
    sy: &lumit_core::anim::Property,
    pending: &mut Option<lumit_core::Op>,
) {
    use lumit_core::anim::Animation;
    use lumit_core::model::TransformProp;
    if !(sx.is_animated() || sy.is_animated()) {
        return;
    }
    let tol = 0.5 / ctx.fps.max(1.0); // within half a frame counts as "on" it
    let small = |i: Icon| egui::Button::new(crate::icons::text(i, 11.0)).frame(false);
    // The union of both axes' key times, ascending — a linked pair usually holds
    // matching keys, but a just-unlinked-then-relinked pair might not.
    let mut times: Vec<f64> = Vec::new();
    for slot in [sx, sy] {
        if let Animation::Keyframed(k) = &slot.animation {
            times.extend(k.iter().map(|kf| kf.time.to_f64()));
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut jump_to: Option<f64> = None;

    let has_prev = times.iter().any(|&t| t < ctx.lt - tol);
    if ui
        .add_enabled(has_prev, small(Icon::PrevKeyframe))
        .on_hover_text("Previous keyframe")
        .clicked()
    {
        jump_to = times.iter().rev().find(|&&t| t < ctx.lt - tol).copied();
    }

    let on_key = times.iter().any(|&t| (t - ctx.lt).abs() < tol);
    if ui
        .add(small(if on_key {
            Icon::KeyframeFilled
        } else {
            Icon::Keyframe
        }))
        .on_hover_text(if on_key {
            "Remove keyframe here"
        } else {
            "Add keyframe here"
        })
        .clicked()
    {
        // Add on both axes, or remove the key at the playhead from both, so the
        // linked pair stays in step (the stopwatch drives them together too).
        let axis = |slot: &lumit_core::anim::Property| -> Animation {
            if on_key {
                if let Animation::Keyframed(k) = &slot.animation {
                    let kept: Vec<_> = k
                        .iter()
                        .filter(|kf| (kf.time.to_f64() - ctx.lt).abs() >= tol)
                        .cloned()
                        .collect();
                    if kept.is_empty() {
                        Animation::Static(slot.value_at(ctx.lt))
                    } else {
                        Animation::Keyframed(kept)
                    }
                } else {
                    slot.animation.clone()
                }
            } else {
                Animation::Keyframed(upsert_key(slot, ctx.lt, slot.value_at(ctx.lt)))
            }
        };
        *pending = Some(two_prop_batch(
            ctx.comp_id,
            ctx.layer.id,
            (TransformProp::ScaleX, axis(sx)),
            (TransformProp::ScaleY, axis(sy)),
        ));
    }

    let has_next = times.iter().any(|&t| t > ctx.lt + tol);
    if ui
        .add_enabled(has_next, small(Icon::NextKeyframe))
        .on_hover_text("Next keyframe")
        .clicked()
    {
        jump_to = times.iter().find(|&&t| t > ctx.lt + tol).copied();
    }

    if let Some(kt) = jump_to {
        app.preview_frame = ((kt + ctx.off) * ctx.fps).round().max(0.0) as usize;
        #[cfg(feature = "media")]
        app.refresh_preview();
    }
}

/// Sorted key times (seconds, layer-local) across both axes of a linked row,
/// de-duplicated within `tol` — the navigator and its diamond work on this
/// union, so a key on either axis counts.
pub(crate) fn union_key_times(
    a: &lumit_core::anim::Property,
    b: &lumit_core::anim::Property,
    tol: f64,
) -> Vec<f64> {
    use lumit_core::anim::Animation;
    let mut times: Vec<f64> = Vec::new();
    for slot in [a, b] {
        if let Animation::Keyframed(keys) = &slot.animation {
            times.extend(keys.iter().map(|k| k.time.to_f64()));
        }
    }
    times.sort_by(f64::total_cmp);
    times.dedup_by(|p, q| (*p - *q).abs() < tol);
    times
}

/// Where a navigator can go from local time `lt` over sorted key `times`:
/// (previous key time, whether a key sits at the playhead, next key time).
/// The half-frame tolerance matches `keyframe_nav`.
pub(crate) fn key_nav_targets(
    times: &[f64],
    lt: f64,
    tol: f64,
) -> (Option<f64>, bool, Option<f64>) {
    let prev = times.iter().rev().find(|t| **t < lt - tol).copied();
    let on_key = times.iter().any(|t| (t - lt).abs() < tol);
    let next = times.iter().find(|t| **t > lt + tol).copied();
    (prev, on_key, next)
}

/// One axis's share of the linked row's diamond click. Removing strips this
/// axis's keys at the playhead — freezing the axis to its current value if
/// none remain, leaving a Static axis untouched. Adding upserts a key at the
/// playhead with the axis's current value, so both axes always key together.
pub(crate) fn toggle_key_at(
    slot: &lumit_core::anim::Property,
    lt: f64,
    tol: f64,
    remove: bool,
) -> lumit_core::anim::Animation {
    use lumit_core::anim::Animation;
    if !remove {
        return Animation::Keyframed(upsert_key(slot, lt, slot.value_at(lt)));
    }
    match &slot.animation {
        Animation::Keyframed(keys) => {
            let kept: Vec<_> = keys
                .iter()
                .filter(|k| (k.time.to_f64() - lt).abs() >= tol)
                .cloned()
                .collect();
            if kept.is_empty() {
                Animation::Static(slot.value_at(lt))
            } else {
                Animation::Keyframed(kept)
            }
        }
        Animation::Static(v) => Animation::Static(*v),
    }
}
