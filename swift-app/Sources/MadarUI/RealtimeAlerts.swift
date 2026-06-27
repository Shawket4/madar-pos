// The core-driven realtime ALERT player. The CORE decides WHEN to alert (which
// events, deduped) and builds the localized title/body — this class is the pure
// platform primitive it calls: play the bundled ping, post an OS local notification,
// fire a haptic. No decision logic lives here (the host is just a UI/platform layer).
import AVFoundation
import UserNotifications
#if os(iOS)
import UIKit
#endif

// NSObject so it can be the `UNUserNotificationCenterDelegate` (an @objc protocol).
final class RealtimeAlertPlayer: NSObject, RealtimePlayer, UNUserNotificationCenterDelegate {
    private var audio: AVAudioPlayer?

    override init() {
        super.init()
        #if os(iOS)
        // `.playback` so the ping is audible even with the ringer-silent switch on
        // (a kitchen/till must still hear new work); mix so it doesn't duck other audio.
        try? AVAudioSession.sharedInstance().setCategory(.playback, options: [.mixWithOthers])
        try? AVAudioSession.sharedInstance().setActive(true)
        #endif
        if let url = Bundle.main.url(forResource: "new_order", withExtension: "wav") {
            audio = try? AVAudioPlayer(contentsOf: url)
            audio?.prepareToPlay()
        }
        // CRITICAL — become the notification-center delegate so SSE-driven local
        // notifications actually PRESENT while the app is in the foreground. A POS
        // terminal (till / KDS / waiter) is almost always the frontmost app, and
        // iOS suppresses local-notification banners in the foreground UNLESS the
        // delegate opts in via `willPresent`. Without this the core posts the
        // request successfully but nothing ever appears on screen — the reported
        // "SSE-channel notifications don't go through on iOS" bug. The center holds
        // the delegate weakly; this player is retained for the app's lifetime by
        // AppModel, so the reference stays valid.
        UNUserNotificationCenter.current().delegate = self
    }

    /// Ask for local-notification permission once after login (best-effort; a denial
    /// just means no banners — the ping + in-app toast still fire).
    static func requestAuthorization() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    // ── RealtimePlayer (called by the core's AlertingListener) ──────────────────

    func playPing() {
        DispatchQueue.main.async { [weak self] in
            self?.audio?.currentTime = 0
            self?.audio?.play()
        }
    }

    func postNotification(title: String, body: String, tag: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        if !body.isEmpty { content.body = body }
        content.sound = .default
        // `tag` is the request identifier → re-posting the same entity REPLACES it,
        // so a delivery order's create→update never stacks duplicate banners.
        let req = UNNotificationRequest(identifier: tag, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(req)
    }

    func haptic() { Haptics.success() }

    // ── UNUserNotificationCenterDelegate ────────────────────────────────────────

    /// Present SSE alerts as a banner + sound EVEN WHEN THE APP IS FOREGROUND.
    /// iOS defaults to suppressing local notifications while the posting app is
    /// active; a POS is the active app, so without this the kitchen/till never
    /// sees the banner. `.list` keeps it in Notification Center; `.badge` mirrors
    /// the authorization options.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .list, .sound, .badge])
    }
}
