package app.madar

import java.time.Duration
import java.time.Instant

actual fun minutesSince(rfc: String): Int = try {
    Duration.between(Instant.parse(rfc), Instant.now()).toMinutes().toInt().coerceAtLeast(0)
} catch (_: Exception) {
    0
}
