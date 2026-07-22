// Pop-out panels (multi-window) — everything behind seams, no real window ever
// spawned. Covers: the arguments/theme snapshot round-trip and panel gating;
// the popout state's external-edit resync (poll adoption) and op push; the
// PopoutHost rendering + poll cadence + clean disposal; the window-opener seam
// contract (open + close→re-dock callback); and the dock's pop-out-offer gating
// (the Viewer/Timeline never offer it).

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lumit_flutter/bridge/bridge.dart';
import 'package:lumit_flutter/popout/popout_app_state.dart';
import 'package:lumit_flutter/popout/popout_arguments.dart';
import 'package:lumit_flutter/popout/popout_host.dart';
import 'package:lumit_flutter/popout/popout_windows.dart';
import 'package:lumit_flutter/shell/dock_widget.dart';
import 'package:lumit_flutter/state/dock.dart';
import 'package:lumit_flutter/theme/theme.dart';
import 'package:lumit_flutter/widgets/controls.dart';

/// A fake document bridge with a MUTABLE snapshot, so a test can simulate an
/// external (main-window) edit by changing [json] and letting the popout poll
/// pick it up. Records the ops routed through it, to prove a popout pushes edits
/// into the same bridge.
class _FakeBridge implements DocumentBridge {
  String json;
  final List<String> ops = [];
  _FakeBridge(this.json);

  BridgeReply _reply() => BridgeReply.parse(json);

  @override
  BridgeReply snapshot() => _reply();

  @override
  BridgeReply undo() {
    ops.add('undo');
    return _reply();
  }

  @override
  BridgeReply addMarker(String compId, int frame) {
    ops.add('addMarker:$compId:$frame');
    return _reply();
  }

  // Registry reads a panel might make degrade to empty rather than a wrong type.
  @override
  List<BridgeEffectInfo> listEffects() => const [];
  @override
  List<BridgeBlendMode> listBlendModes() => const [];

  @override
  dynamic noSuchMethod(Invocation invocation) => _reply();
}

String _sceneJson(String compName) => '''
{
  "ok": true,
  "can_undo": true,
  "can_redo": false,
  "path": "/tmp/demo.lumit",
  "items": [
    {
      "id": "c1", "name": "$compName", "kind": "composition", "children": [],
      "comp": {
        "width": 1920, "height": 1080, "fps": {"num": 60, "den": 1},
        "frame_count": 300,
        "layers": [
          {"id":"l1","index":0,"name":"Solid","kind":"solid",
           "in_frame":0,"out_frame":300,"label":0,"switches":{},
           "colour":[0.2,0.4,0.6,1.0]}
        ],
        "markers": []
      }
    }
  ]
}
''';

void main() {
  group('PopoutArguments', () {
    test('round-trips through the argument string', () {
      const args = PopoutArguments(
        panel: Panel.effectControls,
        scheme: LumitColorScheme.gruvboxDark,
        shape: ThemeShape.round,
        accentArgb: 0xff123456,
        animationLevel: AnimationLevel.minimal,
        showTooltips: false,
        uiScale: 1.25,
        projectPath: '/p/x.lumit',
        frontCompId: 'c1',
        selectedLayer: 'l9',
      );
      final back = PopoutArguments.tryParse(args.toArguments())!;
      expect(back.panel, Panel.effectControls);
      expect(back.scheme, LumitColorScheme.gruvboxDark);
      expect(back.shape, ThemeShape.round);
      expect(back.accentArgb, 0xff123456);
      expect(back.animationLevel, AnimationLevel.minimal);
      expect(back.showTooltips, false);
      expect(back.uiScale, 1.25);
      expect(back.projectPath, '/p/x.lumit');
      expect(back.frontCompId, 'c1');
      expect(back.selectedLayer, 'l9');
    });

    test('isPopout is false for the main window (empty arguments)', () {
      expect(PopoutArguments.isPopout(''), isFalse);
      expect(PopoutArguments.tryParse(''), isNull);
    });

    test('a non-popout / malformed argument string parses to null', () {
      expect(PopoutArguments.isPopout('{"businessId":"other"}'), isFalse);
      expect(PopoutArguments.tryParse('{"businessId":"other"}'), isNull);
      expect(PopoutArguments.tryParse('not json'), isNull);
    });

    test('an unknown panel name degrades to null, never a crash', () {
      const raw =
          '{"businessId":"popout_panel","panel":"holodeck","scheme":"dark","shape":"sharp"}';
      expect(PopoutArguments.isPopout(raw), isTrue);
      expect(PopoutArguments.tryParse(raw), isNull);
    });

    test('theme rebuilds to the scheme + shape (+ accent) it carried', () {
      const args = PopoutArguments(
        panel: Panel.project,
        scheme: LumitColorScheme.dark,
        shape: ThemeShape.round,
      );
      final expected = LumitTheme.forScheme(LumitColorScheme.dark, ThemeShape.round);
      expect(args.theme.shape, ThemeShape.round);
      expect(args.theme.surface0, expected.surface0);
      expect(args.accentOverride, isNull);
    });

    test('packAccent yields an opaque ARGB int the accent reconstructs from', () {
      final packed = PopoutArguments.packAccent(const Color(0xFF3366CC));
      final args = PopoutArguments(
        panel: Panel.project,
        scheme: LumitColorScheme.dark,
        shape: ThemeShape.sharp,
        accentArgb: packed,
      );
      final c = args.accentOverride!;
      expect((c.r * 255).round(), 0x33);
      expect((c.g * 255).round(), 0x66);
      expect((c.b * 255).round(), 0xCC);
    });

    test('only read-mostly panels are hostable; Viewer and Timeline are not', () {
      expect(canPopOutPanel(Panel.project), isTrue);
      expect(canPopOutPanel(Panel.hierarchy), isTrue);
      expect(canPopOutPanel(Panel.effectControls), isTrue);
      expect(canPopOutPanel(Panel.effectsAndPresets), isTrue);
      expect(canPopOutPanel(Panel.scopes), isTrue);
      expect(canPopOutPanel(Panel.viewer), isFalse);
      expect(canPopOutPanel(Panel.timeline), isFalse);
    });
  });

  group('PopoutAppState', () {
    test('adopts the shared snapshot at construction', () {
      final app = PopoutAppState(bridge: _FakeBridge(_sceneJson('Scene')));
      expect(app.snapshot!.items.first.name, 'Scene');
      addTearDown(app.dispose);
    });

    test('resync picks up an external edit and notifies once', () {
      final fake = _FakeBridge(_sceneJson('Scene'));
      final app = PopoutAppState(bridge: fake);
      addTearDown(app.dispose);
      var notified = 0;
      app.addListener(() => notified++);
      final epoch0 = app.documentEpoch;

      // The main window renamed the comp; the popout's poll re-pulls it.
      fake.json = _sceneJson('Scene renamed');
      final changed = app.resync();

      expect(changed, isTrue);
      expect(app.snapshot!.items.first.name, 'Scene renamed');
      expect(app.documentEpoch, greaterThan(epoch0));
      expect(notified, 1);
    });

    test('resync on an unchanged document does not thrash (no notify)', () {
      final fake = _FakeBridge(_sceneJson('Scene'));
      final app = PopoutAppState(bridge: fake);
      addTearDown(app.dispose);
      var notified = 0;
      app.addListener(() => notified++);
      final epoch0 = app.documentEpoch;

      expect(app.resync(), isFalse);
      expect(notified, 0);
      expect(app.documentEpoch, epoch0);
    });

    test('an op pushes through the SAME bridge and self-refreshes', () {
      final fake = _FakeBridge(_sceneJson('Scene'));
      final app = PopoutAppState(bridge: fake);
      addTearDown(app.dispose);
      app.undo();
      expect(fake.ops, contains('undo'));
    });
  });

  group('PopoutHost', () {
    testWidgets('renders its panel over the shared document', (tester) async {
      final fake = _FakeBridge(_sceneJson('Scene'));
      await tester.pumpWidget(PopoutHost(
        args: const PopoutArguments(
          panel: Panel.project,
          scheme: LumitColorScheme.dark,
          shape: ThemeShape.sharp,
        ),
        bridge: fake,
        pollInterval: const Duration(milliseconds: 100),
      ));
      await tester.pump();
      expect(find.text('Scene'), findsWidgets);
      // Unmount so the poll timer is cancelled (no pending timers).
      await tester.pumpWidget(const SizedBox.shrink());
    });

    testWidgets('the poll adopts an external edit', (tester) async {
      final fake = _FakeBridge(_sceneJson('Scene'));
      await tester.pumpWidget(PopoutHost(
        args: const PopoutArguments(
          panel: Panel.project,
          scheme: LumitColorScheme.dark,
          shape: ThemeShape.sharp,
        ),
        bridge: fake,
        pollInterval: const Duration(milliseconds: 100),
      ));
      await tester.pump();
      expect(find.text('Renamed'), findsNothing);

      fake.json = _sceneJson('Renamed');
      await tester.pump(const Duration(milliseconds: 150)); // fire the poll
      await tester.pump();
      expect(find.text('Renamed'), findsWidgets);

      await tester.pumpWidget(const SizedBox.shrink());
    });

    testWidgets('with no engine it shows a calm notice, never crashes',
        (tester) async {
      await tester.pumpWidget(const PopoutHost(
        args: PopoutArguments(
          panel: Panel.scopes,
          scheme: LumitColorScheme.dark,
          shape: ThemeShape.sharp,
        ),
        bridge: null,
      ));
      await tester.pump();
      expect(find.textContaining('no engine'), findsOneWidget);
    });
  });

  group('window-opener seam (fake)', () {
    test('open records the arguments and reports success', () async {
      final windows = FakePopoutWindows();
      const args = PopoutArguments(
        panel: Panel.hierarchy,
        scheme: LumitColorScheme.dark,
        shape: ThemeShape.sharp,
      );
      final ok = await windows.open(args, onClosed: () {});
      expect(ok, isTrue);
      expect(windows.opened.single.panel, Panel.hierarchy);
      expect(windows.isOpen(Panel.hierarchy), isTrue);
    });

    test('close fires the re-dock callback exactly once', () async {
      final windows = FakePopoutWindows();
      var closed = 0;
      await windows.open(
        const PopoutArguments(
          panel: Panel.project,
          scheme: LumitColorScheme.dark,
          shape: ThemeShape.sharp,
        ),
        onClosed: () => closed++,
      );
      windows.fireClose(Panel.project);
      expect(closed, 1);
      expect(windows.isOpen(Panel.project), isFalse);
      windows.fireClose(Panel.project); // already closed → no double fire
      expect(closed, 1);
    });

    test('a failed open reports false so the shell keeps the panel docked',
        () async {
      final windows = FakePopoutWindows()..succeed = false;
      final ok = await windows.open(
        const PopoutArguments(
          panel: Panel.project,
          scheme: LumitColorScheme.dark,
          shape: ThemeShape.sharp,
        ),
        onClosed: () {},
      );
      expect(ok, isFalse);
      expect(windows.opened, isEmpty);
    });
  });

  group('dock pop-out gating', () {
    Widget harness(DockSplit root) => Directionality(
          textDirection: TextDirection.ltr,
          child: ThemeScope(
            theme: LumitTheme.dark(),
            animationLevel: AnimationLevel.none,
            showTooltips: true,
            child: Overlay(
              initialEntries: [
                OverlayEntry(
                  builder: (context) => DockWidget(
                    root: root,
                    buildPanel: (context, panel) => Text(panel.title),
                    onLayoutChanged: () {},
                    activePanel: ValueNotifier<Panel?>(null),
                    onPopOut: (_) {},
                    canPopOut: canPopOutPanel,
                  ),
                ),
              ],
            ),
          ),
        );

    Finder popOutButton() => find.byWidgetPredicate((w) =>
        w is LumitTooltip && w.message == 'Pop out into its own window');

    testWidgets('a tab group whose active panel is hostable offers pop-out',
        (tester) async {
      await tester.pumpWidget(harness(DockSplit(
        DockAxis.horizontal,
        [
          DockTabs([DockPane(Panel.project), DockPane(Panel.timeline)],
              active: 0),
        ],
        [1.0],
      )));
      await tester.pump();
      expect(popOutButton(), findsOneWidget);
    });

    testWidgets('a tab group whose active panel is the Timeline hides it',
        (tester) async {
      await tester.pumpWidget(harness(DockSplit(
        DockAxis.horizontal,
        [
          DockTabs([DockPane(Panel.timeline), DockPane(Panel.project)],
              active: 0),
        ],
        [1.0],
      )));
      await tester.pump();
      expect(popOutButton(), findsNothing);
    });
  });
}
