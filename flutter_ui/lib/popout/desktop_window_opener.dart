// The real [PopoutWindows] implementation over `desktop_multi_window`.
//
// This is the ONE file in the frontend that imports the multi-window plugin.
// It is inherently owner-machine territory: the plugin's native side (the
// Windows `DesktopMultiWindowPlugin`) compiles and runs only in a real
// `flutter build windows`, never in `flutter test`. So nothing here is unit
// tested — the shell depends on the [PopoutWindows] seam and tests inject
// [FakePopoutWindows]. The Dart API used below is pinned to
// desktop_multi_window 0.3.0 (MixinNetwork/flutter-plugins).
//
// How it works: each popped-out panel is a second Flutter engine in the SAME
// OS process (engine-per-window). `WindowController.create` hands the new
// window a single argument string — our serialised [PopoutArguments] — and the
// shared entrypoint (`popout_main.dart`) parses it. Because the process shares
// one `lumit_bridge.dll`, the popout's own bridge handle reaches the same
// document. The plugin has no per-window close callback, so we detect a close
// by diffing the live window set on the global `onWindowsChanged` stream.

import 'dart:async';

import 'package:desktop_multi_window/desktop_multi_window.dart';

import 'popout_arguments.dart';
import 'popout_windows.dart';

class DesktopWindowOpener implements PopoutWindows {
  StreamSubscription<void>? _sub;

  /// windowId → the close callback to fire when that window disappears.
  final Map<String, void Function()> _onClosed = {};

  @override
  Future<bool> open(
    PopoutArguments args, {
    required void Function() onClosed,
  }) async {
    try {
      final controller = await WindowController.create(
        WindowConfiguration(arguments: args.toArguments()),
      );
      _onClosed[controller.windowId] = onClosed;
      _ensureWatching();
      await controller.show();
      return true;
    } catch (_) {
      // MissingPluginException (a build without the native plugin) or any
      // engine refusal: report failure so the shell keeps the panel docked and
      // shows its calm notice.
      return false;
    }
  }

  /// Watch the global window-set stream, firing the close callback for any
  /// tracked window that has vanished. Subscribed lazily, on the first open,
  /// and only once.
  void _ensureWatching() {
    _sub ??= onWindowsChanged.listen((_) => _reconcile());
  }

  Future<void> _reconcile() async {
    if (_onClosed.isEmpty) return;
    final List<WindowController> live;
    try {
      live = await WindowController.getAll();
    } catch (_) {
      return;
    }
    final liveIds = live.map((c) => c.windowId).toSet();
    final gone = _onClosed.keys.where((id) => !liveIds.contains(id)).toList();
    for (final id in gone) {
      _onClosed.remove(id)?.call();
    }
    if (_onClosed.isEmpty) {
      _sub?.cancel();
      _sub = null;
    }
  }
}
