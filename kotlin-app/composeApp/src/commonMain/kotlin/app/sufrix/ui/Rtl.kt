package app.sufrix.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection

// Direction-aware chevron glyphs. Compose auto-mirrors layout (Row, Start/End,
// padding) under LocalLayoutDirection.Rtl, but PLAIN TEXT glyphs like "‹" / "›"
// are not mirrored — so a hardcoded back/disclosure chevron points the wrong way
// in Arabic. These helpers pick the glyph that matches the active direction.

/** True when the active layout direction is right-to-left (Arabic). */
@Composable
fun isRtlLayout(): Boolean = LocalLayoutDirection.current == LayoutDirection.Rtl

/** A back-navigation chevron pointing toward the leading edge: "‹" in LTR,
 *  "›" in RTL (where the leading edge is on the right). */
@Composable
fun backGlyph(): String = if (isRtlLayout()) "›" else "‹"

/** A trailing disclosure chevron pointing toward the trailing edge: "›" in LTR,
 *  "‹" in RTL. */
@Composable
fun disclosureGlyph(): String = if (isRtlLayout()) "‹" else "›"
