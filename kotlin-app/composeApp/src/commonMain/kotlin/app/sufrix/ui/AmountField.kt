package app.sufrix.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToLong

// A big, tabular money input — the opening-cash / tender hero field. Edits a
// major-unit decimal string and reports `amountMinor` (minor units). Mirror of
// the SwiftUI AmountField.
@Composable
fun AmountField(
    amountMinor: Long,
    onAmountMinor: (Long) -> Unit,
    currencyCode: String,
    modifier: Modifier = Modifier,
) {
    val c = sufrixColors()
    var text by remember { mutableStateOf(if (amountMinor == 0L) "" else minorToText(amountMinor)) }
    var focused by remember { mutableStateOf(false) }

    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radii.md))
            .background(c.surface)
            .border(
                if (focused) 2.dp else 1.dp,
                if (focused) c.accent else c.border,
                RoundedCornerShape(Radii.md),
            )
            .padding(vertical = Space.lg, horizontal = Space.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(
            currencyCode.uppercase(),
            color = c.textMuted, fontFamily = SufrixFont,
            fontWeight = FontWeight.SemiBold, fontSize = 12.sp,
        )
        BasicTextField(
            value = text,
            onValueChange = { newValue ->
                text = newValue
                onAmountMinor(toMinor(newValue))
            },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
            textStyle = TextStyle(
                color = c.textPrimary, fontFamily = SufrixFont,
                fontWeight = FontWeight.Black, fontSize = 34.sp, textAlign = TextAlign.Center,
            ),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
            cursorBrush = SolidColor(c.accent),
            decorationBox = { inner ->
                if (text.isEmpty()) {
                    Text(
                        "0.00", color = c.textMuted, fontFamily = SufrixFont,
                        fontWeight = FontWeight.Black, fontSize = 34.sp,
                        textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth(),
                    )
                }
                inner()
            },
        )
    }
}

/** Parse a major-unit decimal string ("500", "499.50") → minor units. */
private fun toMinor(s: String): Long {
    val cleaned = s.filter { it.isDigit() || it == '.' }
    val major = cleaned.toDoubleOrNull() ?: 0.0
    return (major * 100).roundToLong()
}

private fun minorToText(minor: Long): String {
    val major = minor / 100.0
    return if (major == major.toLong().toDouble()) major.toLong().toString()
    else ((major * 100).roundToLong() / 100.0).toString()
}
