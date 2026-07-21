// The last-columns (blend / matte / parent) pickers, Add-mask, the composition
// -settings commit, per-project session restore, and autosave. Widget + unit
// tests over a recording fake DocumentBridge (no library, no plugin channels).

import 'dart:io';

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lumit_flutter/bridge/bridge.dart';
import 'package:lumit_flutter/panels/timeline/columns.dart';
import 'package:lumit_flutter/panels/timeline_panel.dart';
import 'package:lumit_flutter/shell/dialogs.dart';
import 'package:lumit_flutter/state/app_state.dart';
import 'package:lumit_flutter/state/workspace.dart';
import 'package:lumit_flutter/theme/theme.dart';
import 'package:lumit_flutter/widgets/controls.dart';

/// A fake bridge with a front comp "Scene" (a footage layer `l0` and a solid
/// `l1`), recording the ops the tests assert. Its document [path] is mutable so
/// open/save flows can be exercised.
class _FakeBridge implements DocumentBridge {
  final List<String> ops = [];
  String? path;

  _FakeBridge({this.path});

  String _pathJson() =>
      path == null ? 'null' : '"${path!.replaceAll(r'\', r'\\')}"';

  String _json() => '''
  {
    "ok": true,
    "items": [
      {
        "id": "c1", "name": "Scene", "kind": "composition", "children": [],
        "comp": {
          "width": 1920, "height": 1080, "fps": {"num": 60, "den": 1},
          "frame_count": 300,
          "motion_blur": {"enabled": false, "shutter_angle": 180,
            "shutter_phase": 0, "samples": 16},
          "layers": [
            {"id":"l0","index":0,"name":"top","kind":"footage",
             "in_frame":0,"out_frame":300,"label":0,"switches":{},
             "blend_mode":"Normal"},
            {"id":"l1","index":1,"name":"under","kind":"solid",
             "in_frame":0,"out_frame":300,"label":0,"switches":{}}
          ],
          "markers": []
        }
      }
    ],
    "can_undo": true, "can_redo": false, "path": ${_pathJson()}
  }''';

  BridgeReply _snap() => BridgeReply.parse(_json());

  @override
  BridgeReply snapshot() => _snap();
  @override
  BridgeReply newProject() {
    path = null;
    return _snap();
  }

  @override
  BridgeReply undo() => _snap();
  @override
  BridgeReply redo() => _snap();
  @override
  BridgeReply openProject(String p) {
    path = p;
    ops.add('open:$p');
    return _snap();
  }

  @override
  BridgeReply saveProject(String p) {
    ops.add('save:$p');
    if (p.isNotEmpty) path = p;
    return _snap();
  }

  @override
  BridgeReply newComposition(String name) {
    ops.add('newComp:$name');
    return _snap();
  }

  @override
  BridgeReply importFootage(String p) => _snap();
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
      int fpsNum, int fpsDen, int durationFrames) {
    ops.add(
        'compSettings:$compId/$name/$width/$height/$fpsNum:$fpsDen/$durationFrames');
    return _snap();
  }

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
  List<BridgeBlendMode> listBlendModes() => const [
        BridgeBlendMode(name: 'Normal', label: 'Normal'),
        BridgeBlendMode(name: 'Multiply', label: 'Multiply'),
        BridgeBlendMode(name: 'Screen', label: 'Screen'),
      ];
  @override
  BridgeReply setBlendMode(String compId, String layerId, String mode) {
    ops.add('blend:$compId/$layerId=$mode');
    return _snap();
  }

  @override
  BridgeReply setMatte(String compId, String layerId, String source,
      String channel, bool inverted) {
    ops.add('matte:$compId/$layerId=$source/$channel/$inverted');
    return _snap();
  }

  @override
  BridgeReply setParent(String compId, String layerId, String parent) {
    ops.add('parent:$compId/$layerId=$parent');
    return _snap();
  }

  @override
  BridgeReply setMotionBlur(String compId, bool enabled, double shutterAngle,
      double shutterPhase, int samples) {
    ops.add('mb:$compId=$enabled/$shutterAngle/$shutterPhase/$samples');
    return _snap();
  }

  @override
  BridgeReply addMask(String compId, String layerId, String kind) {
    ops.add('mask:$compId/$layerId=$kind');
    return _snap();
  }

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

Widget _host(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: MediaQuery(
        data: const MediaQueryData(size: Size(900, 700)),
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

/// A tap target that opens [onOpen] with a real context under the Overlay.
Widget _opener(void Function(BuildContext) onOpen) => Builder(
      builder: (context) => GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => onOpen(context),
        child: const SizedBox(width: 200, height: 80),
      ),
    );

BridgeLayer _layer(AppStateStub app, String id) =>
    app.frontComp!.layers.firstWhere((l) => l.id == id);

void main() {
  group('Last columns pickers', () {
    testWidgets('the blend picker commits setBlendMode', (tester) async {
      await tester.binding.setSurfaceSize(const Size(900, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_host(_opener((context) => showBlendModePicker(
            context: context,
            app: app,
            compId: 'c1',
            layer: _layer(app, 'l0'),
            position: const Offset(20, 20),
          ))));
      await tester.tap(find.byType(GestureDetector));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Multiply'));
      await tester.pumpAndSettle();
      expect(fake.ops, contains('blend:c1/l0=Multiply'));
    });

    testWidgets('the matte picker points a layer at another', (tester) async {
      await tester.binding.setSurfaceSize(const Size(900, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_host(_opener((context) => showMattePicker(
            context: context,
            app: app,
            compId: 'c1',
            layer: _layer(app, 'l0'),
            position: const Offset(20, 20),
          ))));
      await tester.tap(find.byType(GestureDetector));
      await tester.pumpAndSettle();
      // None + the one other layer are offered.
      expect(find.text('None'), findsOneWidget);
      await tester.tap(find.text('under'));
      await tester.pumpAndSettle();
      expect(fake.ops, contains('matte:c1/l0=l1/alpha/false'));
    });

    testWidgets('the parent picker points a layer at another', (tester) async {
      await tester.binding.setSurfaceSize(const Size(900, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_host(_opener((context) => showParentPicker(
            context: context,
            app: app,
            compId: 'c1',
            layer: _layer(app, 'l0'),
            position: const Offset(20, 20),
          ))));
      await tester.tap(find.byType(GestureDetector));
      await tester.pumpAndSettle();
      await tester.tap(find.text('under'));
      await tester.pumpAndSettle();
      expect(fake.ops, contains('parent:c1/l0=l1'));
    });

    test('parentingWouldCycle rejects self and descendants', () {
      // b is parented to a; parenting a to b would cycle.
      const layers = [
        BridgeLayer(
            id: 'a',
            index: 0,
            name: 'a',
            kind: BridgeLayerKind.solid,
            inFrame: 0,
            outFrame: 1,
            label: 0,
            switches: BridgeSwitches(
                visible: true,
                audible: true,
                locked: false,
                threeD: false,
                collapse: false,
                fx: true,
                solo: false,
                motionBlur: false)),
        BridgeLayer(
            id: 'b',
            index: 1,
            name: 'b',
            kind: BridgeLayerKind.solid,
            inFrame: 0,
            outFrame: 1,
            label: 0,
            parent: 'a',
            switches: BridgeSwitches(
                visible: true,
                audible: true,
                locked: false,
                threeD: false,
                collapse: false,
                fx: true,
                solo: false,
                motionBlur: false)),
      ];
      expect(parentingWouldCycle(layers, 'a', 'a'), isTrue);
      expect(parentingWouldCycle(layers, 'a', 'b'), isTrue);
      expect(parentingWouldCycle(layers, 'b', 'a'), isFalse);
    });
  });

  group('Layer context menu wiring', () {
    testWidgets('Blend mode opens the picker and commits', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake)..frontCompSelect('c1');
      await tester.pumpWidget(_host(TimelinePanel(app: app)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('top'), buttons: kSecondaryButton);
      await tester.pumpAndSettle();
      expect(find.text('Blend mode'), findsOneWidget);
      await tester.tap(find.text('Blend mode'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Screen'));
      await tester.pumpAndSettle();
      expect(fake.ops, contains('blend:c1/l0=Screen'));
    });

    testWidgets('Add mask offers the shapes and commits one', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake)..frontCompSelect('c1');
      await tester.pumpWidget(_host(TimelinePanel(app: app)));
      await tester.pumpAndSettle();
      await tester.tap(find.text('top'), buttons: kSecondaryButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add mask'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Star'));
      await tester.pumpAndSettle();
      expect(fake.ops, contains('mask:c1/l0=star'));
    });
  });

  group('Motion-blur master', () {
    testWidgets('the bottom-bar toggle flips the comp master', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake)..frontCompSelect('c1');
      await tester.pumpWidget(_host(TimelinePanel(app: app)));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('mb-master')));
      await tester.pumpAndSettle();
      // The comp master read-back is off (180°/0°/16), so the toggle enables it.
      expect(fake.ops, contains('mb:c1=true/180.0/0.0/16'));
    });
  });

  group('Add mask', () {
    test('addMaskToSelected adds to the selected layer', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake)
        ..frontCompSelect('c1')
        ..selectLayer('l0');
      app.addMaskToSelected('ellipse');
      expect(fake.ops, contains('mask:c1/l0=ellipse'));
    });

    test('addMaskToSelected with no selection is a quiet error', () {
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake);
      app.selectLayer(null);
      app.addMaskToSelected('rectangle');
      expect(fake.ops.where((o) => o.startsWith('mask:')), isEmpty);
      expect(app.errorNotice, isNotNull);
    });
  });

  group('Composition settings', () {
    testWidgets('Apply commits setCompSettings with the dialogue fields',
        (tester) async {
      await tester.binding.setSurfaceSize(const Size(900, 700));
      final fake = _FakeBridge();
      final app = AppStateStub(bridge: fake)..frontCompSelect('c1');
      await tester.pumpWidget(_host(_opener(
          (context) => showCompositionSettingsDialog(context, app))));
      await tester.tap(find.byType(GestureDetector));
      await tester.pumpAndSettle();

      // Rename, then Apply.
      await tester.enterText(find.byType(EditableText), 'Intro');
      await tester.pump();
      await tester.tap(find.text('Apply'));
      await tester.pumpAndSettle();

      // The comp id, the new name and the fields seeded from the comp:
      // 1920×1080 @ 60/1, and its 300 frames (5 s) round-tripped back.
      expect(
        fake.ops,
        contains('compSettings:c1/Intro/1920/1080/60:1/300'),
      );
    });

    test('fpsRational maps the NTSC presets to 1001 rationals', () {
      expect(fpsRational(23.976), (24000, 1001));
      expect(fpsRational(29.97), (30000, 1001));
      expect(fpsRational(59.94), (60000, 1001));
      expect(fpsRational(60), (60, 1));
      expect(fpsRational(25), (25, 1));
    });
  });

  group('Per-project session', () {
    test('round-trips through the workspace JSON', () {
      final ws = Workspace();
      const session = SavedSession(
          openComps: ['c1'], activeComp: 'c1', frame: 42, selectedLayer: 'l0');
      ws.rememberSession('p.lum', session);
      // Re-decode into a fresh workspace to prove the persistence shape.
      final reloaded = Workspace()..applyJson(ws.toJson());
      expect(reloaded.sessionFor('p.lum'), session);
      expect(reloaded.sessionFor('other.lum'), isNull);
    });

    test('is re-applied after a project opens', () async {
      const session = SavedSession(
          openComps: ['c1'], activeComp: 'c1', frame: 42, selectedLayer: 'l0');
      final sessions = {'p.lum': session};
      final fake = _FakeBridge();
      final app = AppStateStub(
        bridge: fake,
        openProjectPicker: () async => 'p.lum',
        sessionFor: (p) => sessions[p],
      );
      await app.openProject();
      expect(app.frontCompId, 'c1');
      expect(app.previewFrame, 42);
      expect(app.selectedLayer, 'l0');
      expect(app.openComps, ['c1']);
    });

    test('a stale selection falls back rather than crashing', () async {
      const session = SavedSession(
          openComps: ['gone'],
          activeComp: 'missing',
          frame: 10,
          selectedLayer: 'ghost');
      final fake = _FakeBridge();
      final app = AppStateStub(
        bridge: fake,
        openProjectPicker: () async => 'p.lum',
        sessionFor: (_) => session,
      );
      await app.openProject();
      // The comp/selection ids are gone, so they fall back to the defaults.
      expect(app.selectedLayer, isNull);
      expect(app.openComps, isEmpty);
      // The playhead still restores, clamped into the real range.
      expect(app.previewFrame, 10);
    });

    test('edits persist the session through rememberSession', () {
      final saved = <String, SavedSession>{};
      final fake = _FakeBridge(path: 'p.lum');
      final app = AppStateStub(
        bridge: fake,
        rememberSession: (path, s) => saved[path] = s,
      )..frontCompSelect('c1');
      app.selectLayer('l0');
      app.goToFrame(17);
      expect(saved['p.lum']?.selectedLayer, 'l0');
      expect(saved['p.lum']?.frame, 17);
      expect(saved['p.lum']?.activeComp, 'c1');
    });
  });

  group('Autosave scheme', () {
    test('slot names mirror lumit_project::autosave', () {
      final sep = Platform.pathSeparator;
      final project = 'C:${sep}proj${sep}edit.lum';
      expect(AutosaveScheme.stem(project), 'edit');
      expect(AutosaveScheme.dir(project), 'C:${sep}proj${sep}autosaves');
      expect(AutosaveScheme.slot(project, 1),
          'C:${sep}proj${sep}autosaves${sep}edit.autosave-1.lum');
      expect(AutosaveScheme.slot(project, 3),
          'C:${sep}proj${sep}autosaves${sep}edit.autosave-3.lum');
    });

    test('rotation shifts copies up and keeps N', () {
      final tmp = Directory.systemTemp.createTempSync('lumit_autosave_');
      try {
        final sep = Platform.pathSeparator;
        final project = '${tmp.path}${sep}edit.lum';
        File(project).writeAsStringSync('main');

        // First rotation: slot 1 free, nothing to shift.
        final first = AutosaveScheme.rotateAndNewestSlot(project, 3);
        expect(first, AutosaveScheme.slot(project, 1));
        File(first).writeAsStringSync('v1');

        // Second rotation: v1 shifts 1 → 2, slot 1 free again.
        final second = AutosaveScheme.rotateAndNewestSlot(project, 3);
        File(second).writeAsStringSync('v2');
        expect(File(AutosaveScheme.slot(project, 2)).readAsStringSync(), 'v1');
        expect(File(AutosaveScheme.slot(project, 1)).readAsStringSync(), 'v2');

        // Third and fourth push the oldest off the end (keep = 3).
        File(AutosaveScheme.rotateAndNewestSlot(project, 3))
            .writeAsStringSync('v3');
        File(AutosaveScheme.rotateAndNewestSlot(project, 3))
            .writeAsStringSync('v4');
        expect(File(AutosaveScheme.slot(project, 1)).readAsStringSync(), 'v4');
        expect(File(AutosaveScheme.slot(project, 2)).readAsStringSync(), 'v3');
        expect(File(AutosaveScheme.slot(project, 3)).readAsStringSync(), 'v2');
        // Only three copies survive; slot 4 never exists.
        expect(File(AutosaveScheme.slot(project, 4)).existsSync(), isFalse);
        // The main project file is never touched.
        expect(File(project).readAsStringSync(), 'main');
      } finally {
        tmp.deleteSync(recursive: true);
      }
    });

    test('autosaveTick writes a rotating copy only when due and dirty', () {
      final tmp = Directory.systemTemp.createTempSync('lumit_autosave_');
      try {
        final sep = Platform.pathSeparator;
        final project = '${tmp.path}${sep}edit.lum';
        File(project).writeAsStringSync('main');

        // The autosave baseline is set to now() at construction, so the test
        // clock is anchored just before it.
        final start = DateTime.now();
        final fake = _FakeBridge(path: project);
        final app = AppStateStub(bridge: fake)
          ..autosaveInterval = const Duration(minutes: 5)
          ..autosaveKeep = 3;
        // Adopt the document (path set) via the initial snapshot.
        expect(app.snapshot?.path, project);

        // Not dirty yet: no write even when the interval has passed.
        expect(
            app.autosaveTick(start.add(const Duration(minutes: 10))), isFalse);

        // Dirty it through a real op, then a due tick writes a rotating copy.
        app.setBlendMode('c1', 'l0', 'Multiply');
        expect(app.autosaveTick(start.add(const Duration(minutes: 10))), isTrue,
            reason: 'first due tick after dirtying writes a copy');
        expect(fake.ops, contains('save:${AutosaveScheme.slot(project, 1)}'));
        // The dirty gate cleared, so an immediate second tick does nothing.
        expect(
            app.autosaveTick(start.add(const Duration(minutes: 30))), isFalse);
        // The main file is untouched by autosave.
        expect(File(project).readAsStringSync(), 'main');
        app.dispose();
      } finally {
        tmp.deleteSync(recursive: true);
      }
    });
  });
}
