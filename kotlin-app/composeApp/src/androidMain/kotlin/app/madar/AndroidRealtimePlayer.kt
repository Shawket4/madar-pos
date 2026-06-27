package app.madar

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.media.MediaPlayer
import android.os.VibrationEffect
import android.os.Vibrator
import app.madar.core.RealtimePlayer

/**
 * Android realtime alerts — the pure platform primitive the CORE calls: play the
 * bundled ping (res/raw/new_order), post a high-importance OS notification (deduped
 * by `tag`), and a short vibration. The CORE decides WHEN (which events, dedup,
 * localized title/body); this class holds no policy. minSdk 26 → channel + platform
 * Notification.Builder are always available (no androidx-core needed).
 */
class AndroidRealtimePlayer(private val context: Context) : RealtimePlayer {
    private val channelId = "madar_realtime"

    init {
        context.getSystemService(NotificationManager::class.java)?.createNotificationChannel(
            NotificationChannel(channelId, "Madar alerts", NotificationManager.IMPORTANCE_HIGH).apply {
                enableVibration(true)
            },
        )
    }

    override fun playPing() {
        runCatching {
            MediaPlayer.create(context, R.raw.new_order)?.apply {
                setOnCompletionListener { it.release() }
                start()
            }
        }
    }

    override fun postNotification(title: String, body: String, tag: String) {
        runCatching {
            val nm = context.getSystemService(NotificationManager::class.java) ?: return
            val n = Notification.Builder(context, channelId)
                .setSmallIcon(R.mipmap.ic_launcher)
                .setContentTitle(title)
                .apply { if (body.isNotEmpty()) setContentText(body) }
                .setAutoCancel(true)
                .build()
            // `tag.hashCode()` as the id → re-posting the same entity REPLACES it.
            nm.notify(tag.hashCode(), n)
        }
    }

    override fun haptic() {
        runCatching {
            context.getSystemService(Vibrator::class.java)
                ?.vibrate(VibrationEffect.createOneShot(40, VibrationEffect.DEFAULT_AMPLITUDE))
        }
    }
}
