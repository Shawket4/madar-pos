package app.sufrix.ui

import kotlin.math.absoluteValue
import kotlin.math.roundToLong

// Money formatting — minor units → a display string. Identical to the Swift
// `Money` so totals read the same on both platforms.
object Money {
    /** "EGP 12.50" — [minor] units rendered with two decimals and the code. */
    fun format(minor: Long, code: String): String {
        val neg = minor < 0
        val cents = minor.absoluteValue
        val whole = cents / 100
        val frac = (cents % 100).toString().padStart(2, '0')
        val amount = "${if (neg) "-" else ""}$whole.$frac"
        val c = code.uppercase()
        return if (c.isEmpty()) amount else "$c $amount"
    }
}

// Round-trip helper kept here so both the amount field and money display agree.
internal fun Double.toMinorUnits(): Long = (this * 100).roundToLong()
