// The export dialogue (phase F4), ported from export_actions.rs
// `export_dialog_modal` + `ExportDialogState` and docs/07/06 §7.1. A modal in
// the Settings-window visual style: pick a delivery preset (which stamps the
// codec, size and bitrate through the bridge's `exportPreset` resolver), tune
// the fields, choose where to save, then Queue/Export.
//
// The spec resolution lives engine-side: the dialogue sends the field state as
// `spec_json` and the bridge reproduces `ExportDialogState::spec` (the VBR-peak
// and 1.5x-peak rules). Editing the bitrate simply passes the new Mbps through
// — the peak switch happens in the resolver, keyed off the preset name we send.
//
// Shown through the app Overlay (like the composition dialogues), so it needs
// no shell state; the poll timer and status line live in the shell. Without a
// bridge the menu never opens this — it keeps the F0 `engine` notice instead.

import 'dart:convert';

import 'package:flutter/widgets.dart';

import '../state/app_state.dart';
import '../state/settings.dart';
import '../widgets/controls.dart';

/// Open the export dialogue for the front composition, stamped with [preset].
/// [template] is Settings → Export's filename template (K-119), passed to the
/// resolver for the suggested file name.
void showExportDialog(
  BuildContext context,
  AppStateStub app, {
  required ExportPreset preset,
  required String template,
}) {
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
              child: _ExportDialog(
                app: app,
                initialPreset: preset,
                template: template,
                close: close,
              ),
            ),
          ),
        ],
      );
    },
  );
  overlay.insert(entry);
}

/// The bridge's snake_case preset name for an [ExportPreset] (the key
/// `exportPreset`/`start_export` resolve against).
String bridgePresetName(ExportPreset p) => switch (p) {
      ExportPreset.custom => 'custom',
      ExportPreset.youtube1080p60 => 'youtube_1080p60',
      ExportPreset.youtube1440p60 => 'youtube_1440p60',
      ExportPreset.youtube4k60 => 'youtube_4k60',
      ExportPreset.vertical1080p60 => 'vertical_1080p60',
    };

/// The codec display label (the exporter's `VideoCodec::label`).
String _codecLabel(String codec) => switch (codec) {
      'hevc' => 'HEVC',
      _ => 'H.264',
    };

class _ExportDialog extends StatefulWidget {
  final AppStateStub app;
  final ExportPreset initialPreset;
  final String template;
  final VoidCallback close;

  const _ExportDialog({
    required this.app,
    required this.initialPreset,
    required this.template,
    required this.close,
  });

  @override
  State<_ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends State<_ExportDialog> {
  late ExportPreset _preset;
  String _codec = 'h264';

  /// None (the comp's own size) vs an explicit delivery frame `[w, h]`.
  List<int>? _size;
  final TextEditingController _bitrate = TextEditingController();
  final FocusNode _bitrateFocus = FocusNode();
  bool _includeAudio = true;
  String _defaultName = 'export.mp4';

  /// The chosen save path, or null until the user picks one (the confirm falls
  /// through to the picker when so).
  String? _savePath;

  AppStateStub get app => widget.app;

  @override
  void initState() {
    super.initState();
    _preset = widget.initialPreset;
    _stamp(_preset); // plain field assignment before the first build
  }

  @override
  void dispose() {
    _bitrate.dispose();
    _bitrateFocus.dispose();
    super.dispose();
  }

  /// Stamp a preset's fields over the editable state, via the engine-side
  /// resolver (`ExportDialogState::apply`). Without a bridge the resolver
  /// returns the idle defaults (custom, comp size, blank bitrate). Assigns the
  /// fields directly — callers wrap it in [setState] when past the first build.
  void _stamp(ExportPreset preset) {
    final stamp = app.exportPreset(
      bridgePresetName(preset),
      app.frontCompName,
      widget.template,
    );
    _preset = preset;
    _codec = stamp.codec;
    _size = stamp.size;
    _bitrate.text = stamp.bitrateMbps;
    _includeAudio = stamp.includeAudio;
    _defaultName = stamp.defaultName;
  }

  (int, int) get _compSize {
    final comp = app.frontComp;
    return (comp?.width ?? 0, comp?.height ?? 0);
  }

  Future<void> _pickSavePath() async {
    final path = await app.exportSaveLocationPicker(_savePath == null
        ? _defaultName
        : exportFileName(_savePath!));
    if (path != null) setState(() => _savePath = path);
  }

  Future<void> _confirm() async {
    final compId = app.frontCompIdResolved;
    if (compId == null) {
      widget.close();
      return;
    }
    final path = _savePath ?? await app.exportSaveLocationPicker(_defaultName);
    if (path == null) return; // cancelled — keep the dialogue open
    final specJson = jsonEncode({
      'preset': bridgePresetName(_preset),
      'codec': _codec,
      'size': _size,
      'bitrate_mbps': _bitrate.text,
      'include_audio': _includeAudio,
    });
    app.queueExport(compId, specJson, path);
    widget.close();
  }

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    final (cw, ch) = _compSize;
    final (sw, sh) = _size == null ? (cw, ch) : (_size![0], _size![1]);
    final sizeSuffix = _size == null ? ' (comp size)' : '';
    final running = app.exportRunning;
    return Container(
      width: 400,
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
          Text('Export', style: t.heading),
          const SizedBox(height: 8),
          Container(height: 1, color: t.hairline),
          const SizedBox(height: 8),
          _Row(
            label: 'Preset',
            control: BareDropdown<ExportPreset>(
              value: _preset,
              options: ExportPreset.values,
              label: (p) => p.label,
              onChanged: (p) => setState(() => _stamp(p)),
            ),
          ),
          _Row(
            label: 'Codec',
            control: BareDropdown<String>(
              value: _codec,
              options: const ['h264', 'hevc'],
              label: _codecLabel,
              onChanged: (c) => setState(() => _codec = c),
            ),
          ),
          _Row(
            label: 'Frame',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text('$sw×$sh$sizeSuffix',
                    style: t.bodyPrimary.copyWith(color: t.textMuted)),
                if (_size != null) ...[
                  const SizedBox(width: 8),
                  HouseButton(
                    small: true,
                    onPressed: () => setState(() => _size = null),
                    child: const Text('Use comp size'),
                  ),
                ],
              ],
            ),
          ),
          _Row(
            label: 'Bitrate',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _BitrateField(controller: _bitrate, focus: _bitrateFocus),
                const SizedBox(width: 8),
                Flexible(
                  child: Text('Mbps — blank for default',
                      style: t.small.copyWith(color: t.textMuted),
                      overflow: TextOverflow.ellipsis),
                ),
              ],
            ),
          ),
          _Row(
            label: 'Audio',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                HouseCheckbox(
                  value: _includeAudio,
                  onChanged: (v) => setState(() => _includeAudio = v),
                ),
                const SizedBox(width: 8),
                Flexible(
                  child: Text('Include audio (AAC 320 kbps)',
                      style: t.bodyPrimary, overflow: TextOverflow.ellipsis),
                ),
              ],
            ),
          ),
          _Row(
            label: 'Save to',
            control: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text(
                    _savePath == null
                        ? _defaultName
                        : exportFileName(_savePath!),
                    style: t.small.copyWith(
                        color: _savePath == null
                            ? t.textDisabled
                            : t.textSecondary),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 8),
                HouseButton(
                  small: true,
                  onPressed: _pickSavePath,
                  child: const Text('Choose…'),
                ),
              ],
            ),
          ),
          const SizedBox(height: 6),
          Text(
            'Frame rate follows the composition. Exports queue and run in order.',
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
                child: Text(running ? 'Queue export' : 'Export…'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// The last path segment of [path] — shared with the app state's own
/// file-name helper, exposed here so the dialogue can label a chosen path.
String exportFileName(String path) {
  final parts = path.split(RegExp(r'[/\\]'));
  for (final part in parts.reversed) {
    if (part.isNotEmpty) return part;
  }
  return 'export';
}

/// One label-left / control-right dialogue row (the Settings-window row style,
/// matching the composition dialogues).
class _Row extends StatelessWidget {
  final String label;
  final Widget control;
  const _Row({required this.label, required this.control});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Row(
        children: [
          SizedBox(width: 72, child: Text(label, style: t.bodyPrimary)),
          const SizedBox(width: 12),
          // Bound the control to the remaining width so a long descriptive
          // suffix can shrink/ellipsis rather than overflow the dialogue.
          Flexible(child: control),
        ],
      ),
    );
  }
}

/// The bitrate text box, in the Settings-window text-box style.
class _BitrateField extends StatelessWidget {
  final TextEditingController controller;
  final FocusNode focus;
  const _BitrateField({required this.controller, required this.focus});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Container(
      width: 72,
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
