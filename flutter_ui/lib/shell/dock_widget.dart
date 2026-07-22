// Renders the dock tree (state/dock.dart): weighted splits with draggable
// dividers, tab groups as pill tab bars (dock.rs::tab_ui styling), solo panes
// bare (K-086), and the Sharp/Round pane chrome (K-092). Tabs and bare panes
// drag to re-dock (dock.rs drag-to-redock, via egui_tiles): a ghost pill
// follows the cursor, the hovered pane shows a drop-zone preview, and release
// commits the move through movePanel. Pop-out windows remain a checklist item.

import 'package:flutter/widgets.dart';

import '../icons/icons.dart';
import '../state/dock.dart';
import '../theme/theme.dart';
import '../widgets/controls.dart';

typedef PanelBuilder = Widget Function(BuildContext context, Panel panel);

/// A pointer must travel this far before a press on a tab or grip becomes a
/// re-dock drag rather than a click.
const double _dragSlop = 6.0;

class DockWidget extends StatefulWidget {
  final DockSplit root;
  final PanelBuilder buildPanel;
  final VoidCallback onLayoutChanged;

  /// The panel that last took a click — it wears the accent boundary so the
  /// keyboard's home is always visible (Shell::active_panel).
  final ValueNotifier<Panel?> activePanel;

  /// Called when a pane's context menu asks to pop out into its own window.
  final void Function(Panel) onPopOut;

  /// Whether [onPopOut] is offered for a panel — the pop-out button and the
  /// bare-pane context-menu item are hidden when this returns false (the Viewer
  /// and Timeline, and an already-floating panel). Defaults to always offered.
  final bool Function(Panel)? canPopOut;

  const DockWidget({
    super.key,
    required this.root,
    required this.buildPanel,
    required this.onLayoutChanged,
    required this.activePanel,
    required this.onPopOut,
    this.canPopOut,
  });

  bool _canPopOut(Panel panel) => canPopOut?.call(panel) ?? true;

  @override
  State<DockWidget> createState() => _DockWidgetState();
}

class _DockWidgetState extends State<DockWidget> {
  // One stable key per panel, used to hit-test the pane rects during a drag.
  // A panel keeps its key even while it is an inactive tab (unbuilt, so its
  // key resolves to no context and is skipped).
  late final Map<Panel, GlobalKey> _paneKeys = {
    for (final p in Panel.values) p: GlobalKey(),
  };
  late final _DragController _drag = _DragController(
    paneKeys: _paneKeys,
    onGhostShow: _showGhost,
    onGhostHide: _removeGhost,
    onCommit: _commitMove,
  );
  OverlayEntry? _ghost;

  @override
  void dispose() {
    _removeGhost();
    _drag.dispose();
    super.dispose();
  }

  void _showGhost() {
    _removeGhost();
    _ghost = OverlayEntry(builder: (_) => _GhostLayer(drag: _drag));
    Overlay.of(context).insert(_ghost!);
  }

  void _removeGhost() {
    _ghost?.remove();
    _ghost = null;
  }

  void _commitMove(Panel dragged, Panel target, DropPosition pos) {
    setState(() => movePanel(widget.root, dragged, target, pos));
    widget.onLayoutChanged();
  }

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    return Container(
      color: t.surface0,
      padding: EdgeInsets.all(t.tokens.windowInset),
      child: _buildNode(context, widget.root),
    );
  }

  Widget _buildNode(BuildContext context, DockNode node) => switch (node) {
        DockPane(:final panel) => _PaneChrome(
            bare: true,
            panel: panel,
            activePanel: widget.activePanel,
            onPopOut: widget.onPopOut,
            offerPopOut: widget._canPopOut(panel),
            drag: _drag,
            child: widget.buildPanel(context, panel),
          ),
        DockTabs() => _TabGroup(
            tabs: node,
            buildPanel: widget.buildPanel,
            activePanel: widget.activePanel,
            onPopOut: widget.onPopOut,
            canPopOut: widget._canPopOut,
            drag: _drag,
            onChanged: () {
              setState(() {});
              widget.onLayoutChanged();
            },
          ),
        DockSplit() => _buildSplit(context, node),
      };

  Widget _buildSplit(BuildContext context, DockSplit split) {
    final t = ThemeScope.of(context).theme;
    final horizontal = split.axis == DockAxis.horizontal;
    final children = <Widget>[];
    for (var i = 0; i < split.children.length; i++) {
      children.add(Expanded(
        // Flex is integer; scale the share up to keep precision.
        flex: (split.shares[i] * 10000).round().clamp(1, 1 << 30),
        child: _buildNode(context, split.children[i]),
      ));
      if (i < split.children.length - 1) {
        children.add(_Divider(
          horizontal: horizontal,
          gap: t.tokens.tileGap,
          onDrag: (delta, totalExtent) {
            setState(() {
              _resize(split, i, horizontal ? delta.dx : delta.dy, totalExtent);
            });
            widget.onLayoutChanged();
          },
        ));
      }
    }
    return horizontal ? Row(children: children) : Column(children: children);
  }

  /// Move the boundary between child i and i+1 by `deltaPx` of `totalExtent`.
  void _resize(DockSplit split, int i, double deltaPx, double totalExtent) {
    if (totalExtent <= 0) return;
    final total = split.shares.reduce((a, b) => a + b);
    final deltaShare = deltaPx / totalExtent * total;
    const minShare = 0.05;
    final a = split.shares[i] + deltaShare;
    final b = split.shares[i + 1] - deltaShare;
    if (a < minShare || b < minShare) return;
    split.shares[i] = a;
    split.shares[i + 1] = b;
  }
}

/// The live state of a re-dock drag, shared between the dragged source (a tab
/// pill or a bare pane's grip), the ghost pill and every pane's drop preview.
/// It resolves the hovered pane and drop position by hit-testing the pointer
/// against the pane rects each update, because MouseRegion does not fire while
/// a pointer is captured by a drag.
class _DragController extends ChangeNotifier {
  final Map<Panel, GlobalKey> paneKeys;
  final VoidCallback onGhostShow;
  final VoidCallback onGhostHide;
  final void Function(Panel dragged, Panel target, DropPosition pos) onCommit;

  _DragController({
    required this.paneKeys,
    required this.onGhostShow,
    required this.onGhostHide,
    required this.onCommit,
  });

  Panel? dragged;
  LumitTheme? theme;
  Offset pointer = Offset.zero;
  Panel? hoveredPanel;
  DropPosition? dropPosition;

  void start(Panel panel, Offset globalPos, LumitTheme t) {
    dragged = panel;
    theme = t;
    pointer = globalPos;
    _resolve();
    onGhostShow();
    notifyListeners();
  }

  void update(Offset globalPos) {
    pointer = globalPos;
    _resolve();
    notifyListeners();
  }

  void finish() {
    final dragged = this.dragged;
    final target = hoveredPanel;
    final pos = dropPosition;
    _reset();
    onGhostHide();
    notifyListeners();
    if (dragged != null && target != null && pos != null) {
      onCommit(dragged, target, pos);
    }
  }

  void cancel() {
    _reset();
    onGhostHide();
    notifyListeners();
  }

  void _reset() {
    dragged = null;
    theme = null;
    hoveredPanel = null;
    dropPosition = null;
  }

  /// Find the pane under the pointer and the drop position within it.
  void _resolve() {
    hoveredPanel = null;
    dropPosition = null;
    for (final entry in paneKeys.entries) {
      final ctx = entry.value.currentContext;
      if (ctx == null) continue;
      final box = ctx.findRenderObject() as RenderBox?;
      if (box == null || !box.attached) continue;
      final rect = box.localToGlobal(Offset.zero) & box.size;
      if (rect.contains(pointer)) {
        hoveredPanel = entry.key;
        dropPosition = _positionIn(rect, pointer);
        return;
      }
    }
  }

  /// The inner ~50% of both axes is a stack; outside it, the nearest edge
  /// picks a side to split off.
  static DropPosition _positionIn(Rect rect, Offset p) {
    final fx = ((p.dx - rect.left) / rect.width).clamp(0.0, 1.0);
    final fy = ((p.dy - rect.top) / rect.height).clamp(0.0, 1.0);
    if ((fx - 0.5).abs() < 0.25 && (fy - 0.5).abs() < 0.25) {
      return DropPosition.stack;
    }
    final left = fx, right = 1 - fx, top = fy, bottom = 1 - fy;
    final nearest = [left, right, top, bottom].reduce((a, b) => a < b ? a : b);
    if (nearest == left) return DropPosition.left;
    if (nearest == right) return DropPosition.right;
    if (nearest == top) return DropPosition.above;
    return DropPosition.below;
  }
}

/// Turns a press-and-drag on its child into a re-dock drag: a press that
/// travels past the slop starts the drag and drives the controller, leaving a
/// plain tap (a tab click) to the child's own gesture handling. Uses a raw
/// Listener so it stays out of the gesture arena and never fights the tab
/// strip's horizontal scroll.
class _DragSource extends StatefulWidget {
  final Panel panel;
  final _DragController drag;
  final Widget child;

  const _DragSource({
    required this.panel,
    required this.drag,
    required this.child,
  });

  @override
  State<_DragSource> createState() => _DragSourceState();
}

class _DragSourceState extends State<_DragSource> {
  Offset? _downAt;
  bool _dragging = false;

  /// Theme snapshot taken at pointer-down, when the element is certainly
  /// live. A press elsewhere can rebuild the dock (the active-panel edge)
  /// and deactivate this element while the pointer stays captured, so the
  /// move handler must never look up inherited widgets through `context`.
  LumitTheme? _theme;

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerDown: (e) {
        _downAt = e.position;
        _dragging = false;
        _theme = ThemeScope.of(context).theme;
      },
      onPointerMove: (e) {
        final theme = _theme;
        if (_downAt == null || theme == null) return;
        if (!_dragging) {
          if ((e.position - _downAt!).distance < _dragSlop) return;
          _dragging = true;
          widget.drag.start(widget.panel, e.position, theme);
        } else {
          widget.drag.update(e.position);
        }
      },
      onPointerUp: (e) {
        if (_dragging) widget.drag.finish();
        _dragging = false;
        _downAt = null;
      },
      onPointerCancel: (e) {
        if (_dragging) widget.drag.cancel();
        _dragging = false;
        _downAt = null;
      },
      child: widget.child,
    );
  }
}

/// The ghost pill that follows the cursor during a drag, on the app Overlay.
class _GhostLayer extends StatelessWidget {
  final _DragController drag;
  const _GhostLayer({required this.drag});

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: drag,
        builder: (context, _) {
          final panel = drag.dragged;
          final t = drag.theme;
          if (panel == null || t == null) return const SizedBox.shrink();
          return Positioned(
            left: drag.pointer.dx + 10,
            top: drag.pointer.dy + 8,
            child: IgnorePointer(child: _GhostPill(title: panel.title, theme: t)),
          );
        },
      );
}

/// The floating tab pill, styled like the active pill it was lifted from.
class _GhostPill extends StatelessWidget {
  final String title;
  final LumitTheme theme;
  const _GhostPill({required this.title, required this.theme});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
      decoration: BoxDecoration(
        color: theme.surface1,
        borderRadius: BorderRadius.circular(theme.tokens.controlRadius),
        border: Border.all(color: theme.accent, width: 1),
        boxShadow: theme.floatShadow,
      ),
      child: Text(title, style: theme.body.copyWith(color: theme.textPrimary)),
    );
  }
}

class _Divider extends StatefulWidget {
  final bool horizontal;
  final double gap;
  final void Function(Offset delta, double totalExtent) onDrag;

  const _Divider({
    required this.horizontal,
    required this.gap,
    required this.onDrag,
  });

  @override
  State<_Divider> createState() => _DividerState();
}

class _DividerState extends State<_Divider> {
  bool _hover = false;
  bool _dragging = false;

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    // Sharp: hairline-toned gap, brighter on hover/drag. Round: canvas-toned
    // gap, hairline on hover, accent while dragging (dock.rs::resize_stroke).
    final sharp = t.shape == ThemeShape.sharp;
    final idle = sharp ? t.surface2 : t.surface0;
    final colour = _dragging
        ? (sharp ? t.textPrimary : t.accent)
        : _hover
            ? (sharp ? t.textPrimary : t.hairlineStrong)
            : idle;
    // The visible gap keeps the token width; the hit area is padded to a
    // comfortable 7 px so a 1 px hairline is still grabbable.
    final hit = widget.gap < 7.0 ? 7.0 : widget.gap;
    return MouseRegion(
      cursor: widget.horizontal
          ? SystemMouseCursors.resizeColumn
          : SystemMouseCursors.resizeRow,
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onPanStart: (_) => setState(() => _dragging = true),
        onPanEnd: (_) => setState(() => _dragging = false),
        onPanCancel: () => setState(() => _dragging = false),
        onPanUpdate: (d) {
          final parent = context
              .findAncestorRenderObjectOfType<RenderBox>()
              ?.size;
          final extent = parent == null
              ? 0.0
              : (widget.horizontal ? parent.width : parent.height);
          widget.onDrag(d.delta, extent);
        },
        child: SizedBox(
          width: widget.horizontal ? hit : null,
          height: widget.horizontal ? null : hit,
          child: Center(
            child: Container(
              width: widget.horizontal ? widget.gap : null,
              height: widget.horizontal ? null : widget.gap,
              color: colour,
            ),
          ),
        ),
      ),
    );
  }
}

/// A tab group: the 26 px tab bar of pill tabs plus the active pane's body.
class _TabGroup extends StatelessWidget {
  final DockTabs tabs;
  final PanelBuilder buildPanel;
  final VoidCallback onChanged;
  final ValueNotifier<Panel?> activePanel;
  final void Function(Panel) onPopOut;
  final bool Function(Panel) canPopOut;
  final _DragController drag;

  const _TabGroup({
    required this.tabs,
    required this.buildPanel,
    required this.onChanged,
    required this.activePanel,
    required this.onPopOut,
    required this.canPopOut,
    required this.drag,
  });

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    final barColour =
        t.shape == ThemeShape.sharp ? t.surface2 : t.surface0;
    return Column(
      children: [
        Container(
          height: 26,
          color: barColour,
          child: Row(
            children: [
              // The pill strip scrolls when the group is narrower than its
              // tabs, as egui_tiles' tab bar does.
              Expanded(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    children: [
                      for (var i = 0; i < tabs.children.length; i++)
                        _TabPill(
                          panel: tabs.children[i].panel,
                          title: tabs.children[i].panel.title,
                          active: i == tabs.active,
                          drag: drag,
                          onPressed: () {
                            tabs.active = i;
                            onChanged();
                          },
                        ),
                    ],
                  ),
                ),
              ),
              // The pop-out button for the active tab (top_bar_right_ui), shown
              // only for panels a popout can host.
              if (canPopOut(tabs.activePane.panel)) ...[
                LumitTooltip(
                  message: 'Pop out into its own window',
                  child: HouseButton(
                    frameless: true,
                    small: true,
                    onPressed: () => onPopOut(tabs.activePane.panel),
                    child: lumitIcon(LumitIcon.popOut,
                        size: 12, color: t.textMuted),
                  ),
                ),
                const SizedBox(width: 4),
              ],
            ],
          ),
        ),
        Expanded(
          child: _PaneChrome(
            bare: false,
            panel: tabs.activePane.panel,
            activePanel: activePanel,
            onPopOut: onPopOut,
            offerPopOut: false,
            drag: drag,
            child: buildPanel(context, tabs.activePane.panel),
          ),
        ),
      ],
    );
  }
}

class _TabPill extends StatefulWidget {
  final Panel panel;
  final String title;
  final bool active;
  final VoidCallback onPressed;
  final _DragController drag;

  const _TabPill({
    required this.panel,
    required this.title,
    required this.active,
    required this.onPressed,
    required this.drag,
  });

  @override
  State<_TabPill> createState() => _TabPillState();
}

class _TabPillState extends State<_TabPill> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    final Color fill;
    final Color textColour;
    Border? border;
    if (widget.active) {
      fill = t.surface1;
      textColour = t.textPrimary;
      border = Border.all(color: t.accent, width: 1);
    } else if (_hover) {
      fill = t.surface3;
      textColour = t.textPrimary;
      border = Border.all(color: t.hairlineStrong, width: 1);
    } else {
      fill = t.surface2;
      textColour = t.textMuted;
    }
    final pill = Container(
      margin: const EdgeInsets.symmetric(horizontal: 2, vertical: 4),
      padding: const EdgeInsets.symmetric(horizontal: 10),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: fill,
        borderRadius: BorderRadius.circular(t.tokens.controlRadius),
        border: border,
      ),
      child: Text(widget.title, style: t.body.copyWith(color: textColour)),
    );
    return _DragSource(
      panel: widget.panel,
      drag: widget.drag,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onPressed,
          // While this pill is the dragged one, it paints nothing but keeps
          // its footprint — egui leaves the gap while the ghost floats free.
          child: AnimatedBuilder(
            animation: widget.drag,
            builder: (context, child) => Opacity(
              opacity: widget.drag.dragged == widget.panel ? 0.0 : 1.0,
              child: child,
            ),
            child: pill,
          ),
        ),
      ),
    );
  }
}

/// The pane body chrome: Sharp draws edge-to-edge on `surface1`; Round wraps
/// the content in a rounded, shadowed, padded card (dock.rs::pane_ui). Any
/// click inside makes this the active panel, which wears the accent boundary
/// (Shell::active_panel); a right-click on a bare pane offers "pop out"
/// (bare_pane_ui — tabbed panes get it from the tab bar's own button). A live
/// re-dock drag paints the drop-zone preview over the hovered pane, and a bare
/// pane carries the corner drag grip (dock.rs::paint_bare_pane_grip).
class _PaneChrome extends StatelessWidget {
  final bool bare;
  final Panel panel;
  final ValueNotifier<Panel?> activePanel;
  final void Function(Panel) onPopOut;

  /// Whether this bare pane offers "pop out" in its context menu (hidden for
  /// panels a popout cannot host). Ignored for a tabbed pane, which gets its
  /// pop-out affordance from the tab bar's own button.
  final bool offerPopOut;
  final _DragController drag;
  final Widget child;

  const _PaneChrome({
    required this.bare,
    required this.panel,
    required this.activePanel,
    required this.onPopOut,
    required this.offerPopOut,
    required this.drag,
    required this.child,
  });

  void _contextMenu(BuildContext context, Offset globalPos) {
    showLumitPopup<void>(
      context: context,
      position: globalPos,
      builder: (close) => FloatSurface(
        child: MenuRow(
          onPressed: () {
            close(null);
            onPopOut(panel);
          },
          child: const Text('Pop out into its own window'),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = ThemeScope.of(context).theme;
    final round = t.shape == ThemeShape.round;
    return ValueListenableBuilder<Panel?>(
      valueListenable: activePanel,
      builder: (context, active, _) => Listener(
        // Any press claims focus for this panel, before the content handles
        // the event (the egui edge follows the last click the same way).
        onPointerDown: (_) => activePanel.value = panel,
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          onSecondaryTapDown: bare && offerPopOut
              ? (d) => _contextMenu(context, d.globalPosition)
              : null,
          child: Container(
            key: drag.paneKeys[panel],
            decoration: BoxDecoration(
              color: t.surface1,
              borderRadius:
                  round ? BorderRadius.circular(t.tokens.cardRadius) : null,
              boxShadow: round ? t.tokens.cardShadow : null,
            ),
            // The accent boundary paints over the content's edge, like the
            // egui overlay stroke at Order::Middle.
            foregroundDecoration: active == panel
                ? BoxDecoration(
                    border: Border.all(color: t.accent, width: 1),
                    borderRadius: round
                        ? BorderRadius.circular(t.tokens.cardRadius)
                        : null,
                  )
                : null,
            padding: round ? EdgeInsets.all(t.tokens.cardPadding) : null,
            clipBehavior: round ? Clip.antiAlias : Clip.none,
            child: Stack(
              children: [
                child,
                Positioned.fill(child: _DropPreview(panel: panel, drag: drag)),
                if (bare)
                  Positioned(
                    top: 0,
                    right: 0,
                    child: _PaneGrip(panel: panel, drag: drag),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The translucent accent region shown over the pane the pointer hovers while
/// a re-dock drag is live: the whole pane for a stack, the near half for an
/// edge split.
class _DropPreview extends StatelessWidget {
  final Panel panel;
  final _DragController drag;
  const _DropPreview({required this.panel, required this.drag});

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: drag,
        builder: (context, _) {
          if (drag.dragged == null ||
              drag.hoveredPanel != panel ||
              drag.dropPosition == null) {
            return const SizedBox.shrink();
          }
          final t = ThemeScope.of(context).theme;
          return IgnorePointer(
            child: CustomPaint(
              painter: _DropPainter(pos: drag.dropPosition!, accent: t.accent),
            ),
          );
        },
      );
}

class _DropPainter extends CustomPainter {
  final DropPosition pos;
  final Color accent;
  const _DropPainter({required this.pos, required this.accent});

  @override
  void paint(Canvas canvas, Size size) {
    final region = switch (pos) {
      DropPosition.stack => Offset.zero & size,
      DropPosition.left => Rect.fromLTWH(0, 0, size.width / 2, size.height),
      DropPosition.right =>
        Rect.fromLTWH(size.width / 2, 0, size.width / 2, size.height),
      DropPosition.above => Rect.fromLTWH(0, 0, size.width, size.height / 2),
      DropPosition.below =>
        Rect.fromLTWH(0, size.height / 2, size.width, size.height / 2),
    };
    canvas.drawRect(region, Paint()..color = accent.withValues(alpha: 0.35));
    canvas.drawRect(
      region,
      Paint()
        ..color = accent
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1,
    );
  }

  @override
  bool shouldRepaint(_DropPainter old) =>
      old.pos != pos || old.accent != accent;
}

/// The bare-pane drag grip (dock.rs::BARE_PANE_GRIP_SIZE / paint_bare_pane_grip):
/// a 16 px corner square of a 2×3 dot grid, muted at half alpha, brightening on
/// hover or drag, that drags the pane's panel exactly like a tab.
class _PaneGrip extends StatefulWidget {
  final Panel panel;
  final _DragController drag;
  const _PaneGrip({required this.panel, required this.drag});

  @override
  State<_PaneGrip> createState() => _PaneGripState();
}

class _PaneGripState extends State<_PaneGrip> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    return _DragSource(
      panel: widget.panel,
      drag: widget.drag,
      child: LumitTooltip(
        message: 'Drag to move this panel',
        child: MouseRegion(
          cursor: SystemMouseCursors.grab,
          onEnter: (_) => setState(() => _hover = true),
          onExit: (_) => setState(() => _hover = false),
          child: AnimatedBuilder(
            animation: widget.drag,
            builder: (context, _) {
              final t = ThemeScope.of(context).theme;
              final lit = _hover || widget.drag.dragged == widget.panel;
              return SizedBox(
                width: 16,
                height: 16,
                child: CustomPaint(
                  painter: _GripPainter(
                    lit ? t.textSecondary : t.textMuted.withValues(alpha: 0.5),
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}

class _GripPainter extends CustomPainter {
  final Color colour;
  const _GripPainter(this.colour);

  @override
  void paint(Canvas canvas, Size size) {
    const pad = 4.0;
    final inner = (Offset.zero & size).deflate(pad);
    final paint = Paint()..color = colour;
    for (var col = 0; col < 2; col++) {
      for (var row = 0; row < 3; row++) {
        final x = inner.left + col * inner.width;
        final y = inner.top + row * (inner.height / 2);
        canvas.drawCircle(Offset(x, y), 1, paint);
      }
    }
  }

  @override
  bool shouldRepaint(_GripPainter old) => old.colour != colour;
}
