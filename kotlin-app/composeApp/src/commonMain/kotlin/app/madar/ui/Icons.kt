package app.madar.ui

import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Backspace
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.ListAlt
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowCircleRight
import androidx.compose.material.icons.filled.Calculate
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.DirectionsBike
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.GridView
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.QrCode2
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.ShoppingBag
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material.icons.filled.Verified
import androidx.compose.material.icons.filled.WifiOff
import androidx.compose.material.icons.outlined.AccountBalance
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.AccountCircle
import androidx.compose.material.icons.outlined.Business
import androidx.compose.material.icons.outlined.ChatBubbleOutline
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.CloudUpload
import androidx.compose.material.icons.outlined.CreditCard
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Feedback
import androidx.compose.material.icons.outlined.Inbox
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.LockOpen
import androidx.compose.material.icons.outlined.MailOutline
import androidx.compose.material.icons.outlined.MoveToInbox
import androidx.compose.material.icons.outlined.PanTool
import androidx.compose.material.icons.outlined.Payments
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.PlayCircle
import androidx.compose.material.icons.outlined.Print
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.ShoppingCart
import androidx.compose.material.icons.outlined.Storefront
import androidx.compose.material.icons.outlined.Tag
import androidx.compose.material.icons.filled.Close
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.material3.Icon
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import org.jetbrains.compose.resources.painterResource

/**
 * Maps a SwiftUI SF Symbol NAME (the same strings the SwiftUI app passes to its
 * components, e.g. "printer", "checkmark.circle", "person") to the closest
 * Material vector, so the Compose shared components render real icons at parity
 * with SwiftUI instead of Unicode glyphs. Returns null for an unmapped name —
 * callers then render nothing rather than a wrong icon. Keep this in sync with
 * the SF Symbol names used across SwiftUI (`grep systemName:`).
 */
fun sfSymbol(name: String): ImageVector? = when (name) {
    "exclamationmark.circle" -> Icons.Outlined.ErrorOutline
    "exclamationmark.triangle" -> Icons.Outlined.ErrorOutline
    "exclamationmark.bubble" -> Icons.Outlined.Feedback
    "xmark" -> Icons.Filled.Close
    "xmark.circle", "xmark.circle.fill" -> Icons.Filled.Cancel
    "printer" -> Icons.Outlined.Print
    "number", "tag", "tag.fill" -> Icons.Outlined.Tag
    "chevron.backward" -> Icons.AutoMirrored.Filled.KeyboardArrowLeft
    "chevron.forward", "chevron.right" -> Icons.AutoMirrored.Filled.KeyboardArrowRight
    "chevron.down" -> Icons.Filled.KeyboardArrowDown
    "chevron.up" -> Icons.Filled.KeyboardArrowUp
    "checkmark.circle" -> Icons.Outlined.CheckCircle
    "checkmark" -> Icons.Filled.Check
    "checkmark.seal" -> Icons.Filled.Verified
    "trash", "trash.fill" -> Icons.Outlined.Delete
    "text.bubble" -> Icons.Outlined.ChatBubbleOutline
    "person" -> Icons.Outlined.Person
    "person.fill" -> Icons.Filled.Person
    "person.crop.circle.badge.clock" -> Icons.Outlined.AccountCircle
    "magnifyingglass" -> Icons.Filled.Search
    "list.bullet.rectangle" -> Icons.AutoMirrored.Filled.ListAlt
    "building.2" -> Icons.Outlined.Business
    "plus" -> Icons.Filled.Add
    "plus.forwardslash.minus" -> Icons.Filled.Calculate
    "lock", "lock.circle" -> Icons.Outlined.Lock
    "lock.open" -> Icons.Outlined.LockOpen
    "clock.arrow.circlepath" -> Icons.Filled.History
    "clock", "clock.badge.checkmark", "clock.badge.exclamationmark" -> Icons.Outlined.Schedule
    "tray.full", "tray" -> Icons.Outlined.Inbox
    "tray.and.arrow.down" -> Icons.Outlined.MoveToInbox
    "rectangle.portrait.and.arrow.right" -> Icons.AutoMirrored.Filled.Logout
    "note.text", "doc.text" -> Icons.Outlined.Description
    "icloud.and.arrow.up" -> Icons.Outlined.CloudUpload
    "gearshape" -> Icons.Outlined.Settings
    "bicycle" -> Icons.Filled.DirectionsBike
    "banknote" -> Icons.Outlined.Payments
    "wifi.slash" -> Icons.Filled.WifiOff
    "storefront" -> Icons.Outlined.Storefront
    "square.stack.3d.up.fill" -> Icons.Filled.Layers
    "square.grid.2x2.fill" -> Icons.Filled.GridView
    "heart.circle" -> Icons.Filled.Favorite
    "hand.raised" -> Icons.Outlined.PanTool
    "envelope" -> Icons.Outlined.MailOutline
    "ellipsis.circle", "ellipsis" -> Icons.Filled.MoreHoriz
    "delete.left" -> Icons.AutoMirrored.Filled.Backspace
    "creditcard" -> Icons.Outlined.CreditCard
    "cart" -> Icons.Outlined.ShoppingCart
    "bag.fill" -> Icons.Filled.ShoppingBag
    "arrow.triangle.2.circlepath" -> Icons.Filled.Sync
    "arrow.right.circle" -> Icons.Filled.ArrowCircleRight
    "arrow.clockwise" -> Icons.Filled.Refresh
    // Payment-method + sync-op extras (PayChip / SyncScreen), matching SwiftUI's
    // PayChip.symbol and SyncView.opIcon mappings.
    "wallet" -> Icons.Outlined.AccountBalanceWallet
    "bank", "building.columns" -> Icons.Outlined.AccountBalance
    "qrcode", "qr" -> Icons.Filled.QrCode2
    "play.circle" -> Icons.Outlined.PlayCircle
    else -> null
}

/** Render a shared Lucide icon (by SF-Symbol name) tinted + sized — pixel-identical
 *  to the SwiftUI app, which renders the SAME Lucide asset. No-op when the name is
 *  unmapped. Prefer the `MadarIcon` name in new code; `SfIcon` is the legacy alias
 *  the existing call sites use. Falls back to the Material vector (sfSymbol) only if
 *  the Lucide catalog has no entry, so nothing silently vanishes during migration. */
/** Directional glyphs flip in RTL so chevrons/back-arrows point the right way
 *  (parity with Swift MadarIcon.mirror). */
private val iconMirror = mapOf(
    "chevron.right" to "chevron.left", "chevron.left" to "chevron.right",
    "chevron.forward" to "chevron.backward", "chevron.backward" to "chevron.forward",
)

@Composable
fun MadarIcon(name: String?, tint: Color, size: Dp = IconSize.md, modifier: Modifier = Modifier) {
    if (name == null) return
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val resolved = if (rtl) iconMirror[name] ?: name else name
    val lucide = lucideRes(resolved)
    if (lucide != null) {
        Icon(painterResource(lucide), contentDescription = null, tint = tint, modifier = modifier.size(size))
        return
    }
    val vector = sfSymbol(resolved) ?: return
    Icon(vector, contentDescription = null, tint = tint, modifier = modifier.size(size))
}

/** Legacy alias — same shared Lucide rendering. */
@Composable
fun SfIcon(name: String?, tint: Color, size: Dp = IconSize.md, modifier: Modifier = Modifier) =
    MadarIcon(name, tint, size, modifier)
