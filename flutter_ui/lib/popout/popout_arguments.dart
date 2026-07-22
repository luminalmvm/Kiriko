// The arguments a popped-out panel window carries across the engine boundary.
//
// In plain terms: a popped-out panel runs in its OWN Flutter engine (a second
// engine in the SAME process — see docs/flutter-port/03), so it does not share
// the main window's Dart objects. What it needs is (a) which panel to host,
// (b) enough appearance state to rebuild the theme identically, and (c) the
// document context (project path, front comp, selection) to open focused. All
// of that is serialised to a single JSON string — the one argument
// `desktop_multi_window` hands a new window — and parsed back here.
//
// This file is pure Dart (no plugin imports) so it is fully unit-testable and
// is shared by both the main window (which builds the arguments) and the
// popout entrypoint (which parses them).

import 'dart:convert';

import 'package:flutter/widgets.dart' show Color;

import '../state/dock.dart';
import '../theme/theme.dart';

/// The marker that tells the shared entrypoint a new engine is a popped-out
/// panel rather than the main window. The main window's argument string is
/// empty (`WindowController.fromCurrentEngine().arguments == ''`).
const String kPopoutBusinessId = 'popout_panel';

/// The panels a popout can host honestly (docs/flutter-port/05, F-multiwindow).
/// The Viewer and Timeline stay in-window: the Viewer owns the shared-texture
/// registrar (a per-view, main-window concern) and the Timeline owns the
/// playhead/transport and the cache-bar warm set tied to the main preview — a
/// second engine would fork that state. The rest are read-mostly: they read the
/// shared document snapshot and push edits straight through the bridge.
const Set<Panel> kPopoutHostablePanels = {
  Panel.project,
  Panel.hierarchy,
  Panel.effectControls,
  Panel.effectsAndPresets,
  Panel.scopes,
};

/// Whether [panel] may be offered for pop-out.
bool canPopOutPanel(Panel panel) => kPopoutHostablePanels.contains(panel);

/// The decoded arguments of a popout window: the panel to host plus the
/// appearance and document context needed to rebuild it faithfully in a fresh
/// engine.
class PopoutArguments {
  final Panel panel;

  // Appearance — enough to reconstruct the exact [LumitTheme] the main window
  // showed (scheme + shape + optional accent override), plus the two other
  // ThemeScope inputs.
  final LumitColorScheme scheme;
  final ThemeShape shape;

  /// The user's accent override as packed ARGB, or null for the scheme accent.
  final int? accentArgb;
  final AnimationLevel animationLevel;
  final bool showTooltips;
  final double uiScale;

  /// The open project's path, so the popout's own bridge handle opens the same
  /// document even if the process-wide engine were somehow reset. Usually the
  /// engine already holds the document (same process), so this is belt-and-
  /// braces context; null for an unsaved document.
  final String? projectPath;

  /// The composition the main window had fronted, so the popout opens on the
  /// same comp; null falls back to the first composition.
  final String? frontCompId;

  /// The layer the main window had selected, so an Effect-controls popout opens
  /// on the same layer; null opens unselected.
  final String? selectedLayer;

  const PopoutArguments({
    required this.panel,
    required this.scheme,
    required this.shape,
    this.accentArgb,
    this.animationLevel = AnimationLevel.all,
    this.showTooltips = true,
    this.uiScale = 1.0,
    this.projectPath,
    this.frontCompId,
    this.selectedLayer,
  });

  /// The accent override as a [Color], or null for the scheme's own accent.
  Color? get accentOverride => accentArgb == null ? null : Color(accentArgb!);

  /// Pack a [Color] into an opaque ARGB int for [accentArgb], using the
  /// component accessors (the codebase's no-`.value` convention).
  static int packAccent(Color c) =>
      (0xff << 24) |
      ((c.r * 255).round() & 0xff) << 16 |
      ((c.g * 255).round() & 0xff) << 8 |
      ((c.b * 255).round() & 0xff);

  /// The exact theme the main window showed, rebuilt from the appearance
  /// fields — the same funnel `Workspace.recompose` uses.
  LumitTheme get theme => LumitTheme.forScheme(
        scheme,
        shape,
        accentOverride: accentOverride,
      );

  Map<String, dynamic> toJson() => {
        'businessId': kPopoutBusinessId,
        'panel': panel.name,
        'scheme': scheme.name,
        'shape': shape.name,
        if (accentArgb != null) 'accent_argb': accentArgb,
        'animation_level': animationLevel.name,
        'show_tooltips': showTooltips,
        'ui_scale': uiScale,
        if (projectPath != null) 'project_path': projectPath,
        if (frontCompId != null) 'front_comp_id': frontCompId,
        if (selectedLayer != null) 'selected_layer': selectedLayer,
      };

  /// The single argument string handed to a new window.
  String toArguments() => jsonEncode(toJson());

  /// Whether [arguments] (a window's argument string) declares a popout. The
  /// main window's argument is empty, so this is false for it — the dispatch
  /// discriminator in the shared entrypoint.
  static bool isPopout(String arguments) {
    if (arguments.isEmpty) return false;
    try {
      final decoded = jsonDecode(arguments);
      return decoded is Map && decoded['businessId'] == kPopoutBusinessId;
    } catch (_) {
      return false;
    }
  }

  /// Parse a popout argument string, or null when it is not a valid popout
  /// (empty, malformed, or an unknown panel — a newer main window naming a
  /// panel this build does not host degrades to null rather than crashing).
  static PopoutArguments? tryParse(String arguments) {
    if (arguments.isEmpty) return null;
    Object? decoded;
    try {
      decoded = jsonDecode(arguments);
    } catch (_) {
      return null;
    }
    if (decoded is! Map || decoded['businessId'] != kPopoutBusinessId) {
      return null;
    }
    final panel = Panel.values.asNameMap()[decoded['panel']];
    if (panel == null) return null;
    final scheme = LumitColorScheme.values.asNameMap()[decoded['scheme']] ??
        LumitColorScheme.dark;
    final shape =
        ThemeShape.values.asNameMap()[decoded['shape']] ?? ThemeShape.sharp;
    final level =
        AnimationLevel.values.asNameMap()[decoded['animation_level']] ??
            AnimationLevel.all;
    final accent = decoded['accent_argb'];
    final scale = decoded['ui_scale'];
    return PopoutArguments(
      panel: panel,
      scheme: scheme,
      shape: shape,
      accentArgb: accent is num ? accent.toInt() : null,
      animationLevel: level,
      showTooltips: decoded['show_tooltips'] as bool? ?? true,
      uiScale: scale is num ? scale.toDouble() : 1.0,
      projectPath: decoded['project_path'] as String?,
      frontCompId: decoded['front_comp_id'] as String?,
      selectedLayer: decoded['selected_layer'] as String?,
    );
  }
}
