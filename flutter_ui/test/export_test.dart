// The export dialogue, the one-at-a-time queue, live progress polling, and the
// size-targeted share export (docs/06 §7.1, K-037, K-119). Ported from
// export_actions.rs + app_update.rs. Widget + app-state tests over a fake
// DocumentBridge whose export poll is scripted (no library, no plugin channels).

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lumit_flutter/bridge/bridge.dart';
import 'package:lumit_flutter/shell/export_dialog.dart';
import 'package:lumit_flutter/state/app_state.dart';
import 'package:lumit_flutter/state/settings.dart';
import 'package:lumit_flutter/theme/theme.dart';
import 'package:lumit_flutter/widgets/controls.dart';

/// A fake bridge with a front comp "Scene" (1920×1080, 60 fps, 300 frames, one
/// footage layer). The export surface is the interesting part: `startExport`
/// records the out path and always succeeds (the Dart queue guards the
/// one-at-a-time rule, so the bridge is only asked when idle); `exportPoll`
/// drains a scripted list of states; `exportCancel` is recorded.
class _FakeBridge implements DocumentBridge {
  final List<String> startCalls = [];
  final List<String> ops = [];
  final List<BridgeExportState> pollScript = [];

  /// The stamp `exportPreset` returns per bridge preset name.
  BridgeExportPreset Function(String name)? presetStamp;

  static const _json = '''
  {
    "ok": true,
    "items": [
      {
        "id": "c1", "name": "Scene", "kind": "composition", "children": [],
        "comp": {
          "width": 1920, "height": 1080, "fps": {"num": 60, "den": 1},
          "frame_count": 300,
          "layers": [
            {"id":"l0","index":0,"name":"top","kind":"footage",
             "in_frame":0,"out_frame":300,"label":0,"switches":{}}
          ],
          "markers": []
        }
      }
    ],
    "can_undo": false, "can_redo": false, "path": null
  }''';

  BridgeReply _snap() => BridgeReply.parse(_json);

  @override
  BridgeExportPreset exportPreset(
      String presetName, String compName, String template) {
    final stamp = presetStamp;
    if (stamp != null) return stamp(presetName);
    return BridgeExportPreset.idle;
  }

  @override
  BridgeReply startExport(String compId, String specJson, String outPath) {
    startCalls.add(outPath);
    return _snap();
  }

  @override
  BridgeExportState exportPoll() =>
      pollScript.isEmpty ? BridgeExportState.idle : pollScript.removeAt(0);

  @override
  BridgeReply exportCancel() {
    ops.add('export_cancel');
    return _snap();
  }

  // --- Everything else: quiet defaults (unused by these tests) --------------
  @override
  BridgeReply snapshot() => _snap();
  @override
  BridgeReply newProject() => _snap();
  @override
  BridgeReply undo() => _snap();
  @override
  BridgeReply redo() => _snap();
  @override
  BridgeReply openProject(String path) => _snap();
  @override
  BridgeReply saveProject(String path) => _snap();
  @override
  BridgeReply newComposition(String name) => _snap();
  @override
  BridgeReply importFootage(String path) => _snap();
  @override
  BridgeReply setLayerSwitch(
          String compId, String layerId, String switchName, bool value) =>
      _snap();
  @override
  BridgeReply editLayerSpan(
          String compId, String layerId, String edit, int frame) =>
      _snap();
  @override
  BridgeReply setTransform(
          String compId, String layerId, String property, double value) =>
      _snap();
  @override
  BridgeReply addMarker(String compId, int frame) => _snap();
  @override
  BridgeReply addSolidLayer(String compId) => _snap();
  @override
  BridgeReply addTextLayer(String compId) => _snap();
  @override
  BridgeReply addCameraLayer(String compId) => _snap();
  @override
  BridgeReply addAdjustmentLayer(String compId) => _snap();
  @override
  BridgeReply addSequenceLayer(String compId) => _snap();
  @override
  BridgeReply deleteLayer(String compId, String layerId) => _snap();
  @override
  BridgeReply duplicateLayer(String compId, String layerId) => _snap();
  @override
  BridgeReply setCompSettings(String compId, String name, int width, int height,
          int fpsNum, int fpsDen, int durationFrames) =>
      _snap();
  @override
  BridgeReply togglePropertyAnimated(
          String compId, String layerId, String property, int frame) =>
      _snap();
  @override
  BridgeReply addKeyframe(String compId, String layerId, String property,
          int frame, double value) =>
      _snap();
  @override
  BridgeReply removeKeyframe(
          String compId, String layerId, String property, int frame) =>
      _snap();
  @override
  BridgeReply shiftKeyframes(String compId, String layerId, String property,
          List<int> frames, int delta) =>
      _snap();
  @override
  BridgeReply setWorkAreaEdge(String compId, int frame, bool isOut) => _snap();
  @override
  List<BridgeEffectInfo> listEffects() => const [];
  @override
  BridgeReply addEffect(String compId, String layerId, String effectName) =>
      _snap();
  @override
  BridgeReply removeEffect(String compId, String layerId, String effectId) =>
      _snap();
  @override
  BridgeReply setEffectEnabled(
          String compId, String layerId, String effectId, bool enabled) =>
      _snap();
  @override
  BridgeReply setEffectParamScalar(String compId, String layerId,
          String effectId, String paramName, double value) =>
      _snap();
  @override
  BridgeReply setEffectParamColour(String compId, String layerId,
          String effectId, String paramName, double r, double g, double b,
          double a) =>
      _snap();
  @override
  BridgeReply setKeyframeInterp(String compId, String layerId, String property,
          int frame, String interpIn, String interpOut, double speedIn,
          double influenceIn, double speedOut, double influenceOut) =>
      _snap();
  @override
  BridgeReply setRetimeEnabled(String compId, String layerId, bool enabled) =>
      _snap();
  @override
  BridgeReply setRetimeSpeed(String compId, String layerId, double speed) =>
      _snap();
  @override
  BridgeReply setSegmentPreset(
          String compId, String layerId, int frame, String ease) =>
      _snap();
  @override
  BridgeReply segmentToRate(String compId, String layerId, int frame) =>
      _snap();
  @override
  BridgeReply dragBoundary(
          String compId, String layerId, int index, int frame) =>
      _snap();
  @override
  List<BridgeBlendMode> listBlendModes() => const [];
  @override
  BridgeReply setBlendMode(String compId, String layerId, String mode) =>
      _snap();
  @override
  BridgeReply setMatte(String compId, String layerId, String source,
          String channel, bool inverted) =>
      _snap();
  @override
  BridgeReply setParent(String compId, String layerId, String parent) =>
      _snap();
  @override
  BridgeReply setMotionBlur(String compId, bool enabled, double shutterAngle,
          double shutterPhase, int samples) =>
      _snap();
  @override
  BridgeReply addMask(String compId, String layerId, String kind) => _snap();
  @override
  DecodedFrame? decodeFrame(String itemId, int frame) => null;
}

/// A minimal host: the theme scope over an Overlay holding [child].
Widget _host(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: MediaQuery(
        data: const MediaQueryData(size: Size(700, 700)),
        child: ThemeScope(
          theme: LumitTheme.forScheme(LumitColorScheme.dark, ThemeShape.sharp),
          animationLevel: AnimationLevel.none,
          showTooltips: false,
          child: Overlay(
            initialEntries: [OverlayEntry(builder: (_) => child)],
          ),
        ),
      ),
    );

void main() {
  group('share-export bitrate maths (K-037)', () {
    // Faithful port of Shell::start_share_export: byte budget × 8 bits × 0.92
    // container headroom, spread over the duration, audio's share removed first,
    // floored at 100 kbps.
    test('the size target divides over the duration with headroom', () {
      // 50 MB, 60 s, no audio: 50e6·8·0.92 / 60 = 6_133_333 (truncated).
      expect(
        shareExportBitRate(targetMb: 50, durationSeconds: 60, hasAudio: false),
        6133333,
      );
      // 10 MB, 30 s, no audio: 10e6·8·0.92 / 30 = 2_453_333.
      expect(
        shareExportBitRate(targetMb: 10, durationSeconds: 30, hasAudio: false),
        2453333,
      );
    });

    test('audio takes 192 kbps out of the budget first', () {
      // 50 MB, 60 s, with audio: (50e6·8·0.92 − 192000·60) / 60 = 5_941_333.
      expect(
        shareExportBitRate(targetMb: 50, durationSeconds: 60, hasAudio: true),
        5941333,
      );
    });

    test('the bitrate never drops below 100 kbps', () {
      // A tiny budget over a long duration floors at 100 kbps.
      expect(
        shareExportBitRate(targetMb: 1, durationSeconds: 600, hasAudio: false),
        100000,
      );
    });

    test('a near-zero duration is floored at 0.1 s (no divide-by-zero)', () {
      // 10 MB / 0.1 s = 736_000_000 (the 0.1 floor stands in for 0).
      expect(
        shareExportBitRate(targetMb: 10, durationSeconds: 0, hasAudio: false),
        736000000,
      );
    });
  });

  group('export queue + polling', () {
    test('a second export waits, then starts on the first completing', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);

      // The first export starts immediately.
      app.queueExport('c1', '{"preset":"custom"}', 'out1.mp4');
      expect(fake.startCalls, ['out1.mp4']);
      expect(app.exportRunning, isTrue);
      expect(app.exportName, 'out1.mp4');

      // The second only queues while one runs.
      app.queueExport('c1', '{"preset":"custom"}', 'out2.mp4');
      expect(fake.startCalls, ['out1.mp4'], reason: 'not started yet');
      expect(app.exportQueueLength, 1);

      // A running poll updates the progress readout.
      fake.pollScript.add(const BridgeExportState(
          state: 'running', frame: 5, total: 10, encoder: 'libx264'));
      app.exportPollTick();
      expect(app.exportFrame, 5);
      expect(app.exportTotal, 10);
      expect(app.exportEncoder, 'libx264');

      // The first finishing starts the second and posts the quiet notice.
      fake.pollScript
          .add(const BridgeExportState(state: 'done', path: 'out1.mp4'));
      app.exportPollTick();
      expect(app.notice, 'exported out1.mp4 — encoded with libx264');
      expect(app.errorNotice, isNull);
      expect(fake.startCalls, ['out1.mp4', 'out2.mp4']);
      expect(app.exportName, 'out2.mp4');
      expect(app.exportQueueLength, 0);
    });

    test('a failure takes the error tint and still starts the next', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      app.queueExport('c1', '{}', 'a.mp4');
      app.queueExport('c1', '{}', 'b.mp4');

      fake.pollScript
          .add(const BridgeExportState(state: 'failed', error: 'cancelled'));
      app.exportPollTick();
      expect(app.errorNotice, 'export: cancelled');
      // The next export starts despite the failure.
      expect(fake.startCalls, ['a.mp4', 'b.mp4']);
      expect(app.exportName, 'b.mp4');
    });

    test('the status line reads exactly as app_update.rs words it', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      app.queueExport('c1', '{}', 'a.mp4');
      app.queueExport('c1', '{}', 'b.mp4'); // one queued behind

      fake.pollScript.add(const BridgeExportState(
          state: 'running', frame: 3, total: 12, encoder: 'h264_nvenc'));
      app.exportPollTick();
      expect(app.exportStatusText,
          'exporting a.mp4 3/12 · h264_nvenc · 1 queued');
    });

    test('cancel calls the bridge cancel', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      app.queueExport('c1', '{}', 'a.mp4');
      app.cancelExport();
      expect(fake.ops, contains('export_cancel'));
    });

    test('a share export sizes the bitrate and queues one export', () async {
      final fake = _FakeBridge();
      final app = AppStateStub(
        bridge: fake,
        exportSaveLocationPicker: (name) async => 'C:/tmp/$name',
      );
      await app.startShareExport(50);
      // The comp is 300 frames at 60 fps = 5 s; the picker names it share-50mb.
      expect(fake.startCalls, ['C:/tmp/share-50mb.mp4']);
      expect(app.exportName, 'share-50mb.mp4');
    });
  });

  group('export dialogue', () {
    testWidgets('picking a preset stamps the codec, size and bitrate fields',
        (tester) async {
      await tester.binding.setSurfaceSize(const Size(700, 700));
      final fake = _FakeBridge()
        ..presetStamp = (name) => name == 'youtube_1440p60'
            ? const BridgeExportPreset(
                preset: 'youtube_1440p60',
                codec: 'hevc',
                size: [2560, 1440],
                bitrateMbps: '25',
                includeAudio: true,
                defaultName: 'youtube-1440p60.mp4',
              )
            : BridgeExportPreset.idle;
      final app = AppStateStub(bridge: fake);

      late BuildContext ctx;
      await tester.pumpWidget(_host(Builder(builder: (context) {
        ctx = context;
        return const SizedBox();
      })));
      showExportDialog(ctx, app, preset: ExportPreset.custom, template: '');
      await tester.pump();

      // Custom stamps the comp's own size and a blank bitrate (H.264).
      expect(find.text('1920×1080 (comp size)'), findsOneWidget);
      expect(find.text('H.264'), findsOneWidget);

      // Open the preset dropdown and choose YouTube 1440p60.
      await tester.tap(find.text('Custom (comp size)'));
      await tester.pump();
      await tester.tap(find.text('YouTube 1440p60').last);
      await tester.pump();

      // The stamp lands on the fields: HEVC, the delivery frame, 25 Mbps.
      expect(find.text('HEVC'), findsOneWidget);
      expect(find.text('2560×1440'), findsOneWidget);
      expect(find.text('25'), findsOneWidget);
    });
  });
}
