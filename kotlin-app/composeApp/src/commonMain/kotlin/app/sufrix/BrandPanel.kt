package app.sufrix

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import app.sufrix.ui.Space
import app.sufrix.ui.SufrixFont
import app.sufrix.ui.SufrixLockup
import app.sufrix.ui.SufrixMark
import app.sufrix.ui.sufrixColors
import app.sufrix.ui.t

// The brand half of the wide (tablet / desktop) split — shared by Login and
// Open-Shift so the two screens read as one continuous onboarding act. Mirror of
// the SwiftUI BrandPanel.
@Composable
fun BrandPanel(modifier: Modifier = Modifier) {
    val c = sufrixColors()
    Box(modifier.background(c.surfaceAlt)) {
        SufrixMark(size = 360.dp, alpha = 0.05f)
        Column(Modifier.fillMaxSize().padding(48.dp)) {
            SufrixLockup(height = 28.dp)
            Spacer(Modifier.weight(1f))
            Text(t("brand.headline"), color = c.textPrimary, fontFamily = SufrixFont, fontWeight = FontWeight.Black, fontSize = 44.sp)
            Spacer(Modifier.height(Space.lg))
            Text(t("brand.tagline"), color = c.textSecondary, fontFamily = SufrixFont, fontSize = 15.sp, modifier = Modifier.widthIn(max = 300.dp))
            Spacer(Modifier.weight(1f))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                Box(Modifier.size(6.dp).clip(CircleShape).background(c.accent))
                Text("© 2026 Sufrix", color = c.textMuted, fontFamily = SufrixFont, fontSize = 12.sp)
            }
        }
    }
}
