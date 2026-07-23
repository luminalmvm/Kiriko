// The layer-outline degradation order (parity checklist post-parity item 3, an
// owner design change built into F3 from the start): as the outline column
// narrows, switches must drop out cleanly rather than overlap. Pure so the
// width→columns table can be unit tested.
//
// In plain terms: when the Timeline panel is wide, a layer row shows its whole
// switch cluster. As it shrinks, the least-important switches disappear first,
// in a fixed order, so nothing ever stacks on top of anything else. The type
// glyph and the name always survive; the eye survives longest of the switches.

/// Which outline controls fit at a given width. The type glyph and the name are
/// always drawn (not fields here); these flags gate the rest.
class OutlineColumns {
  final bool index;
  final bool eye;
  final bool speaker;
  final bool solo;
  final bool lock;
  final bool fx;
  final bool motionBlur;
  final bool threeD;
  final bool collapse;

  const OutlineColumns({
    required this.index,
    required this.eye,
    required this.speaker,
    required this.solo,
    required this.lock,
    required this.fx,
    required this.motionBlur,
    required this.threeD,
    required this.collapse,
  });

  static const none = OutlineColumns(
    index: false,
    eye: false,
    speaker: false,
    solo: false,
    lock: false,
    fx: false,
    motionBlur: false,
    threeD: false,
    collapse: false,
  );
}

/// One switch's identity in the priority walk.
enum _Col { eye, lock, indexNum, speaker, solo, fx, motionBlur, threeD, collapse }

// Widths (px) each column claims, gaps included, tuned to the 22 px row.
const double _kTabW = 3; // left colour tab
const double _kGlyphW = 18; // type glyph
const double _kNameMin = 30; // the ellipsised name never drops below this
const double _kSwitchW = 18;
const double _kIndexW = 14;
const double _kGap = 2;

/// The columns that fit in [availableWidth], dropping in the owner's order:
/// collapse → 3D → motion-blur → fx → solo → speaker → index → lock → eye.
/// The reverse of that is the keep-priority walked here. [canAudio] gates the
/// speaker (footage/sequence/precomp only, and — for footage — only when its
/// probed source actually carries an audio stream); [canVideo] gates the eye
/// (a footage layer whose probed source is audio-only has no picture to
/// show/hide); [isPrecomp] gates the collapse switch. A width too small for
/// even the eye yields glyph + name only.
OutlineColumns chooseColumns(
  double availableWidth, {
  required bool canAudio,
  required bool isPrecomp,
  bool canVideo = true,
}) {
  // Mandatory core: colour tab + glyph + a minimum name cell.
  var used = _kTabW + _kGlyphW + _kGap + _kNameMin;
  final on = <_Col>{};

  // Keep-priority, highest first (the reverse of the drop order).
  const order = [
    _Col.eye,
    _Col.lock,
    _Col.indexNum,
    _Col.speaker,
    _Col.solo,
    _Col.fx,
    _Col.motionBlur,
    _Col.threeD,
    _Col.collapse,
  ];

  for (final col in order) {
    if (col == _Col.eye && !canVideo) continue;
    if (col == _Col.speaker && !canAudio) continue;
    if (col == _Col.collapse && !isPrecomp) continue;
    final w = (col == _Col.indexNum ? _kIndexW : _kSwitchW) + _kGap;
    if (used + w <= availableWidth) {
      used += w;
      on.add(col);
    }
  }

  return OutlineColumns(
    index: on.contains(_Col.indexNum),
    eye: on.contains(_Col.eye),
    speaker: on.contains(_Col.speaker),
    solo: on.contains(_Col.solo),
    lock: on.contains(_Col.lock),
    fx: on.contains(_Col.fx),
    motionBlur: on.contains(_Col.motionBlur),
    threeD: on.contains(_Col.threeD),
    collapse: on.contains(_Col.collapse),
  );
}
