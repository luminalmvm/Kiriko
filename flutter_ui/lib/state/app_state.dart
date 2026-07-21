// The Phase-F0 stand-in for the engine-backed application state. In the egui
// frontend this is `AppState` (crates/lumit-ui/src/app_state/), owned by Rust;
// here it is a small ChangeNotifier that answers the chrome's questions and
// records the actions the chrome dispatches, so every menu item, shortcut and
// panel control can be wired now and re-pointed at the bridge in Phase F1
// (docs/flutter-port/03-ARCHITECTURE.md).

import 'dart:async';
import 'dart:collection';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../bridge/bridge.dart';
import '../panels/preview_source.dart';
import 'file_dialogs.dart';
import 'workspace.dart';

/// One pending export in the Dart-side queue — a `VecDeque` mirror of
/// export_actions.rs. The egui side snapshots the whole document at QUEUE time;
/// the bridge can only snapshot at START time, so a Dart queue item carries the
/// call arguments and the document is snapshotted when the export actually
/// begins (a recorded deviation, docs/06 §7.1 and 05-PARITY-CHECKLIST).
class QueuedExport {
  final String compId;
  final String specJson;
  final String outPath;

  /// The file name shown in the status line while this export runs (the path's
  /// last segment).
  final String name;

  const QueuedExport(this.compId, this.specJson, this.outPath, this.name);
}

/// The video bitrate (bits/second) a size-targeted share export uses (K-037) —
/// a faithful port of `Shell::start_share_export`: the byte budget spread over
/// the duration, with the audio track's share removed first and an 8%
/// container/overhead headroom. Pure so it is unit-tested without a bridge.
///
/// [durationSeconds] is the export span in seconds (the work area when set,
/// else the whole comp), floored at 0.1 s exactly as the egui `.max(0.1)`.
/// A leaner 192 kbps AAC rate is subtracted when [hasAudio], because on a share
/// export every audio bit comes out of the same budget. The result is floored
/// at 100 kbps.
int shareExportBitRate({
  required double targetMb,
  required double durationSeconds,
  required bool hasAudio,
}) {
  const audioBitRate = 192000;
  final duration = durationSeconds < 0.1 ? 0.1 : durationSeconds;
  var bits = targetMb * 1000000.0 * 8.0 * 0.92;
  if (hasAudio) bits -= audioBitRate * duration;
  final bitRate = (bits / duration).toInt();
  return bitRate < 100000 ? 100000 : bitRate;
}

/// One entry in the stub's action log — what a real engine call would have
/// been. The status line surfaces the latest as a notice, so clicking through
/// the chrome shows honest feedback about what is and isn't wired yet.
class StubAction {
  final String action;
  final DateTime at;
  StubAction(this.action) : at = DateTime.now();
}

/// One composition as the Timeline's comp-tab strip reads it: its snapshot item
/// [id] (the id ops address), display [name], and its [comp] detail.
class CompTabInfo {
  final String id;
  final String name;
  final BridgeComp comp;
  const CompTabInfo(this.id, this.name, this.comp);
}

class AppStateStub extends ChangeNotifier {
  /// The engine bridge, when the `lumit_bridge` library loaded (injected from
  /// `main.dart` via `LumitBridge.tryLoad()`). Null means the app runs on its
  /// F0 placeholders exactly as before — every path here degrades to the
  /// notice-only behaviour when this is null.
  final DocumentBridge? bridge;

  /// The latest document snapshot from the bridge, or null when there is no
  /// bridge. The Project panel renders this when present.
  BridgeSnapshot? snapshot;

  /// File-dialogue seams, defaulting to the real file_selector calls. Tests
  /// inject their own so they never touch a plugin channel (dialogues cannot
  /// open in a widget test).
  final Future<String?> Function() openProjectPicker;
  final Future<String?> Function() saveLocationPicker;
  final Future<List<String>> Function() footagePicker;

  /// The export Save-location seam (suggested name → chosen path, or null when
  /// cancelled), defaulting to the real file_selector call. Tests inject their
  /// own so the export dialogue and share exports never touch a plugin channel.
  final Future<String?> Function(String suggestedName) exportSaveLocationPicker;

  /// Called with the file a project was opened from or saved to, so the
  /// workspace can restore it next launch (wired to `Workspace.rememberProject`
  /// by the shell; null in tests that do not care).
  final void Function(String path)? rememberProject;

  /// Persist the per-project session (open comps, front comp, playhead,
  /// selection) for a project path — wired to `Workspace.rememberSession` by
  /// the shell; null leaves session state session-only.
  final void Function(String path, SavedSession session)? rememberSession;

  /// Read a stored session for a project path (wired to `Workspace.sessionFor`
  /// by the shell), applied after a project opens. Null means no restore.
  final SavedSession? Function(String path)? sessionFor;

  /// The autosave interval — how often [autosaveTick] writes a rotating copy
  /// when the document is dirty. Defaults to the egui `AUTOSAVE_INTERVAL_SECS`
  /// (5 min). The shell points this at Settings → General.
  Duration autosaveInterval;

  /// How many rotating autosaves to keep (egui `AUTOSAVE_KEEP`); the oldest
  /// falls off. The shell points this at Settings → General.
  int autosaveKeep;

  AppStateStub({
    this.bridge,
    Future<String?> Function()? openProjectPicker,
    Future<String?> Function()? saveLocationPicker,
    Future<List<String>> Function()? footagePicker,
    Future<String?> Function(String suggestedName)? exportSaveLocationPicker,
    this.rememberProject,
    this.rememberSession,
    this.sessionFor,
    this.autosaveInterval = const Duration(minutes: 5),
    this.autosaveKeep = 3,
    String? lastProjectPath,
  })  : openProjectPicker = openProjectPicker ?? pickProjectToOpen,
        saveLocationPicker = saveLocationPicker ?? pickProjectSaveLocation,
        footagePicker = footagePicker ?? pickFootage,
        exportSaveLocationPicker =
            exportSaveLocationPicker ?? pickExportSaveLocation {
    // A live bridge means a live document from the first frame: pull the
    // initial snapshot so the Project panel is populated immediately.
    if (bridge != null) {
      final reply = bridge!.snapshot();
      if (reply.ok) {
        _adoptSnapshot(reply.snapshot);
      } else {
        errorNotice = reply.error;
      }
      _restoreLastProject(lastProjectPath);
    }
  }

  /// On launch with a live bridge, reopen the last project if its file is still
  /// on disk. A missing file is not an error (the project simply moved); a
  /// failed open degrades to a calm status-line notice, never a crash.
  void _restoreLastProject(String? path) {
    if (path == null || bridge == null) return;
    if (!File(path).existsSync()) return;
    final reply = bridge!.openProject(path);
    if (reply.ok) {
      _adoptSnapshot(reply.snapshot);
      _applySessionFor(path);
      notice = 'Project reopened';
    } else {
      notice = 'the last project could not be reopened';
    }
  }

  /// Quiet status-line notice (docs/15 §10 — completion is quiet).
  String? notice;

  /// A genuine error, drawn in the error tint. Kept separate from `notice`
  /// exactly as the Rust side splits them.
  String? errorNotice;

  bool playing = false;
  int previewFrame = 0;
  int previewFrameCount = 0;
  double timelineZoom = 1.0;
  bool timelineGraphMode = false;
  bool snapping = true;

  /// The selected layer, by its snapshot layer id (was an int index in F0; the
  /// Timeline selects by the engine's stable layer id so ops address the right
  /// layer). Null when nothing is selected.
  String? selectedLayer;

  /// Which composition the Timeline/Viewer front, by snapshot item id. Null
  /// means "the first composition in the snapshot" — the [frontComp] fallback.
  String? frontCompId;

  /// Transform values the user has committed this session, keyed
  /// `"$layerId/$property"`. Snapshot v2 does not carry current transform
  /// values (only the setter exists), so the Effect controls panel shows these
  /// — and an em-dash before any edit — until snapshot v3 delivers read-back.
  /// Additive F4 session state.
  final Map<String, double> transformEdits = {};

  /// The value the user set for [layerId]'s [property] this session, or null if
  /// it has not been edited yet (draw an em-dash in that case).
  double? transformEditAt(String layerId, String property) =>
      transformEdits['$layerId/$property'];

  final List<String> openComps = [];
  int beatSensitivity = 50;
  bool canUndo = false;
  bool canRedo = false;

  final List<StubAction> actionLog = [];

  /// Record an engine action the bridge will implement, and say so in the
  /// status line — never silently swallow a click.
  void engine(String action) {
    actionLog.add(StubAction(action));
    notice = '$action — engine bridge arrives in phase F1';
    notifyListeners();
  }

  void togglePlay() {
    playing = !playing;
    notifyListeners();
  }

  void stepFrame(int delta) {
    if (playing) playing = false;
    previewFrame = (previewFrame + delta).clamp(0, previewFrameCount);
    _persistSession();
    notifyListeners();
  }

  void goToFrame(int frame) {
    if (playing) playing = false;
    previewFrame = frame.clamp(0, previewFrameCount);
    _persistSession();
    notifyListeners();
  }

  /// Move the playhead during playback WITHOUT stopping (the Viewer's transport
  /// ticker drives this). Unlike [goToFrame] it leaves `playing` set, so the
  /// loop keeps running. Additive F2 seam.
  void advancePlayback(int frame) {
    previewFrame = frame;
    notifyListeners();
  }

  /// The Viewer's CPU frame source (phase F2), shared with the Scopes panel so
  /// both read the same decoded pixels. Created lazily on first use; harmless
  /// without a bridge (it simply never resolves a frame). Single-layer preview
  /// until the compositor is extracted from the egui crate.
  PreviewSource? _previewSource;
  PreviewSource get previewSource => _previewSource ??= PreviewSource(this);

  void zoomTimeline(double factor) {
    timelineZoom = (timelineZoom * factor).clamp(1.0, 400.0);
    notifyListeners();
  }

  void zoomTimelineFit() {
    timelineZoom = 1.0;
    notifyListeners();
  }

  void toggleGraphMode() {
    timelineGraphMode = !timelineGraphMode;
    notifyListeners();
  }

  void setNotice(String? n) {
    notice = n;
    notifyListeners();
  }

  /// Select a layer by its snapshot layer id (the Hierarchy row click), or null
  /// to clear. The Effect controls panel reads [selectedLayer].
  void selectLayer(String? id) {
    if (selectedLayer == id) return;
    selectedLayer = id;
    _persistSession();
    notifyListeners();
  }

  // --- Bridge-routed document actions -------------------------------------
  //
  // Each mirrors an egui `AppState` action. With no bridge they fall back to
  // the F0 notice, unchanged, so the placeholder build behaves exactly as
  // before. With a bridge they route to the engine, refresh the held snapshot,
  // and surface any error in the error tint.

  void newProject() {
    if (bridge == null) {
      engine('New project');
      return;
    }
    _applyReply(bridge!.newProject(), 'New project');
  }

  /// Create a composition. [name] carries the name typed in the New
  /// composition dialogue (F4); empty lets the engine name it ("Comp N"). Size,
  /// frame rate and duration are not yet wired — the bridge has no
  /// comp-settings op, so the dialogue collects them but only the name reaches
  /// the engine for now (see the dialogue's pending note).
  void newComposition([String name = '']) {
    if (bridge == null) {
      engine('New composition');
      return;
    }
    _applyReply(bridge!.newComposition(name), 'Composition added');
  }

  void undo() {
    if (bridge == null) {
      engine('Undo');
      return;
    }
    _applyReply(bridge!.undo(), 'Undone');
  }

  void redo() {
    if (bridge == null) {
      engine('Redo');
      return;
    }
    _applyReply(bridge!.redo(), 'Redone');
  }

  Future<void> save() async {
    if (bridge == null) {
      engine('Save');
      return;
    }
    // A known path saves in place; without one, Save falls through to a save
    // dialogue — that is the egui behaviour (there is no separate Save As).
    if (snapshot?.path != null) {
      final reply = bridge!.saveProject('');
      _applyReply(reply, 'Project saved');
      if (reply.ok) {
        _dirtySinceSave = false;
        if (snapshot?.path != null) rememberProject?.call(snapshot!.path!);
      }
      return;
    }
    final path = await saveLocationPicker();
    if (path == null) return; // cancelled — leave the status line as-is
    final reply = bridge!.saveProject(path);
    _applyReply(reply, 'Project saved');
    if (reply.ok) {
      _dirtySinceSave = false;
      rememberProject?.call(snapshot?.path ?? path);
    }
  }

  Future<void> openProject() async {
    if (bridge == null) {
      engine('Open project');
      return;
    }
    final path = await openProjectPicker();
    if (path == null) return; // cancelled — leave the status line as-is
    final reply = bridge!.openProject(path);
    _applyReply(reply, 'Project opened');
    if (reply.ok) {
      _dirtySinceSave = false;
      _applySessionFor(path);
      rememberProject?.call(path);
    }
  }

  Future<void> importFootage() async {
    if (bridge == null) {
      engine('Import footage');
      return;
    }
    final paths = await footagePicker();
    if (paths.isEmpty) return; // cancelled or nothing chosen
    var imported = 0;
    var failed = 0;
    String? lastError;
    for (final path in paths) {
      final reply = bridge!.importFootage(path);
      if (reply.ok) {
        _adoptSnapshot(reply.snapshot);
        imported++;
      } else {
        failed++;
        lastError = reply.error;
      }
    }
    _postImportNotice(imported, failed, lastError);
    notifyListeners();
  }

  /// One calm line for an import: the count of items brought in as the notice,
  /// any failures in the error tint (the status line shows the error when both
  /// are set, so a partial failure is never hidden).
  void _postImportNotice(int imported, int failed, String? lastError) {
    notice = imported == 0
        ? null
        : imported == 1
            ? '1 item imported'
            : '$imported items imported';
    errorNotice = failed == 0
        ? null
        : failed == 1
            ? (lastError ?? '1 item could not be imported')
            : '$failed items could not be imported';
  }

  /// Every composition in the current snapshot, top-first (nested comps are
  /// flattened in, after their parent). The Timeline's comp-tab strip renders
  /// this; empty when there is no bridge/snapshot or no composition yet.
  List<CompTabInfo> get compositions {
    final snap = snapshot;
    if (snap == null) return const [];
    final out = <CompTabInfo>[];
    void walk(List<BridgeItem> items) {
      for (final item in items) {
        if (item.kind == BridgeItemKind.composition && item.comp != null) {
          out.add(CompTabInfo(item.id, item.name, item.comp!));
        }
        walk(item.children);
      }
    }

    walk(snap.items);
    return out;
  }

  /// The active comp tab: [frontCompId] when it still resolves, else the first
  /// composition in the snapshot. Null when there is no composition.
  CompTabInfo? get _frontTab {
    final comps = compositions;
    if (comps.isEmpty) return null;
    final id = frontCompId;
    if (id != null) {
      for (final c in comps) {
        if (c.id == id) return c;
      }
    }
    return comps.first;
  }

  /// The active comp the Viewer and Timeline read. Honours [frontCompId] and
  /// falls back to the first composition. Null when there is no composition.
  BridgeComp? get frontComp => _frontTab?.comp;

  /// The snapshot item id of the [frontComp] — the id the Timeline passes to the
  /// layer/marker ops. Null when there is no composition.
  String? get frontCompIdResolved => _frontTab?.id;

  /// Front the composition with snapshot item [id] (a comp-tab click). Also
  /// re-syncs the playhead range to that comp's frame count.
  void frontCompSelect(String id) {
    if (frontCompId == id) return;
    frontCompId = id;
    previewFrameCount = frontComp?.frameCount ?? previewFrameCount;
    previewFrame = previewFrame.clamp(0, previewFrameCount);
    _persistSession();
    notifyListeners();
  }

  // --- Snapshot-v2 op pass-throughs ---------------------------------------
  //
  // The Timeline and editor panels drive these; each routes to the engine,
  // refreshes the held snapshot and surfaces any error in the error tint. With
  // no bridge they are quiet no-ops (the placeholder build has no document).

  /// Flip a layer's switch (`visible`, `audible`, `locked`, `solo`,
  /// `motion_blur`, `fx`, `three_d`, `collapse`).
  void setLayerSwitch(
      String compId, String layerId, String switchName, bool value) {
    final b = bridge;
    if (b == null) return;
    _applyOp(b.setLayerSwitch(compId, layerId, switchName, value));
  }

  /// Edit a layer's span at [frame] (`move_in`, `move_out`, `trim_in`,
  /// `trim_out`).
  void editLayerSpan(String compId, String layerId, String edit, int frame) {
    final b = bridge;
    if (b == null) return;
    _applyOp(b.editLayerSpan(compId, layerId, edit, frame));
  }

  /// Set one transform property to a static [value] (snake_case `TransformProp`
  /// name, e.g. `position_x`, `opacity`).
  void setTransform(
      String compId, String layerId, String property, double value) {
    // Remember the value so the Effect controls panel can show it back (the
    // snapshot does not carry current transform values yet — see [transformEdits]).
    transformEdits['$layerId/$property'] = value;
    final b = bridge;
    if (b == null) {
      notifyListeners();
      return;
    }
    _applyOp(b.setTransform(compId, layerId, property, value));
  }

  /// Drop a user marker on the composition timeline at [frame].
  void addMarker(String compId, int frame) {
    final b = bridge;
    if (b == null) return;
    _applyOp(b.addMarker(compId, frame));
  }

  // --- Bridge v0.3 op pass-throughs ---------------------------------------
  //
  // Each routes to the engine, refreshes the held snapshot and surfaces any
  // error in the error tint. With no bridge they are quiet no-ops.

  /// Add a Solid layer to [compId].
  void addSolidLayer(String compId) => _bridgeOp((b) => b.addSolidLayer(compId));

  /// Add a Text layer to [compId].
  void addTextLayer(String compId) => _bridgeOp((b) => b.addTextLayer(compId));

  /// Add a Camera layer to [compId].
  void addCameraLayer(String compId) =>
      _bridgeOp((b) => b.addCameraLayer(compId));

  /// Add an Adjustment layer to [compId].
  void addAdjustmentLayer(String compId) =>
      _bridgeOp((b) => b.addAdjustmentLayer(compId));

  /// Add an (empty) Sequence layer to [compId].
  void addSequenceLayer(String compId) =>
      _bridgeOp((b) => b.addSequenceLayer(compId));

  /// Delete a layer from its composition.
  void deleteLayer(String compId, String layerId) =>
      _bridgeOp((b) => b.deleteLayer(compId, layerId));

  /// Duplicate a layer (a copy above the original).
  void duplicateLayer(String compId, String layerId) =>
      _bridgeOp((b) => b.duplicateLayer(compId, layerId));

  /// Edit a composition's settings as one undo step.
  void setCompSettings(String compId, String name, int width, int height,
          int fpsNum, int fpsDen, int durationFrames) =>
      _bridgeOp((b) => b.setCompSettings(
          compId, name, width, height, fpsNum, fpsDen, durationFrames));

  /// The stopwatch: toggle a transform property's animation at [frame].
  void togglePropertyAnimated(
          String compId, String layerId, String property, int frame) =>
      _bridgeOp((b) => b.togglePropertyAnimated(compId, layerId, property, frame));

  /// Insert or replace a transform keyframe at [frame] with [value].
  void addKeyframe(
      String compId, String layerId, String property, int frame, double value) {
    transformEdits['$layerId/$property'] = value;
    _bridgeOp((b) => b.addKeyframe(compId, layerId, property, frame, value));
  }

  /// Remove the transform keyframe at [frame].
  void removeKeyframe(String compId, String layerId, String property, int frame) =>
      _bridgeOp((b) => b.removeKeyframe(compId, layerId, property, frame));

  /// Slide the transform keyframes at comp [frames] by [delta] frames.
  void shiftKeyframes(String compId, String layerId, String property,
          List<int> frames, int delta) =>
      _bridgeOp((b) => b.shiftKeyframes(compId, layerId, property, frames, delta));

  /// Set one work-area edge to the playhead [frame] ([isOut] picks the out
  /// edge).
  void setWorkAreaEdge(String compId, int frame, bool isOut) =>
      _bridgeOp((b) => b.setWorkAreaEdge(compId, frame, isOut));

  /// The B key: set the work-area IN edge to the current playhead on the front
  /// comp. A convenience over [setWorkAreaEdge] resolving the comp + playhead,
  /// so the shell's B shortcut drives the real op rather than the F0 notice.
  void workAreaInAtPlayhead() {
    final id = frontCompIdResolved;
    if (id == null) return;
    setWorkAreaEdge(id, previewFrame, false);
  }

  /// The N key: set the work-area OUT edge to the current playhead on the front
  /// comp (the sibling of [workAreaInAtPlayhead]).
  void workAreaOutAtPlayhead() {
    final id = frontCompIdResolved;
    if (id == null) return;
    setWorkAreaEdge(id, previewFrame, true);
  }

  /// The built-in effect registry (empty without a bridge).
  List<BridgeEffectInfo> listEffects() => bridge?.listEffects() ?? const [];

  /// Apply a built-in effect (by its match name) to a layer.
  void addEffect(String compId, String layerId, String effectName) =>
      _bridgeOp((b) => b.addEffect(compId, layerId, effectName));

  /// Remove an effect instance from a layer.
  void removeEffect(String compId, String layerId, String effectId) =>
      _bridgeOp((b) => b.removeEffect(compId, layerId, effectId));

  /// Enable or bypass an effect instance.
  void setEffectEnabled(
          String compId, String layerId, String effectId, bool enabled) =>
      _bridgeOp((b) => b.setEffectEnabled(compId, layerId, effectId, enabled));

  /// Set a scalar (Float) effect parameter to a static [value].
  void setEffectParamScalar(String compId, String layerId, String effectId,
          String paramName, double value) =>
      _bridgeOp((b) =>
          b.setEffectParamScalar(compId, layerId, effectId, paramName, value));

  /// Set a Colour effect parameter to a static scene-linear RGBA.
  void setEffectParamColour(String compId, String layerId, String effectId,
          String paramName, double r, double g, double b, double a) =>
      _bridgeOp((bridge) => bridge.setEffectParamColour(
          compId, layerId, effectId, paramName, r, g, b, a));

  // --- Bridge v0.4 op pass-throughs ---------------------------------------

  /// Set the interpolation of the keyframe nearest [frame] on a transform
  /// [property] (`Hold`/`Linear`/`Bezier`; the speed/influence pairs apply only
  /// to a `Bezier` side).
  void setKeyframeInterp(
          String compId,
          String layerId,
          String property,
          int frame,
          String interpIn,
          String interpOut,
          {double speedIn = 0,
          double influenceIn = 1.0 / 3.0,
          double speedOut = 0,
          double influenceOut = 1.0 / 3.0}) =>
      _bridgeOp((b) => b.setKeyframeInterp(compId, layerId, property, frame,
          interpIn, interpOut, speedIn, influenceIn, speedOut, influenceOut));

  /// Enable or disable a footage layer's Retime (the Time stopwatch).
  void setRetimeEnabled(String compId, String layerId, bool enabled) =>
      _bridgeOp((b) => b.setRetimeEnabled(compId, layerId, enabled));

  /// Set a footage layer's constant playback speed (percent; 100 clears it).
  void setRetimeSpeed(String compId, String layerId, double speedPercent) =>
      _bridgeOp((b) => b.setRetimeSpeed(compId, layerId, speedPercent));

  /// Set the ease of the Retime segment at [frame].
  void setSegmentPreset(String compId, String layerId, int frame, String ease) =>
      _bridgeOp((b) => b.setSegmentPreset(compId, layerId, frame, ease));

  /// Convert the Map segment at [frame] to a Rate segment.
  void segmentToRate(String compId, String layerId, int frame) =>
      _bridgeOp((b) => b.segmentToRate(compId, layerId, frame));

  /// →Rate on the Retime segment under [frame], surfacing the fit outcome as a
  /// calm status notice the way the egui speed lens does (docs/04-RETIMING.md
  /// §5.2). The engine reports a fit `drift` in its reply, but the typed
  /// [BridgeReply]/[BridgeSnapshot] do not carry that field (it is dropped in
  /// the bridge's snapshot decode, out of this slice's scope), so we post the
  /// clean-fit confirmation without the millisecond figure — the numeric drift
  /// badge stays a named remainder in the parity checklist.
  void convertSegmentToRate(String compId, String layerId, int frame) {
    final b = bridge;
    if (b == null) return;
    final reply = b.segmentToRate(compId, layerId, frame);
    if (reply.ok) {
      _adoptSnapshot(reply.snapshot);
      // The egui wording: the fit drift surfaces as a quiet notice.
      final drift = reply.driftSeconds;
      notice = drift == null
          ? 'Converted to rate'
          : 'fitted, ${(drift * 1000).round()} ms drift';
      errorNotice = null;
    } else {
      errorNotice = reply.error;
    }
    notifyListeners();
  }

  /// Move the value-lens Retime boundary at [index] to comp [frame].
  void dragBoundary(String compId, String layerId, int index, int frame) =>
      _bridgeOp((b) => b.dragBoundary(compId, layerId, index, frame));

  /// The blend-mode registry (empty without a bridge).
  List<BridgeBlendMode> listBlendModes() =>
      bridge?.listBlendModes() ?? const [];

  /// Set a layer's blend mode (the serde variant name).
  void setBlendMode(String compId, String layerId, String mode) =>
      _bridgeOp((b) => b.setBlendMode(compId, layerId, mode));

  /// Point a layer at another as its matte, or clear it when [source] is empty.
  void setMatte(String compId, String layerId, String source, String channel,
          bool inverted) =>
      _bridgeOp((b) => b.setMatte(compId, layerId, source, channel, inverted));

  /// Point a layer at another as its transform parent, or clear it when
  /// [parent] is empty.
  void setParent(String compId, String layerId, String parent) =>
      _bridgeOp((b) => b.setParent(compId, layerId, parent));

  /// Set the comp's motion-blur master.
  void setMotionBlur(String compId, bool enabled, double shutterAngle,
          double shutterPhase, int samples) =>
      _bridgeOp((b) =>
          b.setMotionBlur(compId, enabled, shutterAngle, shutterPhase, samples));

  /// Add a starter mask shape (`rectangle`/`ellipse`/`star`) to a layer.
  void addMask(String compId, String layerId, String kind) =>
      _bridgeOp((b) => b.addMask(compId, layerId, kind));

  /// Add a starter mask shape to the currently selected layer of the front
  /// comp (the menu-bar / palette Add-mask path). A quiet error when there is
  /// no selected layer (or no front comp) — never a crash.
  void addMaskToSelected(String kind) {
    final compId = frontCompIdResolved;
    final layerId = selectedLayer;
    if (compId == null || layerId == null) {
      errorNotice = 'select a layer to add a mask';
      notifyListeners();
      return;
    }
    addMask(compId, layerId, kind);
  }

  // --- Bridge v0.4 export -------------------------------------------------

  /// Resolve a delivery [presetName] into the dialogue fields it stamps plus its
  /// suggested file name (the default fields without a bridge).
  BridgeExportPreset exportPreset(
          String presetName, String compName, String template) =>
      bridge?.exportPreset(presetName, compName, template) ??
      BridgeExportPreset.idle;

  /// Start an export of [compId] to [outPath] with the dialogue-shaped
  /// [specJson]. Returns the reply so the UI can queue on
  /// "an export is already running"; without a bridge it is a quiet no-op that
  /// reports failure. Does not refresh the snapshot (an export mutates nothing).
  BridgeReply startExport(String compId, String specJson, String outPath) {
    final b = bridge;
    if (b == null) {
      return const BridgeReply.err('no engine library');
    }
    final reply = b.startExport(compId, specJson, outPath);
    if (!reply.ok) errorNotice = reply.error;
    notifyListeners();
    return reply;
  }

  /// Poll the running export — the seam a UI timer drives (this state owns no
  /// timer of its own). Returns the idle state without a bridge.
  BridgeExportState pollExport() => bridge?.exportPoll() ?? BridgeExportState.idle;

  /// Ask the running export to cancel.
  void cancelExport() {
    bridge?.exportCancel();
    notifyListeners();
  }

  // --- Export queue + live progress (F4, export_actions.rs + app_update.rs) --
  //
  // A Dart-side one-at-a-time queue: confirming an export while one runs
  // enqueues it, and each completion (done/failed) starts the next. A shell
  // Timer drives [exportPollTick] at ~4 Hz while one runs; the status line
  // reads [exportStatusText]. Everything degrades to a quiet no-op without a
  // bridge (the menu/palette keep their F0 `engine` notices instead).

  final Queue<QueuedExport> _exportQueue = Queue<QueuedExport>();

  /// The running export's file name (the status-line label), or null when idle.
  String? exportName;

  /// The encoder the ladder settled on, once a running poll reports it — kept
  /// so the quiet completion notice can name it (the `done` poll carries no
  /// encoder, exactly as the egui `export_encoder` outlives the Progress
  /// events).
  String? exportEncoder;

  /// The running export's progress counters (0/0 until the first poll).
  int exportFrame = 0;
  int exportTotal = 0;

  /// Whether an export is in flight (drives the poll timer and the status line).
  bool get exportRunning => exportName != null;

  /// How many exports wait behind the running one.
  int get exportQueueLength => _exportQueue.length;

  /// The status-line export readout while one runs (null when idle) — the exact
  /// wording of app_update.rs: `exporting {name} {frame}/{total}`, with the
  /// encoder and queued-count suffixes.
  String? get exportStatusText {
    final name = exportName;
    if (name == null) return null;
    var line = 'exporting $name $exportFrame/$exportTotal';
    final enc = exportEncoder;
    if (enc != null) line += ' · $enc';
    if (_exportQueue.isNotEmpty) line += ' · ${_exportQueue.length} queued';
    return line;
  }

  /// Queue one export (the dialogue's confirm, or a share export). It starts
  /// immediately when nothing is running; otherwise it waits its turn
  /// (export_actions.rs `enqueue_export` + `try_start_next_export`).
  void queueExport(String compId, String specJson, String outPath) {
    _exportQueue.add(QueuedExport(compId, specJson, outPath, _fileName(outPath)));
    _tryStartNextExport();
  }

  /// Start the next queued export when none is running (a no-op otherwise).
  void _tryStartNextExport() {
    if (exportRunning || _exportQueue.isEmpty) return;
    final next = _exportQueue.first;
    // We only reach here idle, so a failure is a genuine start error (a bad
    // comp, no GPU) rather than "already running"; startExport has set the
    // error tint. Drop the item either way so the queue can never wedge.
    final reply = startExport(next.compId, next.specJson, next.outPath);
    _exportQueue.removeFirst();
    if (reply.ok) {
      exportName = next.name;
      exportEncoder = null;
      exportFrame = 0;
      exportTotal = 0;
      // Deliberately leave `errorNotice` alone: starting the next export must
      // not wipe the tint from an export that just failed (egui's
      // `try_start_next_export` never clears the error).
      notifyListeners();
    }
  }

  /// One poll tick — a shell Timer drives this at ~4 Hz while an export runs.
  /// Reads the bridge's export state, updates the status-line readout, and on a
  /// terminal state posts the quiet completion notice or the error tint with
  /// app_update.rs's exact wording, then starts the next queued export.
  void exportPollTick() {
    if (!exportRunning) return;
    final s = pollExport();
    switch (s.state) {
      case 'running':
        exportFrame = s.frame;
        exportTotal = s.total;
        if (s.encoder != null) exportEncoder = s.encoder;
        notifyListeners();
      case 'done':
        // A completed export is a quiet notice, not an error (docs/15 §10).
        final enc = exportEncoder;
        final withEnc = enc != null ? ' — encoded with $enc' : '';
        notice = 'exported ${s.path ?? ''}$withEnc';
        errorNotice = null;
        _clearExportSession();
        _tryStartNextExport();
        notifyListeners();
      case 'failed':
        errorNotice = 'export: ${s.error ?? 'failed'}';
        _clearExportSession();
        _tryStartNextExport();
        notifyListeners();
      default:
        // 'idle' while we believed one was running — treat as finished quietly.
        _clearExportSession();
        _tryStartNextExport();
        notifyListeners();
    }
  }

  void _clearExportSession() {
    exportName = null;
    exportEncoder = null;
    exportFrame = 0;
    exportTotal = 0;
  }

  /// The last path segment of [path] (its file name), for the status line.
  static String _fileName(String path) {
    final parts = path.split(RegExp(r'[/\\]'));
    for (final part in parts.reversed) {
      if (part.isNotEmpty) return part;
    }
    return 'export';
  }

  /// Start a size-targeted share export (K-037): resolve the front comp, size
  /// the video bitrate to [targetMb] via [shareExportBitRate], ask where to
  /// save, then queue it directly — no settings dialogue, exactly as
  /// `Shell::start_share_export` does. A quiet no-op without a bridge (the menu
  /// keeps its F0 notice for that build).
  Future<void> startShareExport(double targetMb) async {
    if (bridge == null) return;
    final comp = frontComp;
    final compId = frontCompIdResolved;
    if (comp == null || compId == null) {
      errorNotice = 'select a composition to export';
      notifyListeners();
      return;
    }
    final bitRate = shareExportBitRate(
      targetMb: targetMb,
      durationSeconds: _compDurationSeconds(comp),
      hasAudio: _compHasAudio(comp),
    );
    final suggested = 'share-${targetMb.toInt()}mb.mp4';
    final path = await exportSaveLocationPicker(suggested);
    if (path == null) return; // cancelled — leave the status line as-is
    // The bridge's spec resolver takes Mbps (blank = default quality); a share
    // export always pins an explicit bitrate. The comp's own size and the
    // leaner share AAC rate ride the spec too.
    final specJson = jsonEncode({
      'preset': 'custom',
      'codec': 'h264',
      'size': [comp.width, comp.height],
      'bitrate_mbps': (bitRate / 1000000.0).toString(),
      'include_audio': true,
      'audio_bit_rate': 192000,
    });
    queueExport(compId, specJson, path);
  }

  /// The export span in seconds — the work area when set, else the whole comp
  /// (a faithful mirror of `start_share_export`'s `duration`, before its
  /// 0.1 s floor, which [shareExportBitRate] applies).
  double _compDurationSeconds(BridgeComp comp) {
    final fps = comp.fps.fps;
    if (fps <= 0) return 0;
    final wa = comp.workArea;
    final frames =
        wa != null ? (wa[1] - wa[0]).toDouble() : comp.frameCount.toDouble();
    return frames / fps;
  }

  /// A best-effort read of whether [comp] carries audio: any audible footage
  /// layer whose source item probed with an audio track. The egui side asks the
  /// renderer for the comp's audio jobs; the snapshot cannot reproduce that
  /// exactly, so this approximates it (noted in the checklist).
  bool _compHasAudio(BridgeComp comp) {
    final snap = snapshot;
    if (snap == null) return false;
    for (final layer in comp.layers) {
      if (!layer.switches.audible) continue;
      final srcId = layer.sourceItemId;
      if (srcId == null) continue;
      if (_findItem(snap, srcId)?.media?.audio == true) return true;
    }
    return false;
  }

  /// Find a project item by its id across the snapshot tree, or null.
  BridgeItem? _findItem(BridgeSnapshot snap, String id) {
    BridgeItem? search(List<BridgeItem> items) {
      for (final item in items) {
        if (item.id == id) return item;
        final nested = search(item.children);
        if (nested != null) return nested;
      }
      return null;
    }

    return search(snap.items);
  }

  /// The front composition's display name (the `{comp}` filename token), or an
  /// empty string when there is no composition — for the export dialogue.
  String get frontCompName {
    final id = frontCompIdResolved;
    for (final c in compositions) {
      if (c.id == id) return c.name;
    }
    return '';
  }

  /// Run [op] against the bridge (a quiet no-op without one), applying its
  /// reply the same way [setLayerSwitch] and friends do.
  void _bridgeOp(BridgeReply Function(DocumentBridge b) op) {
    final b = bridge;
    if (b == null) return;
    _applyOp(op(b));
  }

  /// The current value of [layerId]'s transform [property]: the snapshot v3
  /// read-back when it is present, falling back to the session edit map (and
  /// null before any edit). The effect-controls panel adopts this so it shows
  /// true engine values once read-back lands, not only this session's edits.
  double? transformValueFor(String layerId, String property) {
    final snap = snapshot;
    if (snap != null) {
      final layer = _findLayer(snap, layerId);
      final prop = layer?.transform?[property];
      if (prop != null) return prop.value;
    }
    return transformEdits['$layerId/$property'];
  }

  /// Find a layer by its id across every composition in [snap], or null.
  BridgeLayer? _findLayer(BridgeSnapshot snap, String layerId) {
    BridgeLayer? search(List<BridgeItem> items) {
      for (final item in items) {
        final comp = item.comp;
        if (comp != null) {
          for (final l in comp.layers) {
            if (l.id == layerId) return l;
          }
        }
        final nested = search(item.children);
        if (nested != null) return nested;
      }
      return null;
    }

    return search(snap.items);
  }

  /// Decode one footage frame for the Viewer's CPU path, or null when there is
  /// no bridge or the frame cannot be decoded.
  DecodedFrame? decodeFrame(String itemId, int frame) =>
      bridge?.decodeFrame(itemId, frame);

  /// Apply a fine-grained op reply: refresh the snapshot on success (no chatty
  /// notice — these are direct manipulations, not menu actions), surface any
  /// failure in the error tint.
  void _applyOp(BridgeReply reply) {
    if (reply.ok) {
      _adoptSnapshot(reply.snapshot);
      errorNotice = null;
      // A successful direct edit dirties the document — the autosave gate (like
      // egui's `dirty`) so an idle session never writes rotating copies.
      _dirtySinceSave = true;
    } else {
      errorNotice = reply.error;
    }
    notifyListeners();
  }

  /// Adopt a snapshot into the held state (undo/redo flags follow it). Keeps the
  /// playhead range in step with the front comp so the Timeline scrub and the
  /// End-key jump land on real frames.
  void _adoptSnapshot(BridgeSnapshot? snap) {
    if (snap == null) return;
    snapshot = snap;
    canUndo = snap.canUndo;
    canRedo = snap.canRedo;
    final fc = frontComp;
    if (fc != null) {
      previewFrameCount = fc.frameCount;
      previewFrame = previewFrame.clamp(0, previewFrameCount);
    }
  }

  /// Apply a bridge reply: on success refresh the snapshot and post a quiet
  /// confirmation; on failure surface the engine's message in the error tint.
  void _applyReply(BridgeReply reply, String done) {
    if (reply.ok) {
      _adoptSnapshot(reply.snapshot);
      notice = done;
      errorNotice = null;
    } else {
      errorNotice = reply.error;
    }
    notifyListeners();
  }

  // --- Per-project session (SavedSession parity) --------------------------
  //
  // The Flutter counterpart of the egui shell's `SavedSession`: which comps are
  // open, which is fronted, where the playhead sits, and which layer is
  // selected — persisted per project path and restored when it reopens. All
  // additive: without the [rememberSession]/[sessionFor] seams wired (the shell
  // points them at the Workspace), the app behaves exactly as before.

  /// The session as it stands right now, for the front project.
  SavedSession currentSession() => SavedSession(
        openComps: List<String>.from(openComps),
        activeComp: frontCompIdResolved,
        frame: previewFrame,
        selectedLayer: selectedLayer,
      );

  /// Persist the current session against the loaded project path, if one is
  /// known and the seam is wired. A quiet no-op otherwise.
  void _persistSession() {
    final path = snapshot?.path;
    if (path == null) return;
    rememberSession?.call(path, currentSession());
  }

  /// Apply the stored session for [path] after its project opens: front the
  /// saved comp, restore the playhead and the selection — each validated
  /// against the freshly-loaded document so a stale id falls back to the
  /// default rather than crashing.
  void _applySessionFor(String path) {
    final read = sessionFor;
    if (read == null) return;
    final session = read(path);
    if (session == null) return;
    final comps = compositions;
    // Restore the open-comp list to the ids that still exist.
    openComps
      ..clear()
      ..addAll([
        for (final id in session.openComps)
          if (comps.any((c) => c.id == id)) id,
      ]);
    // Front the saved comp when it still resolves.
    final active = session.activeComp;
    if (active != null && comps.any((c) => c.id == active)) {
      frontCompId = active;
      previewFrameCount = frontComp?.frameCount ?? previewFrameCount;
    }
    // Restore the playhead, clamped into the (possibly changed) range.
    previewFrame = session.frame.clamp(0, previewFrameCount);
    // Restore the selection only if that layer is still present.
    final sel = session.selectedLayer;
    selectedLayer =
        (sel != null && snapshot != null && _findLayer(snapshot!, sel) != null)
            ? sel
            : null;
  }

  // --- Autosave (lumit_project::autosave parity) --------------------------
  //
  // A periodic rotating copy beside the project (`autosaves/<stem>.autosave-N.
  // lum`), written only when the document is dirty and has a path — the main
  // file is never touched. The timer is opt-in ([startAutosave]); tests drive
  // [autosaveTick] with an injected clock instead.

  bool _dirtySinceSave = false;
  DateTime _lastAutosave = DateTime.now();
  Timer? _autosaveTimer;

  /// Whether an autosave would write now (dirty, a bridge, and a saved path).
  bool get autosaveEligible =>
      _dirtySinceSave && bridge != null && snapshot?.path != null;

  /// Start the periodic autosave driver: every [checkEvery] it asks
  /// [autosaveTick] whether a write is due. Idempotent — a running timer is
  /// replaced. The shell calls this once a bridge is live; tests need not.
  void startAutosave({Duration checkEvery = const Duration(seconds: 30)}) {
    _autosaveTimer?.cancel();
    _lastAutosave = DateTime.now();
    _autosaveTimer =
        Timer.periodic(checkEvery, (_) => autosaveTick(DateTime.now()));
  }

  /// Stop the periodic autosave driver (no-op when none runs).
  void stopAutosave() {
    _autosaveTimer?.cancel();
    _autosaveTimer = null;
  }

  /// One autosave check at [now]: writes a rotating copy when the interval has
  /// elapsed and the document is dirty. The seam a timer (or a test clock)
  /// drives. Returns true when a copy was written.
  bool autosaveTick(DateTime now) {
    if (!autosaveEligible) return false;
    if (now.difference(_lastAutosave) < autosaveInterval) return false;
    _lastAutosave = now;
    return _writeAutosave();
  }

  /// Write one rotating autosave copy now, regardless of the interval (used by
  /// [autosaveTick] and available to a manual "save a copy" path). Rotates the
  /// `autosaves/` folder then writes the newest slot through the bridge; the
  /// reply is deliberately NOT adopted, so the held snapshot keeps pointing at
  /// the real project path.
  bool _writeAutosave() {
    final b = bridge;
    final path = snapshot?.path;
    if (b == null || path == null) return false;
    final slot1 = AutosaveScheme.rotateAndNewestSlot(path, autosaveKeep);
    final reply = b.saveProject(slot1);
    if (reply.ok) {
      // Autosave is silent in the egui frontend (no status-line notice), so
      // nothing is surfaced here beyond clearing the dirty gate.
      _dirtySinceSave = false;
      return true;
    }
    errorNotice = reply.error;
    notifyListeners();
    return false;
  }

  @override
  void dispose() {
    _autosaveTimer?.cancel();
    super.dispose();
  }
}
