package app.sufrix.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
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
    autofocus: Boolean = false,
) {
    val c = sufrixColors()
    var text by remember { mutableStateOf(if (amountMinor == 0L) "" else minorToText(amountMinor)) }
    // The last value WE emitted — lets us tell an external change (a carried-over
    // prefill arriving async) from the teller's own typing, with no focus races.
    var lastEmitted by remember { mutableStateOf(amountMinor) }
    var focused by remember { mutableStateOf(false) }
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(autofocus) {
        if (autofocus) runCatching { focusRequester.requestFocus() }
    }
    LaunchedEffect(amountMinor) {
        if (amountMinor != lastEmitted) {
            text = if (amountMinor == 0L) "" else minorToText(amountMinor)
            lastEmitted = amountMinor
        }
    }

    // One contained row: a muted currency prefix on the left, the big tabular
    // amount filling the rest (was a tiny label stacked over a 34sp number).
    Row(
        modifier
            .fillMaxWidth()
            .height(64.dp)
            .clip(RoundedCornerShape(Radii.md))
            .background(c.surface)
            .border(
                if (focused) 2.dp else 1.dp,
                if (focused) c.accent else c.border,
                RoundedCornerShape(Radii.md),
            )
            .padding(horizontal = Space.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        Text(
            currencyCode.uppercase(),
            color = c.textMuted, fontFamily = SufrixFont,
            fontWeight = FontWeight.Bold, fontSize = 15.sp,
        )
        Box(Modifier.weight(1f).wrapContentHeight()) {
            if (text.isEmpty()) {
                Text(
                    "0.00", color = c.textMuted, fontFamily = SufrixFont,
                    fontWeight = FontWeight.Black, fontSize = 28.sp,
                )
            }
            BasicTextField(
                value = text,
                onValueChange = { newValue ->
                    text = newValue
                    val m = toMinor(newValue)
                    lastEmitted = m
                    onAmountMinor(m)
                },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().focusRequester(focusRequester).onFocusChanged { focused = it.isFocused },
                textStyle = TextStyle(
                    color = c.textPrimary, fontFamily = SufrixFont,
                    fontWeight = FontWeight.Black, fontSize = 28.sp,
                ),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                cursorBrush = SolidColor(c.accent),
            )
        }
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
