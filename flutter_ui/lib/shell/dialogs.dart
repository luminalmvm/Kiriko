// Modal dialogues driven from the menu bar (phase F4): the composition
// settings / new-composition dialogue and the shared modal scaffold.
//
// The dialogues are real chrome in the Settings-window visual style. Both now
// commit for real: Composition settings applies its whole field set through
// `app.setCompSettings` (one SetCompSettings undo step); New composition
// creates the comp through `app.newComposition`, then applies size, frame rate
// and duration to it with `setCompSettings` as one visible flow (the bridge's
// newComposition takes only a name).
//
// The dialogue is shown through the app's Overlay (like the menus and
// dropdowns) rather than shell state, so it needs no shell wiring. A dimmed
// backdrop eats clicks and closes on tap; Cancel/OK close from the buttons.

import 'package:flutter/widgets.dart';

import '../state/app_state.dart';
import '../widgets/controls.dart';

/// The frame-rate presets the egui composition dialogue offers (dialogs.rs).
/// The egui dialogue also accepts free-typed rates (e.g. 29.9997); this slice
/// offers the preset dropdown only.
const List<double> kFpsPresets = [
  23.976,
  24.0,
  25.0,
  29.97,
  30.0,
  50.0,
  59.94,
  60.0,
  120.0,
];

/// A frame-rate preset as the engine's exact rational `{num, den}` — the
/// fractional presets are the NTSC 1001 rates, the rest are whole/1. `SetComp
/// Settings` takes the pair, so the dialogue resolves the display double back
/// to it here.
(int num, int den) fpsRational(double fps) {
  if ((fps - 23.976).abs() < 0.01) return (24000, 1001);
  if ((fps - 29.97).abs() < 0.01) return (30000, 1001);
  if ((fps - 59.94).abs() < 0.01) return (60000, 1001);
  return (fps.round(), 1);
}

/// The nearest preset to an arbitrary [fps] (seeding the dropdown from an
/// existing comp whose rate might be a rational the presets round).
double nearestFpsPreset(double fps) {
  var best = kFpsPresets.first;
  var bestGap = (fps - best).abs();
  for (final p in kFpsPresets) {
    final gap = (fps - p).abs();
    if (gap < bestGap) {
      best = p;
      bestGap = gap;
    }
  }
  return best;
}

/// Open the composition-settings dialogue (edit the front comp). Apply commits
/// the field set through `app.setCompSettings` as one undo step.
Future<void> showCompositionSettingsDialog(
  BuildContext context,
  AppStateStub app,
) =>
    _showModal(
      context,
      (close) => _CompDialog(app: app, creating: false, close: close),
    );

/// Open the new-composition dialogue. On OK the name commits through the real
/// `app.newComposition` op.
Future<void> showNewCompositionDialog(
  BuildContext context,
  AppStateStub app,
) =>
    _showModal(
      context,
      (close) => _CompDialog(app: app, creating: true, close: close),
    );

/// Insert a centred modal into the app Overlay with a dimmed, click-to-dismiss
/// backdrop. Completes when the dialogue closes.
Future<void> _showModal(
  BuildContext context,
  Widget Function(VoidCallback close) builder,
) {
  final overlay = Overlay.of(context);
  late OverlayEntry entry;
  var done = false;
  void close() {
    if (done) return;
    done = true;
    entry.remove();
  }

  entry = OverlayEntry(
    builder: (context) {
      final t = ThemeScope.of(context).theme;
      return Stack(
        children: [
          Positioned.fill(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: close,
              child: Container(color: t.modalBackdrop),
            ),
          ),
          Center(
            child: GestureDetector(
              onTap: () {}, // swallow clicks inside the dialogue
              child: builder(close),
            ),
          ),
        ],
      );
    },
  );
  overlay.insert(entry);
  return Future<void>.value();
}

class _CompDialog extends StatefulWidget {
  final AppStateStub app;

  /// Creating a new comp (title "New composition", button "Create") versus
  /// editing settings ("Composition settings", "Apply").
  final bool creating;
  final VoidCallback close;

  const _CompDialog({
    required this.app,
    required this.creating,
    required this.close,
  });

  @override
  State<_CompDialog> createState() => _CompDialogState();
}

class _CompDialogState extends State<_CompDialog> {
  // Defaults mirror the egui new-comp dialogue (compositions.rs
  // open_new_comp_dialog): 1920×1080, 60 fps, 30 s. When editing, they seed
  // from the front comp instead.
  late final TextEditingController _name;
  final FocusNode _nameFocus = FocusNode();
  int _width = 1920;
  int _height = 1080;
  double _fps = 60.0;
  int _durationS = 30;

  @override
  void initState() {
    super.initState();
    final comp = widget.creating ? null : widget.app.frontComp;
    final name = widget.creating
        ? 'Comp'
        : _frontCompName(widget.app) ?? 'Comp';
    _name = TextEditingController(text: name);
    if (comp != null) {
      _width = comp.width;
      _height = comp.height;
      final fps = comp.fps.fps;
      if (fps > 0) {
        _fps = nearestFpsPreset(fps);
        _durationS = (comp.frameCount / fps).round().clamp(1, 86400);
      }
    }
  }

  /// The front comp's display name (the id-keyed lookup the tab strip uses).
  static String? _frontCompName(AppStateStub app) {
    final id = app.frontCompIdResolved;
    for (final c in app.compositions) {
      if (c.id == id) return c.name;
    }
    return null;
  }

  @override
  void dispose() {
    _name.dispose();
    _nameFocus.dispose();
    super.dispose();
  }

  void _confirm() {
    final app = widget.app;
    final (fpsNum, fpsDen) = fpsRational(_fps);
    final durationFrames = (_durationS * _fps).round();
    if (widget.creating) {
      // Two-step: the bridge's newComposition only takes a name, so create it,
      // then commit the full field set through setCompSettings on the new comp
      // (found as the composition the create added). One visible flow.
      final before = {for (final c in app.compositions) c.id};
      app.newComposition(_name.text.trim());
      final added = app.compositions.where((c) => !before.contains(c.id));
      if (added.isNotEmpty) {
        app.setCompSettings(added.first.id, _name.text.trim(), _width, _height,
            fpsNum, fpsDen, durationFrames);
      }
    } else {
      // Edit: commit the whole field set as one SetCompSettings undo step.
      final compId = app.frontCompIdResolved;
      if (compId != null) {
        app.setCompSettings(compId, _name.text.trim(), _width, _height, fpsNum,
            fpsDen, durationFrames);
      }
    }
    widget.close();
  }

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Container(
      width: 380,
      decoration: BoxDecoration(
        color: t.surface3,
        borderRadius: BorderRadius.circular(t.tokens.floatRadius),
        border: Border.all(color: t.hairline),
        boxShadow: t.floatShadow,
      ),
      padding: const EdgeInsets.all(14),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            widget.creating ? 'New composition' : 'Composition settings',
            style: t.heading,
          ),
          const SizedBox(height: 8),
          Container(height: 1, color: t.hairline),
          const SizedBox(height: 8),
          _DialogRow(
            label: 'Name',
            control: _NameField(controller: _name, focus: _nameFocus),
          ),
          _DialogRow(
            label: 'Size',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                DragValueField(
                  value: _width,
                  min: 16,
                  max: 16384,
                  speed: 8,
                  onChanged: (v) => setState(() => _width = v.round()),
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                  child: Text('×', style: t.small),
                ),
                DragValueField(
                  value: _height,
                  min: 16,
                  max: 16384,
                  speed: 8,
                  onChanged: (v) => setState(() => _height = v.round()),
                ),
              ],
            ),
          ),
          _DialogRow(
            label: 'Frame rate',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                BareDropdown<double>(
                  value: _fps,
                  options: kFpsPresets,
                  label: _fpsLabel,
                  onChanged: (v) => setState(() => _fps = v),
                ),
                const SizedBox(width: 6),
                Text('fps', style: t.small),
              ],
            ),
          ),
          _DialogRow(
            label: 'Duration',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                DragValueField(
                  value: _durationS,
                  min: 1,
                  max: 86400,
                  onChanged: (v) => setState(() => _durationS = v.round()),
                ),
                const SizedBox(width: 6),
                Text('sec', style: t.small),
              ],
            ),
          ),
          const SizedBox(height: 6),
          if (widget.creating)
            Text(
              'The new composition is created, then its size, frame rate and '
              'duration are applied in one step.',
              style: t.small.copyWith(color: t.textDisabled),
            ),
          const SizedBox(height: 12),
          Row(
            children: [
              const Spacer(),
              HouseButton(
                onPressed: widget.close,
                child: const Text('Cancel'),
              ),
              const SizedBox(width: 8),
              HouseButton(
                onPressed: _confirm,
                child: Text(widget.creating ? 'Create' : 'Apply'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Frame-rate label: a whole rate reads as an integer, a fractional one keeps
/// its decimals (23.976, 29.97).
String _fpsLabel(double v) =>
    v == v.roundToDouble() ? v.toInt().toString() : v.toString();

/// One label-left / control-right dialogue row (the Settings-window row style).
class _DialogRow extends StatelessWidget {
  final String label;
  final Widget control;
  const _DialogRow({required this.label, required this.control});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Row(
        children: [
          Expanded(child: Text(label, style: t.bodyPrimary)),
          const SizedBox(width: 12),
          control,
        ],
      ),
    );
  }
}

/// The name text field, in the Settings-window text-box style.
class _NameField extends StatelessWidget {
  final TextEditingController controller;
  final FocusNode focus;
  const _NameField({required this.controller, required this.focus});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Container(
      width: 200,
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
      decoration: BoxDecoration(
        color: t.surface0,
        borderRadius: BorderRadius.circular(t.tokens.controlRadius),
        border: Border.all(color: t.hairline),
      ),
      child: EditableText(
        controller: controller,
        focusNode: focus,
        style: t.bodyPrimary,
        cursorColor: t.accent,
        backgroundCursorColor: t.surface2,
        selectionColor: t.accent.withValues(alpha: 0.5),
      ),
    );
  }
}
