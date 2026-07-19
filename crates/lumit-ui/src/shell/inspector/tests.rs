use super::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod section_bar_tests {
    use super::*;

    /// Regression for the invisible effect-title bars: the bar was painted in
    /// `surface_1`, which is the very colour the Round shape fills each pane
    /// card with and sits within a few RGB steps of the Sharp background
    /// (`surface_0`) — so in the Effect Controls panel the bar could not be
    /// seen at all. The fill must stand apart from BOTH pane backgrounds in
    /// every colour scheme, or effect boundaries vanish again.
    #[test]
    fn the_effect_title_bar_stands_apart_from_both_pane_backgrounds() {
        use crate::theme::{ColorScheme, ThemeShape};
        for scheme in ColorScheme::ALL {
            let theme = Theme::for_scheme(scheme, ThemeShape::Sharp);
            let fill = section_bar_fill(&theme);
            assert_ne!(
                fill, theme.surface_0,
                "{scheme:?}: the bar must not match the Sharp panel background"
            );
            assert_ne!(
                fill, theme.surface_1,
                "{scheme:?}: the bar must not match the Round pane card fill"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod lane_key_tests {
    use super::*;
    use crate::app_state::{LaneKeySel, PropRow};
    use lumit_core::anim::{Keyframe, SideInterp};
    use lumit_core::model::TransformProp;

    fn key(t: f64, interp: SideInterp) -> Keyframe {
        Keyframe {
            time: rational_at(t),
            value: 0.0,
            interp_in: interp,
            interp_out: interp,
        }
    }

    // A lane drag shifts only the keys at the named times, and by the whole
    // delta — the group slides rigidly (note 2.1).
    #[test]
    fn shift_moves_only_the_named_times() {
        let keys = [
            key(0.0, SideInterp::Linear),
            key(1.0, SideInterp::Linear),
            key(2.0, SideInterp::Linear),
        ];
        let out = shift_keys_time(&keys, &[rational_at(1.0)], 0.5, 30.0);
        let times: Vec<f64> = out.iter().map(|k| k.time.to_f64()).collect();
        assert_eq!(times, vec![0.0, 1.5, 2.0]);
    }

    // A key dragged onto another key's time collapses to one (the collision rule
    // the graph editor uses), never a duplicate-time pair.
    #[test]
    fn shift_dedups_on_collision() {
        let keys = [
            key(0.0, SideInterp::Linear),
            key(1.0, SideInterp::Linear),
            key(2.0, SideInterp::Linear),
        ];
        let out = shift_keys_time(&keys, &[rational_at(1.0)], 1.0, 30.0);
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .all(|k| k.time.to_f64() == 0.0 || k.time.to_f64() == 2.0));
    }

    // Time never goes negative, and bezier handles ride along with the key.
    #[test]
    fn shift_clamps_and_keeps_handles() {
        let bez = SideInterp::Bezier {
            speed: 3.5,
            influence: 0.4,
        };
        let keys = [key(0.5, bez)];
        let out = shift_keys_time(&keys, &[rational_at(0.5)], -2.0, 30.0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].time.to_f64(), 0.0);
        assert_eq!(out[0].interp_in, bez);
        assert_eq!(out[0].interp_out, bez);
    }

    fn sel(t: f64) -> LaneKeySel {
        LaneKeySel {
            layer: uuid::Uuid::nil(),
            row: PropRow::Transform(TransformProp::Rotation),
            time: rational_at(t),
        }
    }

    #[test]
    fn plain_click_replaces_the_selection() {
        let mut s = vec![sel(1.0), sel(2.0)];
        lane_select_click(&mut s, sel(3.0), egui::Modifiers::default());
        assert_eq!(s, vec![sel(3.0)]);
    }

    #[test]
    fn ctrl_click_toggles_membership() {
        let mut s = vec![sel(1.0)];
        let ctrl = egui::Modifiers {
            ctrl: true,
            ..Default::default()
        };
        lane_select_click(&mut s, sel(2.0), ctrl); // add
        assert_eq!(s, vec![sel(1.0), sel(2.0)]);
        lane_select_click(&mut s, sel(1.0), ctrl); // remove
        assert_eq!(s, vec![sel(2.0)]);
    }

    #[test]
    fn shift_click_extends_without_removing() {
        let mut s = vec![sel(1.0)];
        let shift = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        lane_select_click(&mut s, sel(2.0), shift);
        lane_select_click(&mut s, sel(2.0), shift); // already in — no duplicate
        assert_eq!(s, vec![sel(1.0), sel(2.0)]);
    }

    fn psel(prop: TransformProp) -> crate::app_state::PropSel {
        crate::app_state::PropSel {
            layer: uuid::Uuid::nil(),
            row: PropRow::Transform(prop),
        }
    }

    // Shift-click ranges over the drawn order between anchor and target,
    // inclusive, whichever way round they sit (note 2.6b).
    #[test]
    fn prop_range_covers_the_rows_between() {
        let order = vec![
            psel(TransformProp::AnchorX),
            psel(TransformProp::PositionX),
            psel(TransformProp::ScaleX),
            psel(TransformProp::Rotation),
            psel(TransformProp::Opacity),
        ];
        let (range, to_anchor) = prop_range(
            &order,
            Some(psel(TransformProp::PositionX)),
            psel(TransformProp::Rotation),
        );
        assert!(!to_anchor);
        assert_eq!(
            range,
            vec![
                psel(TransformProp::PositionX),
                psel(TransformProp::ScaleX),
                psel(TransformProp::Rotation),
            ]
        );
        // Reversed (target above the anchor) gives the same inclusive span.
        let (range_rev, _) = prop_range(
            &order,
            Some(psel(TransformProp::Rotation)),
            psel(TransformProp::PositionX),
        );
        assert_eq!(range_rev.len(), 3);
        assert_eq!(range_rev.first(), Some(&psel(TransformProp::PositionX)));
    }

    // No usable anchor: Shift-click falls back to selecting just the target.
    #[test]
    fn prop_range_without_anchor_selects_target() {
        let order = vec![psel(TransformProp::Rotation)];
        let (range, to_anchor) = prop_range(&order, None, psel(TransformProp::Rotation));
        assert!(to_anchor);
        assert_eq!(range, vec![psel(TransformProp::Rotation)]);
    }
}
