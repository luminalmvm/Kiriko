// The root widget a popped-out panel window runs.
//
// In plain terms: this is the whole contents of a popout window. It rebuilds
// the theme from the arguments (so the panel looks identical to the main
// window), hosts exactly one panel over a [PopoutAppState], and keeps that
// state fresh by re-pulling the shared document snapshot on a modest cadence
// (~2 Hz) — that is how an edit the MAIN window made shows up here. The
// panel's own edits push straight through the bridge into the shared journal
// and self-refresh, exactly as they do in the main window.
//
// It depends only on the app-state and panel code (no plugin import), so it is
// widget-testable with a fake bridge and never needs a real window.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../bridge/bridge.dart';
import '../panels/panels.dart';
import '../widgets/controls.dart';
import '../widgets/ui_scale.dart';
import 'popout_app_state.dart';
import 'popout_arguments.dart';

class PopoutHost extends StatefulWidget {
  final PopoutArguments args;

  /// The popout's own bridge handle (its engine's `LumitBridge.tryLoad()`).
  /// Null degrades to a calm notice — a popout with no engine has nothing to
  /// show, but it must never crash.
  final DocumentBridge? bridge;

  /// How often to re-pull the shared snapshot so external (main-window) edits
  /// appear. The spec's ~2 Hz; overridable for tests.
  final Duration pollInterval;

  const PopoutHost({
    super.key,
    required this.args,
    required this.bridge,
    this.pollInterval = const Duration(milliseconds: 500),
  });

  @override
  State<PopoutHost> createState() => _PopoutHostState();
}

class _PopoutHostState extends State<PopoutHost> {
  PopoutAppState? _app;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    final bridge = widget.bridge;
    if (bridge == null) return;
    final app = PopoutAppState(bridge: bridge);
    // Open focused on what the main window had fronted/selected: the popout's
    // selection is its own (a second engine), seeded once from the arguments.
    if (widget.args.frontCompId != null) {
      app.frontCompId = widget.args.frontCompId;
    }
    if (widget.args.selectedLayer != null) {
      app.selectLayer(widget.args.selectedLayer);
    }
    _app = app;
    _poll = Timer.periodic(widget.pollInterval, (_) => app.resync());
  }

  @override
  void dispose() {
    _poll?.cancel();
    _app?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final args = widget.args;
    final theme = args.theme;
    final app = _app;
    return Directionality(
      textDirection: TextDirection.ltr,
      child: ColoredBox(
        color: theme.surface0,
        child: UiScaleView(
          scale: args.uiScale,
          // ThemeScope above the one Overlay, exactly as the main shell nests
          // them (shell.dart): popups the panel inserts into the Overlay (menus,
          // dropdowns, tooltips) still read the theme.
          child: ThemeScope(
            theme: theme,
            animationLevel: args.animationLevel,
            showTooltips: args.showTooltips,
            child: Overlay(
              initialEntries: [
                OverlayEntry(
                  builder: (context) => app == null
                      ? _NoEngineNotice(title: args.panel.title)
                      : _PopoutBody(app: app, args: args),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The panel body under a slim identity header, so the window reads as "this
/// panel, popped out" and the pane rebuilds when the popout state notifies.
class _PopoutBody extends StatelessWidget {
  final PopoutAppState app;
  final PopoutArguments args;
  const _PopoutBody({required this.app, required this.args});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Column(
      children: [
        Container(
          height: 26,
          color: t.surface2,
          alignment: Alignment.centerLeft,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: Text(args.panel.title, style: t.small),
        ),
        Expanded(
          child: ColoredBox(
            color: t.surface1,
            child: ListenableBuilder(
              listenable: app,
              builder: (context, _) => buildPanelBody(context, args.panel, app),
            ),
          ),
        ),
      ],
    );
  }
}

class _NoEngineNotice extends StatelessWidget {
  final String title;
  const _NoEngineNotice({required this.title});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Center(
      child: Text('$title — no engine', style: t.body),
    );
  }
}
