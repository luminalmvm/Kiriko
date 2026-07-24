// The workspace controller: everything the egui `Shell` persists (dock
// layout, colour scheme, shape, accent override, animation level, the
// settings structs), held in one ChangeNotifier and written to a JSON file —
// the Flutter counterpart of eframe's storage (docs/archive/flutter-port/03).

import 'dart:convert';
import 'dart:io';
import 'dart:ui';

import 'package:flutter/foundation.dart';

import '../theme/theme.dart';
import 'dock.dart';
import 'settings.dart';

/// The per-project session the egui shell restores on open (its `SavedSession`,
/// crates/lumit-ui/src/app_state/mod.rs): which compositions are open, which is
/// fronted, where the playhead sits, and which layer is selected. Ids are the
/// snapshot's own string ids (the Flutter port keys by them, not Uuid). Stale
/// ids are validated against the document on restore and fall back to defaults,
/// so a session that names a since-deleted comp/layer never crashes.
class SavedSession {
  final List<String> openComps;
  final String? activeComp;
  final int frame;
  final String? selectedLayer;

  const SavedSession({
    this.openComps = const [],
    this.activeComp,
    this.frame = 0,
    this.selectedLayer,
  });

  Map<String, dynamic> toJson() => {
        'open_comps': openComps,
        'active_comp': activeComp,
        'frame': frame,
        'selected_layer': selectedLayer,
      };

  factory SavedSession.fromJson(Map<String, dynamic> j) => SavedSession(
        openComps: j['open_comps'] is List
            ? [for (final c in j['open_comps'] as List) if (c is String) c]
            : const [],
        activeComp: j['active_comp'] is String ? j['active_comp'] as String : null,
        frame: j['frame'] is num ? (j['frame'] as num).toInt() : 0,
        selectedLayer:
            j['selected_layer'] is String ? j['selected_layer'] as String : null,
      );

  @override
  bool operator ==(Object other) =>
      other is SavedSession &&
      other.activeComp == activeComp &&
      other.frame == frame &&
      other.selectedLayer == selectedLayer &&
      _listEq(other.openComps, openComps);

  @override
  int get hashCode => Object.hash(
        activeComp,
        frame,
        selectedLayer,
        Object.hashAll(openComps),
      );

  static bool _listEq(List<String> a, List<String> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

/// The autosave file scheme, mirroring `lumit_project::autosave` (rotating
/// copies beside the project in an `autosaves/` sibling folder,
/// `{stem}.autosave-1.lum` newest). Kept as pure functions so the naming and
/// rotation are unit-tested without a bridge or a real save.
class AutosaveScheme {
  /// The `autosaves/` directory beside [projectPath].
  static String dir(String projectPath) {
    final sep = Platform.pathSeparator;
    final parent = File(projectPath).parent.path;
    return '$parent${sep}autosaves';
  }

  /// The project file's stem (name without its extension), the autosave prefix.
  static String stem(String projectPath) {
    var name = File(projectPath).uri.pathSegments.isNotEmpty
        ? File(projectPath).uri.pathSegments.last
        : projectPath;
    // Strip the last extension only (`edit.lum` → `edit`), matching Rust's
    // `file_stem`. A dotfile with no extension keeps its name.
    final dot = name.lastIndexOf('.');
    if (dot > 0) name = name.substring(0, dot);
    return name.isEmpty ? 'project' : name;
  }

  /// The slot-[k] autosave path (k = 1 is newest).
  static String slot(String projectPath, int k) {
    final sep = Platform.pathSeparator;
    return '${dir(projectPath)}$sep${stem(projectPath)}.autosave-$k.lum';
  }

  /// Rotate the existing autosaves up one slot and return the (now free) newest
  /// slot to write. The oldest ([keep]) falls off the end. Best-effort file
  /// moves: a missing slot is simply skipped, never an error. The main project
  /// file is never touched. Creates the `autosaves/` folder if needed.
  static String rotateAndNewestSlot(String projectPath, int keep) {
    final k = keep < 1 ? 1 : keep;
    Directory(dir(projectPath)).createSync(recursive: true);
    // Drop the oldest.
    final oldest = File(slot(projectPath, k));
    if (oldest.existsSync()) {
      try {
        oldest.deleteSync();
      } catch (_) {}
    }
    // Shift the rest up: k-1 → k, … , 1 → 2.
    for (var i = k - 1; i >= 1; i--) {
      final from = File(slot(projectPath, i));
      if (from.existsSync()) {
        try {
          from.renameSync(slot(projectPath, i + 1));
        } catch (_) {}
      }
    }
    return slot(projectPath, 1);
  }
}

class Workspace extends ChangeNotifier {
  DockSplit dock = defaultLayout();
  LumitColorScheme colorScheme = LumitColorScheme.dark;
  ThemeShape themeShape = ThemeShape.sharp;
  Color? accentOverride;
  AnimationLevel animationLevel = AnimationLevel.all;

  PerformanceSettings performance = PerformanceSettings();
  AutosaveSettings autosave = AutosaveSettings();
  InterfaceSettings interface = InterfaceSettings();
  ExportSettings export = ExportSettings();

  /// The project last opened or saved with a path, restored on the next launch
  /// (the egui frontend reopens the last project the same way). Null until a
  /// project has been opened or saved to a file. This is only the *file*;
  /// [sessions] carries the per-project session beside it.
  String? lastProjectPath;

  /// Per-project sessions keyed by project file path — the Flutter counterpart
  /// of the egui shell's `SavedSession` map, restored when a project reopens.
  final Map<String, SavedSession> sessions = {};

  LumitTheme _theme = LumitTheme.dark();
  LumitTheme get theme => _theme;

  Workspace() {
    recompose();
  }

  /// Rebuild the theme from the current appearance fields — the single funnel
  /// every Appearance control uses (`Shell::recompose`).
  void recompose() {
    _theme = LumitTheme.forScheme(
      colorScheme,
      themeShape,
      accentOverride: accentOverride,
    );
    notifyListeners();
  }

  void setScheme(LumitColorScheme s) {
    colorScheme = s;
    recompose();
    save();
  }

  void setShape(ThemeShape s) {
    themeShape = s;
    recompose();
    save();
  }

  void setAccent(Color? c) {
    accentOverride = c;
    recompose();
    save();
  }

  void setAnimationLevel(AnimationLevel a) {
    animationLevel = a;
    notifyListeners();
    save();
  }

  void resetWorkspaceLayout() {
    dock = defaultLayout();
    notifyListeners();
    save();
  }

  void touch() {
    notifyListeners();
    save();
  }

  /// Remember the file a project was just opened from or saved to, so the next
  /// launch can reopen it. Persisted immediately; no theme rebuild is needed, so
  /// this does not notify listeners.
  void rememberProject(String path) {
    lastProjectPath = path;
    save();
  }

  /// Remember [session] for the project at [path], persisted immediately so the
  /// next open restores it. A no-op write when the session is unchanged, so the
  /// piggybacked [save] does not churn the store on every identical update.
  void rememberSession(String path, SavedSession session) {
    if (sessions[path] == session) return;
    sessions[path] = session;
    save();
  }

  /// The saved session for the project at [path], or null when none is stored.
  SavedSession? sessionFor(String path) => sessions[path];

  // --- Persistence ---------------------------------------------------------

  /// `%APPDATA%\lumit\flutter-workspace.json` on Windows; a dotfolder
  /// fallback elsewhere. No plugin needed, and nothing machine-specific ever
  /// enters the repository.
  static File storeFile() {
    final base = Platform.environment['APPDATA'] ??
        '${Platform.environment['HOME'] ?? '.'}/.config';
    return File('$base${Platform.pathSeparator}lumit'
        '${Platform.pathSeparator}flutter-workspace.json');
  }

  Map<String, dynamic> toJson() => {
        'version': 1,
        'dock': dock.toJson(),
        'color_scheme': colorScheme.name,
        'theme_shape': themeShape.name,
        'accent_override': accentOverride == null
            ? null
            : [
                (accentOverride!.r * 255).round(),
                (accentOverride!.g * 255).round(),
                (accentOverride!.b * 255).round(),
              ],
        'animation_level': animationLevel.name,
        'performance': performance.toJson(),
        'autosave': autosave.toJson(),
        'interface': interface.toJson(),
        'export': export.toJson(),
        'last_project_path': lastProjectPath,
        'sessions': {
          for (final e in sessions.entries) e.key: e.value.toJson(),
        },
      };

  void applyJson(Map<String, dynamic> j) {
    final d = j['dock'];
    if (d is Map<String, dynamic>) {
      final parsed = DockNode.fromJson(d);
      if (parsed is DockSplit) dock = parsed;
    }
    colorScheme = LumitColorScheme.values.asNameMap()[j['color_scheme']] ??
        LumitColorScheme.dark;
    themeShape =
        ThemeShape.values.asNameMap()[j['theme_shape']] ?? ThemeShape.sharp;
    final acc = j['accent_override'];
    accentOverride = acc is List && acc.length == 3
        ? Color.fromARGB(0xff, acc[0] as int, acc[1] as int, acc[2] as int)
        : null;
    animationLevel = AnimationLevel.values.asNameMap()[j['animation_level']] ??
        AnimationLevel.all;
    if (j['performance'] is Map<String, dynamic>) {
      performance = PerformanceSettings.fromJson(j['performance']);
    }
    if (j['autosave'] is Map<String, dynamic>) {
      autosave = AutosaveSettings.fromJson(j['autosave']);
    }
    if (j['interface'] is Map<String, dynamic>) {
      interface = InterfaceSettings.fromJson(j['interface']);
    }
    if (j['export'] is Map<String, dynamic>) {
      export = ExportSettings.fromJson(j['export']);
    }
    lastProjectPath =
        j['last_project_path'] is String ? j['last_project_path'] as String : null;
    sessions.clear();
    final rawSessions = j['sessions'];
    if (rawSessions is Map) {
      rawSessions.forEach((key, value) {
        if (key is String && value is Map) {
          sessions[key] = SavedSession.fromJson(value.cast<String, dynamic>());
        }
      });
    }
    // The left group always opens on Project (activate_panel_tab at start-up).
    activatePanelTab(dock, Panel.project);
    recompose();
  }

  void load() {
    try {
      final f = storeFile();
      if (!f.existsSync()) return;
      final j = jsonDecode(f.readAsStringSync());
      if (j is Map<String, dynamic>) applyJson(j);
    } catch (_) {
      // A corrupt store falls back to defaults — never a crash.
    }
  }

  void save() {
    try {
      final f = storeFile();
      f.parent.createSync(recursive: true);
      f.writeAsStringSync(const JsonEncoder.withIndent('  ').convert(toJson()));
    } catch (_) {
      // Persistence is best-effort; the session keeps working without it.
    }
  }
}
