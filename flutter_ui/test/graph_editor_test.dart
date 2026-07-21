// Phase-F3 Retime graph-lens tests: the pure speed-lens maths (ease shapes,
// segment→polyline sampling, map-segment derivative, boundary hit-testing and
// drag clamping, segment-at-frame) with no widget tree, and widget tests over
// the live GraphEditor driven by a fake DocumentBridge (the graph appears when
// the graph lens is on with a footage selection, a preset click stamps the
// segment under the playhead, a boundary drag commits `dragBoundary`, and →Rate
// surfaces the conversion notice).

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lumit_flutter/bridge/bridge.dart';
import 'package:lumit_flutter/panels/timeline/graph_editor.dart';
import 'package:lumit_flutter/panels/timeline/graph_maths.dart';
import 'package:lumit_flutter/panels/timeline_panel.dart';
import 'package:lumit_flutter/state/app_state.dart';
import 'package:lumit_flutter/theme/theme.dart';
import 'package:lumit_flutter/widgets/controls.dart';

/// A comp ("Scene") whose footage layer "hero" carries a two-segment Retime:
/// a Rate ramp 100%→200% (Linear) over frames 0..60, then a Rate hold at 200%
/// (Linear) over 60..120. Boundaries at frames 0, 60, 120 (one interior). "flat"
/// is a plain footage layer with no Retime.
const _retimeJson = '''
{
  "ok": true,
  "items": [
    {
      "id": "c0", "name": "Scene", "kind": "composition", "children": [],
      "comp": {
        "width": 1920, "height": 1080,
        "fps": {"num": 60, "den": 1}, "frame_count": 120,
        "layers": [
          {
            "id": "hero", "index": 0, "name": "hero", "kind": "footage",
            "in_frame": 0, "out_frame": 120, "label": 2,
            "switches": {"visible": true, "audible": true, "locked": false,
              "three_d": false, "collapse": false, "fx": true,
              "solo": false, "motion_blur": false},
            "retime": {
              "reverse": false, "interpolation": "nearest",
              "boundaries": [
                {"t_frame": 0, "t_seconds": 0.0, "s_seconds": 0.0, "smooth": false},
                {"t_frame": 60, "t_seconds": 1.0, "s_seconds": 1.5, "smooth": false},
                {"t_frame": 120, "t_seconds": 2.0, "s_seconds": 3.5, "smooth": false}
              ],
              "segments": [
                {"kind": "rate", "v0": 1.0, "v1": 2.0, "ease": "Linear"},
                {"kind": "rate", "v0": 2.0, "v1": 2.0, "ease": "Linear"}
              ]
            }
          },
          {
            "id": "flat", "index": 1, "name": "flat", "kind": "footage",
            "in_frame": 0, "out_frame": 120, "label": 0,
            "switches": {"visible": true, "audible": true, "locked": false,
              "three_d": false, "collapse": false, "fx": true,
              "solo": false, "motion_blur": false}
          }
        ],
        "markers": []
      }
    }
  ],
  "can_undo": false, "can_redo": false, "path": null
}''';

BridgeRetime _heroRetime() {
  final snap = BridgeReply.parse(_retimeJson).snapshot!;
  final comp = snap.items.first.comp!;
  return comp.layers.firstWhere((l) => l.id == 'hero').retime!;
}

/// A fake bridge that answers with the Retime document and records ops.
class _GraphFake implements DocumentBridge {
  final List<String> ops = [];
  BridgeReply _snap() => BridgeReply.parse(_retimeJson);
  BridgeReply _op(String record) {
    ops.add(record);
    return _snap();
  }

  @override
  BridgeReply snapshot() => _snap();
  @override
  BridgeReply newProject() => _snap();
  @override
  BridgeReply undo() => _snap();
  @override
  BridgeReply redo() => _snap();
  @override
  BridgeReply openProject(String p) => _snap();
  @override
  BridgeReply saveProject(String p) => _snap();
  @override
  BridgeReply newComposition(String name) => _snap();
  @override
  BridgeReply importFootage(String p) => _snap();
  @override
  BridgeReply setLayerSwitch(
          String compId, String layerId, String switchName, bool value) =>
      _op('switch:$layerId/$switchName=$value');
  @override
  BridgeReply editLayerSpan(
          String compId, String layerId, String edit, int frame) =>
      _op('span:$layerId/$edit@$frame');
  @override
  BridgeReply setTransform(
          String compId, String layerId, String property, double value) =>
      _op('transform:$layerId/$property=$value');
  @override
  BridgeReply addMarker(String compId, int frame) => _op('marker@$frame');
  @override
  BridgeReply addSolidLayer(String compId) => _op('add_solid');
  @override
  BridgeReply addTextLayer(String compId) => _op('add_text');
  @override
  BridgeReply addCameraLayer(String compId) => _op('add_camera');
  @override
  BridgeReply addAdjustmentLayer(String compId) => _op('add_adjustment');
  @override
  BridgeReply addSequenceLayer(String compId) => _op('add_sequence');
  @override
  BridgeReply deleteLayer(String compId, String layerId) => _op('delete');
  @override
  BridgeReply duplicateLayer(String compId, String layerId) => _op('dup');
  @override
  BridgeReply setCompSettings(String compId, String name, int width, int height,
          int fpsNum, int fpsDen, int durationFrames) =>
      _op('comp_settings');
  @override
  BridgeReply togglePropertyAnimated(
          String compId, String layerId, String property, int frame) =>
      _op('stopwatch');
  @override
  BridgeReply addKeyframe(String compId, String layerId, String property,
          int frame, double value) =>
      _op('add_key');
  @override
  BridgeReply removeKeyframe(
          String compId, String layerId, String property, int frame) =>
      _op('remove_key');
  @override
  BridgeReply shiftKeyframes(String compId, String layerId, String property,
          List<int> frames, int delta) =>
      _op('shift_keys');
  @override
  BridgeReply setWorkAreaEdge(String compId, int frame, bool isOut) =>
      _op('work_area');
  @override
  List<BridgeEffectInfo> listEffects() => const [];
  @override
  BridgeReply addEffect(String compId, String layerId, String effectName) =>
      _op('add_effect');
  @override
  BridgeReply removeEffect(String compId, String layerId, String effectId) =>
      _op('remove_effect');
  @override
  BridgeReply setEffectEnabled(
          String compId, String layerId, String effectId, bool enabled) =>
      _op('effect_enabled');
  @override
  BridgeReply setEffectParamScalar(String compId, String layerId,
          String effectId, String paramName, double value) =>
      _op('effect_scalar');
  @override
  BridgeReply setEffectParamColour(String compId, String layerId,
          String effectId, String paramName, double r, double g, double b,
          double a) =>
      _op('effect_colour');
  @override
  BridgeReply setKeyframeInterp(String compId, String layerId, String property,
          int frame, String interpIn, String interpOut, double speedIn,
          double influenceIn, double speedOut, double influenceOut) =>
      _op('key_interp');
  @override
  BridgeReply setRetimeEnabled(String compId, String layerId, bool enabled) =>
      _op('retime_enabled:$layerId=$enabled');
  @override
  BridgeReply setRetimeSpeed(String compId, String layerId, double speed) =>
      _op('retime_speed:$layerId=$speed');
  @override
  BridgeReply setSegmentPreset(
          String compId, String layerId, int frame, String ease) =>
      _op('preset:$layerId@$frame=$ease');
  @override
  BridgeReply segmentToRate(String compId, String layerId, int frame) =>
      _op('to_rate:$layerId@$frame');
  @override
  BridgeReply dragBoundary(
          String compId, String layerId, int index, int frame) =>
      _op('drag_boundary:$layerId/$index@$frame');
  @override
  List<BridgeBlendMode> listBlendModes() => const [];
  @override
  BridgeReply setBlendMode(String compId, String layerId, String mode) =>
      _op('blend');
  @override
  BridgeReply setMatte(String compId, String layerId, String source,
          String channel, bool inverted) =>
      _op('matte');
  @override
  BridgeReply setParent(String compId, String layerId, String parent) =>
      _op('parent');
  @override
  BridgeReply setMotionBlur(String compId, bool enabled, double shutterAngle,
          double shutterPhase, int samples) =>
      _op('mb');
  @override
  BridgeReply addMask(String compId, String layerId, String kind) => _op('mask');
  @override
  BridgeExportPreset exportPreset(
          String presetName, String compName, String template) =>
      BridgeExportPreset.idle;
  @override
  BridgeReply startExport(String compId, String specJson, String outPath) =>
      _snap();
  @override
  BridgeExportState exportPoll() => BridgeExportState.idle;
  @override
  BridgeReply exportCancel() => _snap();
  @override
  DecodedFrame? decodeFrame(String itemId, int frame) => null;
}

// Fills the whole (surface-sized) view at the origin, so a boundary's
// `scale.xOfFrame` maps straight to a global gesture coordinate.
Widget _host(AppStateStub app) => Directionality(
      textDirection: TextDirection.ltr,
      child: MediaQuery(
        data: const MediaQueryData(size: Size(1200, 700)),
        child: ThemeScope(
          theme: LumitTheme.dark(),
          animationLevel: AnimationLevel.none,
          showTooltips: false,
          child: TimelinePanel(app: app),
        ),
      ),
    );

void main() {
  group('graph_maths — ease shapes', () {
    test('small_e hits its endpoints and matches the ported forms', () {
      for (final e in GraphEase.values) {
        expect(smallE(e, 0.0), closeTo(0.0, 1e-12), reason: '$e at 0');
        expect(smallE(e, 1.0), closeTo(1.0, 1e-12), reason: '$e at 1');
      }
      expect(smallE(GraphEase.linear, 0.5), closeTo(0.5, 1e-12));
      expect(smallE(GraphEase.slow, 0.5), closeTo(0.25, 1e-12));
      expect(smallE(GraphEase.fast, 0.5), closeTo(0.75, 1e-12));
      // Smooth and Sharp cross 0.5 at the midpoint (S-curve symmetry).
      expect(smallE(GraphEase.smooth, 0.5), closeTo(0.5, 1e-12));
      expect(smallE(GraphEase.sharp, 0.5), closeTo(0.5, 1e-12));
    });

    test('preset labels are the exact strings the op takes', () {
      expect(presetLabelFor(GraphEase.linear), 'Lin');
      expect(presetLabelFor(GraphEase.smooth), 'Smth');
      expect(presetLabelFor(GraphEase.sharp), 'Shrp');
      expect(presetLabels, ['Lin', 'Slow', 'Fast', 'Smth', 'Shrp']);
    });

    test('reverse gate floors negative speeds only when reverse is off', () {
      expect(clampedSpeeds(-0.5, 2.0, false), (0.0, 2.0));
      expect(clampedSpeeds(-0.5, 2.0, true), (-0.5, 2.0));
    });
  });

  group('graph_maths — sampling', () {
    test('a rate ramp samples from its start speed to its end speed', () {
      final rt = _heroRetime();
      final samples = sampleSpeedCurve(rt, perSegment: 10);
      expect(samples, isNotEmpty);
      // The very first point is v0 = 100%.
      expect(samples.first.pct, closeTo(100.0, 1e-9));
      expect(samples.first.frame, closeTo(0.0, 1e-9));
      // The last point is the hold at 200%, at the last boundary frame.
      expect(samples.last.pct, closeTo(200.0, 1e-9));
      expect(samples.last.frame, closeTo(120.0, 1e-9));
      // Linear ramp midpoint (frame 30) sits at 150%.
      final mid = samples.firstWhere((s) => (s.frame - 30).abs() < 1e-6);
      expect(mid.pct, closeTo(150.0, 1e-9));
    });

    test('a structurally broken store samples to nothing', () {
      const broken = BridgeRetime(
        reverse: false,
        interpolation: 'nearest',
        boundaries: [
          BridgeRetimeBoundary(
              tFrame: 0, tSeconds: 0, sSeconds: 0, smooth: false),
        ],
        segments: [BridgeRetimeSegment(kind: 'rate', v0: 1, v1: 1)],
      );
      expect(sampleSpeedCurve(broken), isEmpty);
    });

    test('speed range always frames 0% and 100%', () {
      final rt = _heroRetime();
      final (lo, hi) = speedRange(sampleSpeedCurve(rt));
      expect(lo, lessThanOrEqualTo(0.0));
      expect(hi, greaterThanOrEqualTo(200.0));
    });

    test('a map segment draws its derived speed', () {
      // A single map segment whose source advances 2s over 1s of local time at
      // constant 1/3 handles: the derived speed is a smooth hump, positive
      // throughout, peaking above the 200% chord average.
      const map = BridgeRetime(
        reverse: false,
        interpolation: 'nearest',
        boundaries: [
          BridgeRetimeBoundary(
              tFrame: 0, tSeconds: 0, sSeconds: 0, smooth: false),
          BridgeRetimeBoundary(
              tFrame: 60, tSeconds: 1, sSeconds: 2, smooth: false),
        ],
        segments: [
          BridgeRetimeSegment(
              kind: 'map', m0: 0.0, m1: 0.0, b0: 1 / 3, b1: 1 / 3),
        ],
      );
      final s = sampleSpeedCurve(map, perSegment: 20);
      expect(s, isNotEmpty);
      // Ends have zero tangent (m0 = m1 = 0), the middle is fast.
      expect(s.first.pct, closeTo(0.0, 1e-6));
      expect(s.last.pct, closeTo(0.0, 1e-6));
      final peak = s.map((e) => e.pct).reduce((a, b) => a > b ? a : b);
      expect(peak, greaterThan(200.0));
    });
  });

  group('graph_maths — segments and boundaries', () {
    test('segmentIndexAtFrame locates the segment under a frame', () {
      final rt = _heroRetime();
      expect(segmentIndexAtFrame(rt, 0), 0);
      expect(segmentIndexAtFrame(rt, 30), 0);
      expect(segmentIndexAtFrame(rt, 59), 0);
      expect(segmentIndexAtFrame(rt, 60), 1);
      expect(segmentIndexAtFrame(rt, 120), 1);
      expect(segmentIndexAtFrame(rt, -1), isNull);
      expect(segmentIndexAtFrame(rt, 200), isNull);
    });

    test('speedPctAtFrame reads the profile at the playhead', () {
      final rt = _heroRetime();
      expect(speedPctAtFrame(rt, 0), closeTo(100.0, 1e-9));
      expect(speedPctAtFrame(rt, 30), closeTo(150.0, 1e-9));
      expect(speedPctAtFrame(rt, 90), closeTo(200.0, 1e-9));
    });

    test('only interior boundaries are draggable', () {
      final rt = _heroRetime();
      expect(draggableBoundaryIndices(rt), [1]);
    });

    test('boundaryAtX grabs the interior boundary near the pointer', () {
      final rt = _heroRetime();
      // A trivial linear map: frame f -> x = f.
      double xOf(num f) => f.toDouble();
      expect(boundaryAtX(rt, 60, xOf), 1); // right on boundary 1 (frame 60)
      expect(boundaryAtX(rt, 63, xOf), 1); // within 6 px
      expect(boundaryAtX(rt, 80, xOf), isNull); // too far
      expect(boundaryAtX(rt, 0, xOf), isNull); // endpoint isn't draggable
    });

    test('a dragged boundary clamps between its neighbours', () {
      final rt = _heroRetime();
      expect(clampBoundaryFrame(rt, 1, 30), 30); // in range
      expect(clampBoundaryFrame(rt, 1, -5), 1); // not before boundary 0 + 1
      expect(clampBoundaryFrame(rt, 1, 999), 119); // not past boundary 2 - 1
    });

    test('withBoundaryFrame moves only the chosen join', () {
      final rt = _heroRetime();
      final moved = withBoundaryFrame(rt, 1, 30);
      expect(moved.boundaries[0].tFrame, 0);
      expect(moved.boundaries[1].tFrame, 30);
      expect(moved.boundaries[2].tFrame, 120);
      // Speeds are untouched (the join only slides on x).
      expect(moved.segments[0].v1, rt.segments[0].v1);
    });
  });

  group('GraphEditor widget', () {
    // A wide surface so the header's ramp presets + →Rate sit on-screen (they
    // scroll only on a genuinely narrow panel); reset after each test.
    Future<void> wide(WidgetTester tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
    }

    testWidgets('the graph appears only with the lens on and a footage selection',
        (tester) async {
      await wide(tester);
      final app = AppStateStub(bridge: _GraphFake());
      await tester.pumpWidget(_host(app));
      await tester.pump();

      // Lens off: no graph editor, the lane rows show instead.
      expect(find.byType(GraphEditor), findsNothing);

      // Lens on but nothing selected: the graph shows its select hint.
      app.toggleGraphMode();
      await tester.pump();
      expect(find.byType(GraphEditor), findsOneWidget);
      expect(find.text('Select a layer to edit its curves.'), findsOneWidget);

      // Select the retimed footage layer: the header (with →Rate) appears.
      app.selectLayer('hero');
      await tester.pump();
      expect(find.text('→Rate'), findsOneWidget);
      expect(find.text('Ramp'), findsOneWidget);

      // Select the un-retimed footage layer: the enable hint instead.
      app.selectLayer('flat');
      await tester.pump();
      expect(find.textContaining('no Retime'), findsOneWidget);
    });

    testWidgets('a preset click stamps the segment under the playhead',
        (tester) async {
      await wide(tester);
      final fake = _GraphFake();
      final app = AppStateStub(bridge: fake)
        ..toggleGraphMode()
        ..selectLayer('hero')
        ..goToFrame(90); // in the second segment
      await tester.pumpWidget(_host(app));
      await tester.pump();

      await tester.tap(find.text('Smth'));
      await tester.pump();
      // The op carries the playhead frame (the bridge resolves the segment).
      expect(fake.ops, contains('preset:hero@90=Smth'));
    });

    testWidgets('→Rate surfaces the conversion notice', (tester) async {
      await wide(tester);
      final fake = _GraphFake();
      final app = AppStateStub(bridge: fake)
        ..toggleGraphMode()
        ..selectLayer('hero')
        ..goToFrame(30);
      await tester.pumpWidget(_host(app));
      await tester.pump();

      await tester.tap(find.text('→Rate'));
      await tester.pump();
      expect(fake.ops, contains('to_rate:hero@30'));
      expect(app.notice, isNotNull);
      expect(app.notice, contains('rate'));
    });

    testWidgets('a boundary drag commits dragBoundary on release',
        (tester) async {
      await wide(tester);
      final fake = _GraphFake();
      final app = AppStateStub(bridge: fake)
        ..toggleGraphMode()
        ..selectLayer('hero');
      await tester.pumpWidget(_host(app));
      await tester.pump();

      // The interior boundary (index 1) sits at frame 60. At zoom 1 the whole
      // 120-frame comp spans the lane, so find its on-screen x through the
      // GraphEditor's own scale and drag it left by ~15 frames.
      final editor =
          tester.state(find.byType(GraphEditor)) as dynamic;
      final scale = editor.widget.scale;
      final startX = scale.xOfFrame(60) as double;
      final targetX = scale.xOfFrame(45) as double;
      final gd = find.byType(GraphEditor);
      final topLeft = tester.getTopLeft(gd);
      // Drag along a y inside the plot (below the 26 px header).
      final y = topLeft.dy + 120;
      final gesture = await tester.startGesture(Offset(startX, y));
      await tester.pump();
      await gesture.moveTo(Offset(targetX, y));
      await tester.pump();
      await gesture.up();
      await tester.pump();

      expect(
        fake.ops.any((o) => o.startsWith('drag_boundary:hero/1@')),
        isTrue,
        reason: 'ops were: ${fake.ops}',
      );
    });
  });
}
