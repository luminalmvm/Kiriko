// The state object a popped-out panel renders over.
//
// In plain terms: the panels are written against [AppStateStub], so a popout
// needs one. But a popout's engine is a *second* engine in the same process —
// it holds its OWN AppStateStub, driven by its OWN bridge handle that opens the
// same `lumit_bridge.dll`. Because the DLL is loaded once per process, that
// handle reaches the very same document behind the engine's process-wide mutex
// (see bridge.dart's note on the render isolate for the same fact). So a
// popout's ops land in the shared journal, and its snapshot reads the shared
// document.
//
// The one thing an AppStateStub cannot do out of the box is pick up an *external*
// edit: it refreshes its held snapshot only after its own ops (there is no
// public "re-pull from the engine" — the main window relies on its own
// interactions). A popout, though, must notice edits the MAIN window made. This
// subclass adds exactly that — a public [resync] that re-pulls the shared
// snapshot and adopts it — built only from AppStateStub's public surface
// (`snapshot`, `documentEpoch`, `canUndo`/`canRedo`, `previewFrameCount` are
// public fields; `notifyListeners` is reachable from a subclass), so it needs
// no change to the app-state file it extends.

import '../bridge/bridge.dart';
import '../state/app_state.dart';

class PopoutAppState extends AppStateStub {
  // A popout never restores the last project (the engine already holds the
  // document) and never runs autosave — those are the main window's job.
  // Passing no lastProjectPath skips the restore; autosave only starts on an
  // explicit startAutosave() the popout never calls.
  PopoutAppState({required DocumentBridge bridge})
      : super(bridge: bridge, lastProjectPath: null);

  /// Re-pull the shared document snapshot and adopt it, returning true when the
  /// document had advanced (a new epoch) so the caller can keep its own change
  /// bookkeeping. A no-op with no bridge or on a failed/empty read (the last
  /// good snapshot stays). Mirrors the public-visible half of the private
  /// `_adoptSnapshot`: the cache-bar warm-set reset it also does is a
  /// Viewer/Timeline concern those panels stay in-window, so it is not needed
  /// here.
  bool resync() {
    final b = bridge;
    if (b == null) return false;
    final reply = b.snapshot();
    if (!reply.ok) return false;
    final snap = reply.snapshot;
    if (snap == null) return false;
    if (_sameDocument(snapshot, snap)) return false;
    snapshot = snap;
    canUndo = snap.canUndo;
    canRedo = snap.canRedo;
    documentEpoch++;
    final fc = frontComp;
    if (fc != null) previewFrameCount = fc.frameCount;
    notifyListeners();
    return true;
  }

  /// A cheap structural equality between the held and freshly-pulled snapshot,
  /// so an unchanged poll does not thrash the epoch (which would needlessly
  /// re-decode thumbnails and repaint scopes). Compares the fields a popout
  /// panel actually reflects; a false negative only costs one extra repaint.
  static bool _sameDocument(BridgeSnapshot? a, BridgeSnapshot b) {
    if (a == null) return false;
    if (a.canUndo != b.canUndo || a.canRedo != b.canRedo) return false;
    if (a.path != b.path) return false;
    return _itemsEqual(a.items, b.items);
  }

  static bool _itemsEqual(List<BridgeItem> a, List<BridgeItem> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (!_itemEqual(a[i], b[i])) return false;
    }
    return true;
  }

  static bool _itemEqual(BridgeItem a, BridgeItem b) {
    if (a.id != b.id || a.name != b.name || a.kind != b.kind) return false;
    if (a.status != b.status) return false;
    final ca = a.comp, cb = b.comp;
    if ((ca == null) != (cb == null)) return false;
    if (ca != null && cb != null) {
      // Layer count/order and each layer's identity+span is the cheapest proxy
      // for "the comp changed" that catches the edits a popout reflects.
      if (ca.layers.length != cb.layers.length) return false;
      if (ca.frameCount != cb.frameCount) return false;
      if (ca.markers.length != cb.markers.length) return false;
      for (var i = 0; i < ca.layers.length; i++) {
        final la = ca.layers[i], lb = cb.layers[i];
        if (la.id != lb.id ||
            la.name != lb.name ||
            la.index != lb.index ||
            la.inFrame != lb.inFrame ||
            la.outFrame != lb.outFrame ||
            la.effects.length != lb.effects.length) {
          return false;
        }
      }
    }
    return _itemsEqual(a.children, b.children);
  }
}
