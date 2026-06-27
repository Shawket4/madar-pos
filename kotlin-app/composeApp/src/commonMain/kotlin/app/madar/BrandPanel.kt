package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.ui.Space
import app.madar.ui.LocalMadarFont
import app.madar.ui.MadarLockup
import app.madar.ui.MadarMark
import app.madar.ui.madarColors
import app.madar.ui.t

// The brand half of the wide (tablet / desktop) split — shared by Login and
// Open-Shift so the two screens read as one continuous onboarding act. Mirror of
// the SwiftUI BrandPanel.
@Composable
fun BrandPanel(modifier: Modifier = Modifier) {
    val c = madarColors()
    val rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    Box(modifier.background(c.surfaceAlt)) {
        // Faded watermark mark — offset like the SwiftUI BrandPanel, mirrored in RTL.
        Box(Modifier.offset(x = if (rtl) (-80).dp else 80.dp, y = 80.dp)) {
            MadarMark(size = 360.dp, alpha = 0.05f)
        }
        Column(Modifier.fillMaxSize().padding(48.dp)) {
            MadarLockup(height = 28.dp)
            Spacer(Modifier.weight(1f))
            Text(t("brand.headline"), color = c.textPrimary, fontFamily = LocalMadarFont.current, fontWeight = FontWeight.Black, fontSize = 44.sp)
            Spacer(Modifier.height(Space.lg))
            Text(t("brand.tagline"), color = c.textSecondary, fontFamily = LocalMadarFont.current, fontSize = 15.sp, modifier = Modifier.widthIn(max = 300.dp))
            Spacer(Modifier.weight(1f))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Box(Modifier.size(6.dp).clip(CircleShape).background(c.accent))
                Text("© 2026 Madar", color = c.textMuted, fontFamily = LocalMadarFont.current, fontSize = 12.sp)
            }
        }
    }
}
