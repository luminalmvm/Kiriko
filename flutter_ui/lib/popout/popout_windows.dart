// The seam between the shell and real OS windows.
//
// The shell asks a [PopoutWindows] to open a panel in its own window and to
// tell it when that window closes (so the panel re-docks). This interface is
// pure Dart with no plugin import, so the shell wiring and every test depend
// only on it — tests inject [FakePopoutWindows] and never spawn a real window.
// The real implementation lives in `desktop_window_opener.dart`, the only file
// that touches the `desktop_multi_window` plugin.

import '../state/dock.dart';
import 'popout_arguments.dart';

abstract class PopoutWindows {
  /// Open a window hosting [args]'s panel. [onClosed] fires once, when the user
  /// closes that window, so the shell can re-dock the panel. Returns true when
  /// the window opened; false when multi-window is unavailable at runtime (the
  /// plugin missing, an engine that refused) so the caller degrades to a calm
  /// notice and leaves the panel docked.
  Future<bool> open(PopoutArguments args, {required void Function() onClosed});
}

/// A test double: records opened arguments and lets a test fire the close
/// callback for a panel, without any real window.
class FakePopoutWindows implements PopoutWindows {
  final List<PopoutArguments> opened = [];
  final Map<Panel, void Function()> _onClosed = {};

  /// When false, [open] reports failure (the multi-window-unavailable path).
  bool succeed = true;

  @override
  Future<bool> open(
    PopoutArguments args, {
    required void Function() onClosed,
  }) async {
    if (!succeed) return false;
    opened.add(args);
    _onClosed[args.panel] = onClosed;
    return true;
  }

  /// Simulate the user closing the popout window for [panel].
  void fireClose(Panel panel) => _onClosed.remove(panel)?.call();

  /// Whether a window is currently open for [panel].
  bool isOpen(Panel panel) => _onClosed.containsKey(panel);
}
