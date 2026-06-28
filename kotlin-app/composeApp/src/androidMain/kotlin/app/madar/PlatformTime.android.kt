package app.madar

import java.time.Duration
import java.time.Instant

// minSdk 26 → java.time is available without desugaring.
actual fun minutesSince(rfc: String): Int = try {
    Duration.between(Instant.parse(rfc), Instant.now()).toMinutes().toInt().coerceAtLeast(0)
} catch (_: Exception) {
    0
}

actual fun isoDaysAgo(days: Long): String = Instant.now().minusSeconds(days * 86_400L).toString()
