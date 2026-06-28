package app.madar

// Wall-clock age of an RFC3339 / ISO-8601 timestamp, in whole minutes, by the
// DEVICE clock — parity with Swift's `Date().timeIntervalSince(...)` used for the
// KDS ticket-age SLA cue. Returns 0 if unparseable or in the future. Host-side
// (not core) so it matches the Swift host exactly; both targets are JVM (minSdk
// 26 → java.time is available), so the two actuals are identical.
expect fun minutesSince(rfc: String): Int

/** RFC3339 timestamp for (now − [days]) — for the all-orders search date presets.
 *  days=0 → now. The server treats the search `from` as an inclusive lower bound. */
expect fun isoDaysAgo(days: Long): String
