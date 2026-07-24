// Lumit's Flutter frontend (K-174, the frontend alternative experiment).
// The engine stays in the Rust crates; this application is the chrome —
// see docs/archive/flutter-port/ for the plan and the parity checklist.

import 'package:flutter/widgets.dart';

import 'bridge/bridge.dart';
import 'popout/popout_main.dart';
import 'shell/shell.dart';
import 'state/workspace.dart';
import 'widgets/ui_scale.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  // A popped-out panel runs through this same entrypoint in its own engine
  // (multi-window, same process). If this engine is a popout, it takes over
  // here; otherwise — the main window, or any build without the multi-window
  // plugin — this is a swallowed no-op and the normal shell boots below.
  if (await maybeRunPopout(args)) return;
  final workspace = Workspace()..load();
  // Try the engine bridge; a null result keeps the F0 placeholder behaviour
  // (the app and every test must work without the library present).
  final bridge = LumitBridge.tryLoad();
  runApp(LumitApp(workspace: workspace, bridge: bridge));
}

class LumitApp extends StatelessWidget {
  final Workspace workspace;
  final LumitBridge? bridge;
  const LumitApp({super.key, required this.workspace, this.bridge});

  @override
  Widget build(BuildContext context) {
    // WidgetsApp-level infrastructure only — no Material chrome
    // (docs/archive/flutter-port/04 "Why not Material chrome"). Settings → Interface →
    // UI scale is applied here via [UiScaleView], the Flutter counterpart of
    // egui's `ctx.set_pixels_per_point` — layout and hit-testing scale together
    // (see widgets/ui_scale.dart for why this mechanism, not a devicePixelRatio
    // override). The slider commits on release; this just reflects the value.
    return ListenableBuilder(
      listenable: workspace,
      builder: (context, _) => Directionality(
        textDirection: TextDirection.ltr,
        child: ColoredBox(
          color: workspace.theme.surface0,
          child: UiScaleView(
            scale: workspace.interface.uiScale,
            child: LumitShell(workspace: workspace, bridge: bridge),
          ),
        ),
      ),
    );
  }
}
