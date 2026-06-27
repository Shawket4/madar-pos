package app.madar

import app.madar.core.RealtimePlayer

/**
 * Desktop (JVM) realtime alerts — the pure platform primitive the CORE calls. A
 * desktop till is attended, so a system beep is enough; there's no portable native
 * notification/haptic without extra deps, and the in-app UI carries the rest. The
 * core still owns ALL the policy (which events, dedup, localized text).
 */
class DesktopRealtimePlayer : RealtimePlayer {
    override fun playPing() {
        runCatching { java.awt.Toolkit.getDefaultToolkit().beep() }
    }

    override fun postNotification(title: String, body: String, tag: String) {
        // No portable OS notification on desktop — the in-app board + beep suffice.
    }

    override fun haptic() {
        // No haptic on desktop.
    }
}
