// The blend / matte / parent "last columns" — ported from the egui outline
// subcolumns (`blend_control`, `matte_control`, `parent_control` in
// crates/lumit-ui/src/shell/inspector/controls.rs, laid out in the timeline
// panel's row loop). egui shows them as inline dropdowns in a wide outline row;
// the Flutter outline is narrow and column-degraded, so — taking the option the
// port explicitly allows — these three live in the layer context menu instead,
// each opening its own picker popup. The pickers commit through the real ops
// (`listBlendModes`/`setBlendMode`, `setMatte`, `setParent`).
//
// In plain terms: right-clicking a layer now lets you set how it blends with
// what's below, use another layer as its matte (a stencil), and pin its motion
// to another layer (parenting).

import 'package:flutter/widgets.dart';

import '../../bridge/bridge.dart';
import '../../state/app_state.dart';
import '../../widgets/controls.dart';

/// True when pointing [layerId] at [candidateId] as its transform parent would
/// form a cycle — i.e. the candidate already hangs (directly or transitively)
/// off this layer. Pure so it is unit-tested; mirrors
/// `lumit_core::model::parenting_would_cycle`.
bool parentingWouldCycle(
    List<BridgeLayer> layers, String layerId, String candidateId) {
  if (layerId == candidateId) return true;
  String? cursor = candidateId;
  final seen = <String>{};
  while (cursor != null && seen.add(cursor)) {
    if (cursor == layerId) return true;
    final node = layers.where((l) => l.id == cursor);
    cursor = node.isEmpty ? null : node.first.parent;
  }
  return false;
}

/// The blend-mode picker: the whole registry (`listBlendModes`), the current
/// mode ticked, a pick committing `setBlendMode`.
Future<void> showBlendModePicker({
  required BuildContext context,
  required AppStateStub app,
  required String compId,
  required BridgeLayer layer,
  required Offset position,
}) async {
  final modes = app.listBlendModes();
  if (modes.isEmpty) return;
  final picked = await showLumitPopup<String>(
    context: context,
    position: position,
    builder: (close) => FloatSurface(
      width: 180,
      child: _ScrollColumn(
        children: [
          for (final m in modes)
            MenuRow(
              selected: (layer.blendMode ?? 'Normal') == m.name,
              onPressed: () => close(m.name),
              child: Text(m.label),
            ),
        ],
      ),
    ),
  );
  if (picked != null && picked != layer.blendMode) {
    app.setBlendMode(compId, layer.id, picked);
  }
}

/// The matte picker: None or another layer as the source, plus — when a matte
/// is set — the channel (Alpha/Luma, the matte "mode") and an Inverted toggle.
/// Each row commits `setMatte` and closes; reopen to change another facet.
Future<void> showMattePicker({
  required BuildContext context,
  required AppStateStub app,
  required String compId,
  required BridgeLayer layer,
  required Offset position,
}) async {
  final layers = app.frontComp?.layers ?? const <BridgeLayer>[];
  final others = [for (final l in layers) if (l.id != layer.id) l];
  final matte = layer.matte;
  await showLumitPopup<Object>(
    context: context,
    position: position,
    builder: (close) => FloatSurface(
      width: 200,
      child: _ScrollColumn(
        children: [
          MenuRow(
            selected: matte == null,
            onPressed: () {
              if (matte != null) app.setMatte(compId, layer.id, '', 'alpha', false);
              close(null);
            },
            child: const Text('None'),
          ),
          for (final other in others)
            MenuRow(
              selected: matte?.source == other.id,
              onPressed: () {
                app.setMatte(
                  compId,
                  layer.id,
                  other.id,
                  matte?.channel ?? 'alpha',
                  matte?.inverted ?? false,
                );
                close(null);
              },
              child: Text(other.name),
            ),
          if (matte != null) ...[
            const _PickerDivider(),
            MenuRow(
              selected: matte.channel == 'luma',
              onPressed: () {
                final nextChannel = matte.channel == 'luma' ? 'alpha' : 'luma';
                app.setMatte(
                    compId, layer.id, matte.source, nextChannel, matte.inverted);
                close(null);
              },
              child: const Text('Luma matte'),
            ),
            MenuRow(
              selected: matte.inverted,
              onPressed: () {
                app.setMatte(compId, layer.id, matte.source, matte.channel,
                    !matte.inverted);
                close(null);
              },
              child: const Text('Inverted'),
            ),
          ],
        ],
      ),
    ),
  );
}

/// The parent picker: None or another (non-cycling) layer as the transform
/// parent, committing `setParent`.
Future<void> showParentPicker({
  required BuildContext context,
  required AppStateStub app,
  required String compId,
  required BridgeLayer layer,
  required Offset position,
}) async {
  final layers = app.frontComp?.layers ?? const <BridgeLayer>[];
  final candidates = [
    for (final l in layers)
      if (l.id != layer.id && !parentingWouldCycle(layers, layer.id, l.id)) l,
  ];
  final picked = await showLumitPopup<String>(
    context: context,
    position: position,
    builder: (close) => FloatSurface(
      width: 180,
      child: _ScrollColumn(
        children: [
          MenuRow(
            selected: layer.parent == null,
            // The empty sentinel clears the parent.
            onPressed: () => close(''),
            child: const Text('None'),
          ),
          for (final cand in candidates)
            MenuRow(
              selected: layer.parent == cand.id,
              onPressed: () => close(cand.id),
              child: Text(cand.name),
            ),
        ],
      ),
    ),
  );
  if (picked == null) return; // dismissed
  final next = picked.isEmpty ? null : picked;
  if (next != layer.parent) {
    app.setParent(compId, layer.id, picked);
  }
}

/// A capped-height, scrollable column for a long picker list (the blend-mode
/// registry runs to ~30 modes).
class _ScrollColumn extends StatelessWidget {
  final List<Widget> children;
  const _ScrollColumn({required this.children});

  @override
  Widget build(BuildContext context) => ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 360),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: children,
          ),
        ),
      );
}

class _PickerDivider extends StatelessWidget {
  const _PickerDivider();
  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 4),
      child: Container(height: 1, color: t.hairline),
    );
  }
}
