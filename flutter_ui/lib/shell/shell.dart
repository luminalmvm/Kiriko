// The application shell: menu bar, docked panels, status line, modals and
// the keyboard shortcut routing — the Flutter counterpart of shell/mod.rs +
// app_update.rs + shortcuts.rs.
//
// Structure note: the ThemeScope sits ABOVE the app's one Overlay so that
// popups inserted into the Overlay (menus, dropdowns, tooltips) still read
// the theme; the shell body is its own StatefulWidget *inside* the overlay's
// initial entry, because an OverlayEntry's builder does not re-run when an
// ancestor's setState fires — the body must own its modal state itself.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../bridge/bridge.dart';
import '../panels/panels.dart';
import '../panels/preview_isolate.dart';
import '../popout/desktop_window_opener.dart';
import '../popout/popout_arguments.dart';
import '../popout/popout_windows.dart';
import '../state/app_state.dart';
import '../state/dock.dart';
import '../state/workspace.dart';
import '../widgets/controls.dart';
import 'command_palette.dart';
import 'dock_widget.dart';
import 'export_dialog.dart';
import 'menu_bar.dart';
import 'recovery_dialog.dart';
import 'settings_window.dart';
import 'splash.dart';

class LumitShell extends StatelessWidget {
  final Workspace workspace;

  /// The engine bridge, when the `lumit_bridge` library loaded (null = the F0
  /// placeholder build). Threaded down to the shell body's [AppStateStub].
  final LumitBridge? bridge;

  /// Opens panels in their own OS windows. Defaults to the real
  /// `desktop_multi_window`-backed opener; tests inject a fake so no real
  /// window is ever spawned.
  final PopoutWindows? popoutWindows;

  const LumitShell({
    super.key,
    required this.workspace,
    this.bridge,
    this.popoutWindows,
  });

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: workspace,
      builder: (context, _) => ThemeScope(
        theme: workspace.theme,
        animationLevel: workspace.animationLevel,
        showTooltips: workspace.interface.showTooltips,
        child: Overlay(
          initialEntries: [
            OverlayEntry(
              builder: (context) => _ShellBody(
                workspace: workspace,
                bridge: bridge,
                popoutWindows: popoutWindows,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ShellBody extends StatefulWidget {
  final Workspace workspace;
  final LumitBridge? bridge;
  final PopoutWindows? popoutWindows;
  const _ShellBody({
    required this.workspace,
    this.bridge,
    this.popoutWindows,
  });

  @override
  State<_ShellBody> createState() => _ShellBodyState();
}

class _ShellBodyState extends State<_ShellBody> {
  late final AppStateStub app = AppStateStub(
    bridge: widget.bridge,
    // The perf pass: with a real engine library the Viewer renders on a worker
    // isolate (K-176). A null result (no library, or spawn refused) keeps the
    // inline renderer, so the placeholder build and tests are unaffected.
    previewRendererFactory: IsolateFrameRenderer.tryCreate,
    lastProjectPath: widget.workspace.lastProjectPath,
    rememberProject: widget.workspace.rememberProject,
    rememberSession: widget.workspace.rememberSession,
    sessionFor: widget.workspace.sessionFor,
    autosaveInterval:
        Duration(minutes: widget.workspace.autosave.intervalMins),
    autosaveKeep: widget.workspace.autosave.keep,
  );

  @override
  void initState() {
    super.initState();
    app.addListener(_manageExportPoll);
    // Autosave only means anything with an engine to save through — and the
    // guard keeps bridge-less widget tests free of pending timers.
    if (widget.bridge != null) app.startAutosave();
  }
  bool settingsOpen = false;
  bool paletteOpen = false;
  bool splashDone = false;
  final ValueNotifier<Panel?> activePanel = ValueNotifier(null);
  final FocusNode _rootFocus = FocusNode(debugLabel: 'lumit-shell');

  /// Opens popped-out panel windows. The real opener until a test injects a
  /// fake; constructing it touches no plugin (only [PopoutWindows.open] does),
  /// so bridge-less widget tests are unaffected.
  late final PopoutWindows _popoutWindows =
      widget.popoutWindows ?? DesktopWindowOpener();

  /// Panels currently floating in their own window. While floating, the panel
  /// keeps its dock slot but shows a placeholder (egui's "hidden while
  /// floating"); closing the window re-docks it (the placeholder is replaced by
  /// the real body again). Held here, not in the dock tree, so re-dock is
  /// lossless.
  final Set<Panel> _floating = {};

  /// The engine's boot log, pulled once at start-up (empty without a bridge, so
  /// the splash falls back to its canned lines) — streamed by the splash card.
  late final List<String> _bootLog = app.bootLog();

  /// The ~4 Hz export-progress poll: alive only while an export runs, driven by
  /// app-state notifications (a running export → start the timer; idle → stop).
  Timer? _exportPoll;

  Workspace get ws => widget.workspace;

  /// Keep the poll timer's lifetime tied to whether an export is running.
  void _manageExportPoll() {
    if (app.exportRunning && _exportPoll == null) {
      _exportPoll = Timer.periodic(
          const Duration(milliseconds: 250), (_) => app.exportPollTick());
    } else if (!app.exportRunning && _exportPoll != null) {
      _exportPoll!.cancel();
      _exportPoll = null;
    }
  }

  /// Open the export dialogue (bridge present) or keep the F0 notice — shared by
  /// the menu bar and the command palette's Export command.
  void _openExport() {
    if (app.bridge == null) {
      app.engine('Export comp');
      return;
    }
    showExportDialog(context, app,
        preset: ws.export.defaultPreset,
        template: ws.export.filenameTemplate ?? '');
  }

  /// Pop a panel into its own OS window (multi-window, same process — the
  /// popout's engine shares this process's `lumit_bridge.dll`, so it edits the
  /// same document). The Viewer and Timeline stay in-window and are never
  /// offered ([canPopOutPanel]). On success the panel floats (its dock slot
  /// shows a placeholder) until its window closes, when it re-docks. If the
  /// window cannot open (no multi-window at runtime), the panel stays docked
  /// under a calm notice.
  Future<void> _onPopOut(Panel panel) async {
    if (!canPopOutPanel(panel) || _floating.contains(panel)) return;
    if (app.bridge == null) {
      app.setNotice('${panel.title}: pop out needs the engine');
      return;
    }
    final accent = ws.accentOverride;
    final args = PopoutArguments(
      panel: panel,
      scheme: ws.colorScheme,
      shape: ws.themeShape,
      accentArgb: accent == null ? null : PopoutArguments.packAccent(accent),
      animationLevel: ws.animationLevel,
      showTooltips: ws.interface.showTooltips,
      uiScale: ws.interface.uiScale,
      projectPath: app.snapshot?.path ?? ws.lastProjectPath,
      frontCompId: app.frontCompIdResolved,
      selectedLayer: app.selectedLayer,
    );
    final opened = await _popoutWindows.open(
      args,
      onClosed: () {
        if (!mounted) return;
        setState(() => _floating.remove(panel));
      },
    );
    if (!mounted) return;
    if (opened) {
      setState(() => _floating.add(panel));
    } else {
      app.setNotice('${panel.title}: multi-window is unavailable on this build');
    }
  }

  @override
  void dispose() {
    _exportPoll?.cancel();
    app.removeListener(_manageExportPoll);
    activePanel.dispose();
    _rootFocus.dispose();
    super.dispose();
  }

  /// The global shortcut set (docs/flutter-port/02 §5), with the "never
  /// steal typing" gate: if the focused node is an editable text, stand down.
  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final focused = FocusManager.instance.primaryFocus;
    if (focused != null && focused.context?.widget is EditableText) {
      return KeyEventResult.ignored;
    }
    if (settingsOpen || paletteOpen) {
      if (event.logicalKey == LogicalKeyboardKey.escape) {
        setState(() {
          settingsOpen = false;
          paletteOpen = false;
        });
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }

    final pressed = HardwareKeyboard.instance;
    final ctrl = pressed.isControlPressed || pressed.isMetaPressed;
    final shift = pressed.isShiftPressed;
    final alt = pressed.isAltPressed;
    final key = event.logicalKey;

    bool handled = true;
    if (ctrl && shift && key == LogicalKeyboardKey.keyZ) {
      app.redo();
    } else if (ctrl && key == LogicalKeyboardKey.keyZ) {
      app.undo();
    } else if (ctrl && key == LogicalKeyboardKey.keyS) {
      app.save();
    } else if (ctrl && key == LogicalKeyboardKey.comma) {
      setState(() => settingsOpen = true);
    } else if (ctrl && shift && key == LogicalKeyboardKey.keyP) {
      setState(() => paletteOpen = true);
    } else if (ctrl && key == LogicalKeyboardKey.keyD) {
      final compId = app.frontCompIdResolved;
      final layerId = app.selectedLayer;
      if (compId != null && layerId != null) {
        app.duplicateLayer(compId, layerId);
      }
    } else if (ctrl && key == LogicalKeyboardKey.keyC) {
      // Copy the selected Timeline keyframes (egui note 2.2 / UI-7). A quiet
      // no-op when the Timeline is unmounted or nothing is selected — the text
      // focus gate above already stands this down while a field has focus.
      app.copySelectedKeyframes();
    } else if (ctrl && key == LogicalKeyboardKey.keyV) {
      // Paste the copied keyframes at the playhead (the sibling of Ctrl+C).
      app.pasteKeyframes();
    } else if (shift && key == LogicalKeyboardKey.f3) {
      app.toggleGraphMode();
    } else if (key == LogicalKeyboardKey.space) {
      app.togglePlay();
    } else if (key == LogicalKeyboardKey.keyK) {
      if (app.playing) app.togglePlay();
    } else if (key == LogicalKeyboardKey.keyL) {
      if (!app.playing) app.togglePlay();
    } else if (key == LogicalKeyboardKey.keyJ ||
        key == LogicalKeyboardKey.arrowLeft) {
      app.stepFrame(-1);
    } else if (key == LogicalKeyboardKey.arrowRight) {
      app.stepFrame(1);
    } else if (key == LogicalKeyboardKey.home) {
      app.goToFrame(0);
    } else if (key == LogicalKeyboardKey.end) {
      app.goToFrame(app.previewFrameCount);
    } else if (key == LogicalKeyboardKey.keyB) {
      app.workAreaInAtPlayhead();
    } else if (key == LogicalKeyboardKey.keyN) {
      app.workAreaOutAtPlayhead();
    } else if (key == LogicalKeyboardKey.delete ||
        key == LogicalKeyboardKey.backspace) {
      // Selected lane keyframes are handled inside the timeline's own focus
      // scope; here the selected layer goes (the egui fallback order).
      final compId = app.frontCompIdResolved;
      final layerId = app.selectedLayer;
      if (compId != null && layerId != null) {
        app.deleteLayer(compId, layerId);
      } else {
        handled = false;
      }
    } else if (key == LogicalKeyboardKey.equal ||
        key == LogicalKeyboardKey.add) {
      app.zoomTimeline(1.4);
    } else if (key == LogicalKeyboardKey.minus) {
      app.zoomTimeline(1 / 1.4);
    } else if (key == LogicalKeyboardKey.backslash) {
      app.zoomTimelineFit();
    } else if (key == LogicalKeyboardKey.bracketLeft) {
      final compId = app.frontCompIdResolved;
      final layerId = app.selectedLayer;
      if (compId != null && layerId != null) {
        app.editLayerSpan(
            compId, layerId, alt ? 'trim_in' : 'move_in', app.previewFrame);
      } else {
        handled = false;
      }
    } else if (key == LogicalKeyboardKey.bracketRight) {
      final compId = app.frontCompIdResolved;
      final layerId = app.selectedLayer;
      if (compId != null && layerId != null) {
        app.editLayerSpan(
            compId, layerId, alt ? 'trim_out' : 'move_out', app.previewFrame);
      } else {
        handled = false;
      }
    } else if (event.character == '*') {
      // Layout-independent, like the egui text-event read.
      final compId = app.frontCompIdResolved;
      if (compId != null) {
        app.addMarker(compId, app.previewFrame);
      } else {
        handled = false;
      }
    } else {
      handled = false;
    }
    return handled ? KeyEventResult.handled : KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    return Focus(
      focusNode: _rootFocus,
      autofocus: true,
      onKeyEvent: _onKey,
      child: Stack(
        children: [
          Column(
            children: [
              LumitMenuBar(
                app: app,
                workspace: ws,
                onOpenSettings: () => setState(() => settingsOpen = true),
                onOpenPalette: () => setState(() => paletteOpen = true),
              ),
              Expanded(
                child: DockWidget(
                  root: ws.dock,
                  // A floating panel keeps its dock slot but shows a
                  // placeholder; its real body lives in the popped-out window.
                  buildPanel: (context, panel) => _floating.contains(panel)
                      ? _FloatingPanelSlot(panel: panel)
                      : buildPanelBody(context, panel, app),
                  onLayoutChanged: ws.save,
                  activePanel: activePanel,
                  // Pop-out opens the panel in its own OS window (multi-window,
                  // same process → shared engine document). Offered only for the
                  // read-mostly panels a second engine can host honestly; the
                  // Viewer and Timeline stay in-window (see popout_arguments.dart
                  // and docs/flutter-port/05).
                  onPopOut: _onPopOut,
                  canPopOut: (panel) =>
                      canPopOutPanel(panel) && !_floating.contains(panel),
                ),
              ),
              _StatusLine(app: app),
            ],
          ),
          if (settingsOpen)
            SettingsWindow(
              workspace: ws,
              app: app,
              onClose: () => setState(() => settingsOpen = false),
            ),
          if (paletteOpen)
            CommandPalette(
              commands: paletteCommands(
                app: app,
                workspace: ws,
                openSettings: () => setState(() {
                  paletteOpen = false;
                  settingsOpen = true;
                }),
                openExport: _openExport,
              ),
              onClose: () => setState(() => paletteOpen = false),
            ),
          if (!splashDone)
            Positioned.fill(
              child: SplashOverlay(
                lines: _bootLog,
                onDone: () {
                  setState(() => splashDone = true);
                  // Once the shell is up, offer recovery if the last project's
                  // autosaves look newer than its save (see recovery_dialog).
                  WidgetsBinding.instance.addPostFrameCallback((_) {
                    if (!mounted || widget.bridge == null) return;
                    maybeShowRecovery(context, app,
                        projectPath: ws.lastProjectPath);
                  });
                },
              ),
            ),
        ],
      ),
    );
  }
}

/// What a panel's dock slot shows while it is floating in its own window: a
/// calm placeholder naming the panel, so the layout is preserved and closing
/// the window restores the real body in place.
class _FloatingPanelSlot extends StatelessWidget {
  final Panel panel;
  const _FloatingPanelSlot({required this.panel});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Center(
      child: Text(
        '${panel.title} is open in its own window',
        style: t.small.copyWith(color: t.textMuted),
      ),
    );
  }
}

/// The status line: quiet notices left, genuine errors in the error tint
/// (docs/15 §10), the export-progress slot right.
class _StatusLine extends StatelessWidget {
  final AppStateStub app;
  const _StatusLine({required this.app});

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return ListenableBuilder(
      listenable: app,
      builder: (context, _) => Container(
        height: 22,
        color: t.surface2,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        child: Row(
          children: [
            if (app.errorNotice != null)
              Text(app.errorNotice!, style: t.small.copyWith(color: t.error))
            else if (app.notice != null)
              Text(app.notice!, style: t.small),
            if (app.exportStatusText != null) ...[
              const SizedBox(width: 12),
              Text(app.exportStatusText!,
                  style: t.small.copyWith(color: t.accent)),
              const SizedBox(width: 6),
              LumitTooltip(
                message: 'Cancel export',
                child: HouseButton(
                  small: true,
                  onPressed: app.cancelExport,
                  child: const Text('×'),
                ),
              ),
            ],
            const Spacer(),
            Text('Flutter frontend — phase F0', style: t.small),
          ],
        ),
      ),
    );
  }
}
