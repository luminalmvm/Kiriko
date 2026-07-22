// The popout entrypoint — the branch the shared `main()` takes when this engine
// is a popped-out panel rather than the main window.
//
// `desktop_multi_window` runs every window through the same Dart `main`; a
// window learns which one it is by asking the plugin for its own arguments
// (`WindowController.fromCurrentEngine()`). The main window's argument string
// is empty; a popout's is the serialised [PopoutArguments]. So `main()` calls
// [maybeRunPopout] first: it returns true (and has already run the popout app)
// when this engine is a popout, and false — leaving `main()` to boot the normal
// shell — for the main window, or on any build without the native plugin
// (the plugin call throws and is swallowed).
//
// This file imports the plugin, so it is owner-machine territory: it runs only
// in a real `flutter build windows`, never in `flutter test`. The testable
// parts are elsewhere — argument parsing in `popout_arguments.dart`, the host
// UI in `popout_host.dart`.

import 'package:desktop_multi_window/desktop_multi_window.dart';
import 'package:flutter/widgets.dart';

import '../bridge/bridge.dart';
import 'popout_arguments.dart';
import 'popout_host.dart';

/// If this engine is a popped-out panel, run its app and return true. Otherwise
/// (the main window, or no multi-window plugin present) return false so the
/// caller boots the normal shell. Never throws.
Future<bool> maybeRunPopout(List<String> args) async {
  String argument;
  try {
    final controller = await WindowController.fromCurrentEngine();
    argument = controller.arguments;
  } catch (_) {
    // No plugin registered (a build without multi-window), or the main window
    // on a host that has no window definition — boot the normal shell.
    return false;
  }
  final parsed = PopoutArguments.tryParse(argument);
  if (parsed == null) return false;
  _runPopout(parsed);
  return true;
}

void _runPopout(PopoutArguments args) {
  // This engine's own bridge handle: same process → same `lumit_bridge.dll` →
  // the same document behind the engine's process-wide mutex.
  final bridge = LumitBridge.tryLoad();
  runApp(PopoutHost(args: args, bridge: bridge));
}
