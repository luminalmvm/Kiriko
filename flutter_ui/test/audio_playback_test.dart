// Audio playback (bridge v0.10, docs/09): the transport drives the engine's
// audio (play/pause/seek with the right start seconds), an edit while audio is
// loaded or playing re-prepares the mix, the Viewer's ticker chases the audio
// clock (frame = clock × fps, looping the work area with a seek on wrap), and
// everything degrades to the wall-clock transport when the capability is
// absent — so every pre-audio fake and test keeps its exact behaviour.

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lumit_flutter/bridge/bridge.dart';
import 'package:lumit_flutter/panels/viewer_panel.dart';
import 'package:lumit_flutter/state/app_state.dart';
import 'package:lumit_flutter/theme/theme.dart';
import 'package:lumit_flutter/widgets/controls.dart';

// --- Fakes -----------------------------------------------------------------

/// A bridge with the audio capability: answers a fixed snapshot, records every
/// audio call, and reports a settable clock. `noSuchMethod` quietly answers
/// the DocumentBridge surface these tests do not exercise.
class _AudioFake implements DocumentBridge, AudioPlaybackBridge {
  final BridgeSnapshot snap;
  _AudioFake(this.snap);

  final List<String> calls = [];
  AudioClock clock = AudioClock.none;

  @override
  BridgeReply snapshot() => BridgeReply.ok(snap);

  @override
  BridgeReply setLayerSwitch(String c, String l, String s, bool v) {
    calls.add('op:switch');
    return BridgeReply.ok(snap);
  }

  @override
  bool get supportsAudioPlayback => true;

  @override
  void audioPrepare(String compId) => calls.add('prepare:$compId');

  @override
  void audioPlay(String compId, double startSeconds) =>
      calls.add('play:$compId@${startSeconds.toStringAsFixed(3)}');

  @override
  void audioPause() => calls.add('pause');

  @override
  void audioSeek(double seconds) =>
      calls.add('seek:${seconds.toStringAsFixed(3)}');

  @override
  void audioStop() => calls.add('stop');

  @override
  AudioClock audioClock() => clock;

  @override
  dynamic noSuchMethod(Invocation invocation) => BridgeReply.ok(snap);
}

/// The same fake WITHOUT the capability flag: proves the gate is the flag, not
/// merely the interface.
class _UnsupportedAudioFake extends _AudioFake {
  _UnsupportedAudioFake(super.snap);
  @override
  bool get supportsAudioPlayback => false;
}

// --- Fixtures ---------------------------------------------------------------

BridgeSwitches _switches() => const BridgeSwitches(
      visible: true,
      audible: true,
      locked: false,
      threeD: false,
      collapse: false,
      fx: true,
      solo: false,
      motionBlur: false,
    );

BridgeComp _comp({int fps = 24, int frames = 240, List<int>? workArea}) =>
    BridgeComp(
      width: 4,
      height: 4,
      fps: BridgeFps(fps, 1),
      frameCount: frames,
      layers: [
        BridgeLayer(
          id: 'l0',
          index: 0,
          name: 'clip.mp4',
          kind: BridgeLayerKind.footage,
          inFrame: 0,
          outFrame: frames,
          label: 0,
          switches: _switches(),
        ),
      ],
      markers: const [],
      workArea: workArea,
    );

BridgeSnapshot _snapshot({List<BridgeComp>? comps}) => BridgeSnapshot(
      items: [
        for (final (i, comp) in (comps ?? [_comp()]).indexed)
          BridgeItem(
            id: 'c$i',
            name: 'Scene $i',
            kind: BridgeItemKind.composition,
            children: const [],
            comp: comp,
          ),
      ],
      canUndo: false,
      canRedo: false,
      path: null,
    );

Widget _wrap(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: ThemeScope(
        theme: LumitTheme.dark(),
        animationLevel: AnimationLevel.none,
        showTooltips: false,
        child: child,
      ),
    );

void main() {
  group('audioChaseFrame (pure)', () {
    test('the frame follows the clock at the comp rate', () {
      final chase = audioChaseFrame(
          clockSeconds: 2.5, fps: 24, frameCount: 240, workArea: null);
      expect(chase.frame, 60);
      expect(chase.seekFrame, isNull);
    });

    test('the comp end wraps to frame 0 with a seek', () {
      final chase = audioChaseFrame(
          clockSeconds: 10.0, fps: 24, frameCount: 240, workArea: null);
      expect(chase.frame, 0);
      expect(chase.seekFrame, 0, reason: 'the audio rewinds and plays on');
    });

    test('a work area loops within [in, out) and seeks its start', () {
      final inside = audioChaseFrame(
          clockSeconds: 3.0, fps: 24, frameCount: 240, workArea: [48, 96]);
      expect(inside.frame, 72);
      expect(inside.seekFrame, isNull);
      final past = audioChaseFrame(
          clockSeconds: 4.0, fps: 24, frameCount: 240, workArea: [48, 96]);
      expect(past.frame, 48);
      expect(past.seekFrame, 48);
      final before = audioChaseFrame(
          clockSeconds: 0.0, fps: 24, frameCount: 240, workArea: [48, 96]);
      expect(before.frame, 48, reason: 'a clock before the loop snaps in');
      expect(before.seekFrame, 48);
    });

    test('degenerate inputs are calm', () {
      expect(
          audioChaseFrame(clockSeconds: 1.0, fps: 0, frameCount: 240).frame, 0);
      expect(
          audioChaseFrame(clockSeconds: 1.0, fps: 24, frameCount: 0).frame, 0);
    });
  });

  group('transport → audio bridge', () {
    test('play starts the comp audio from the playhead seconds', () {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      app.advancePlayback(48); // frame 48 at 24 fps = 2 s, no audio side effect
      fake.calls.clear();
      app.togglePlay();
      expect(app.playing, isTrue);
      expect(fake.calls, ['play:c0@2.000']);
      app.togglePlay();
      expect(app.playing, isFalse);
      expect(fake.calls, ['play:c0@2.000', 'pause']);
      app.dispose();
    });

    test('a scrub pauses the audio and parks its clock on the frame', () {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      app.togglePlay();
      fake.calls.clear();
      app.goToFrame(120); // 5 s at 24 fps
      expect(app.playing, isFalse);
      expect(fake.calls, ['pause', 'seek:5.000']);
      app.dispose();
    });

    test('stepping a frame pauses and seeks too', () {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      app.advancePlayback(24);
      fake.calls.clear();
      app.stepFrame(1);
      expect(fake.calls, ['pause', 'seek:${(25 / 24).toStringAsFixed(3)}']);
      app.dispose();
    });

    test('an edit while playing re-prepares the comp mix', () {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      app.togglePlay();
      fake.calls.clear();
      app.setLayerSwitch('c0', 'l0', 'audible', false);
      expect(fake.calls, ['op:switch', 'prepare:c0']);
      app.dispose();
    });

    test('an edit while loaded-but-paused re-prepares; unloaded does not', () {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      // Not playing, nothing loaded: an edit must NOT start managing audio.
      app.setLayerSwitch('c0', 'l0', 'audible', false);
      expect(fake.calls, ['op:switch']);
      fake.calls.clear();
      // The engine reports a loaded mix (e.g. paused after playback): an edit
      // re-prepares so the parked mix cannot go stale.
      fake.clock = const AudioClock(seconds: 1, playing: false, loaded: true);
      app.pollAudioClock();
      app.setLayerSwitch('c0', 'l0', 'audible', true);
      expect(fake.calls, ['op:switch', 'prepare:c0']);
      app.dispose();
    });

    test('fronting another comp mid-playback plays its audio', () {
      final fake = _AudioFake(_snapshot(comps: [_comp(), _comp(fps: 30)]));
      final app = AppStateStub(bridge: fake);
      app.advancePlayback(60);
      app.togglePlay();
      fake.calls.clear();
      app.frontCompSelect('c1');
      expect(fake.calls, ['play:c1@2.000'],
          reason: 'frame 60 at the new comp\'s 30 fps is 2 s');
      app.dispose();
    });

    test('without the capability the transport is pure wall clock', () {
      final fake = _UnsupportedAudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      app.togglePlay();
      app.goToFrame(12);
      app.setLayerSwitch('c0', 'l0', 'audible', false);
      expect(fake.calls, ['op:switch'], reason: 'no audio call ever lands');
      expect(app.pollAudioClock(), isNull);
      app.dispose();
    });
  });

  group('Viewer tick chases the audio clock', () {
    testWidgets('the frame follows clock × fps while audio drives',
        (tester) async {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_wrap(ViewerPanel(app: app)));
      fake.clock = const AudioClock(seconds: 0, playing: true, loaded: true);
      app.togglePlay();
      await tester.pump(); // start the ticker
      fake.clock = const AudioClock(seconds: 2, playing: true, loaded: true);
      await tester.pump(const Duration(milliseconds: 16));
      expect(app.previewFrame, 48, reason: '2 s × 24 fps');
      // The clock moves; the picture follows exactly — no wall-clock drift.
      fake.clock = const AudioClock(seconds: 4.5, playing: true, loaded: true);
      await tester.pump(const Duration(milliseconds: 16));
      expect(app.previewFrame, 108);
      app.togglePlay();
      await tester.pumpWidget(const SizedBox());
      app.dispose();
    });

    testWidgets('a clock past the loop end wraps the playhead and re-seeks',
        (tester) async {
      final fake = _AudioFake(_snapshot(comps: [_comp(workArea: [48, 96])]));
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_wrap(ViewerPanel(app: app)));
      app.advancePlayback(90);
      fake.clock = const AudioClock(seconds: 3.75, playing: true, loaded: true);
      app.togglePlay();
      await tester.pump();
      fake.clock = const AudioClock(seconds: 4.1, playing: true, loaded: true);
      await tester.pump(const Duration(milliseconds: 16));
      expect(app.previewFrame, 48, reason: 'wrapped to the work-area start');
      expect(fake.calls, contains('play:c0@2.000'),
          reason: 'the audio is re-seeked to the loop start and plays on');
      app.togglePlay();
      await tester.pumpWidget(const SizedBox());
      app.dispose();
    });

    testWidgets('without a loaded mix the wall clock still advances playback',
        (tester) async {
      final fake = _AudioFake(_snapshot());
      final app = AppStateStub(bridge: fake);
      await tester.pumpWidget(_wrap(ViewerPanel(app: app)));
      // Capability present but nothing loaded (a silent comp / no device).
      fake.clock = AudioClock.none;
      app.togglePlay();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 120));
      await tester.pump(const Duration(milliseconds: 120));
      expect(app.playing, isTrue);
      expect(app.previewFrame, greaterThan(0),
          reason: 'the wall-clock fallback is intact');
      app.togglePlay();
      await tester.pumpWidget(const SizedBox());
      app.dispose();
    });
  });
}
