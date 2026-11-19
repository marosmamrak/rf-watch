package com.marosmamrak.rfwatch.monitor

import android.Manifest
import android.app.Notification
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.car.app.notification.CarAppExtender
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.marosmamrak.rfwatch.MainActivity
import com.marosmamrak.rfwatch.R
import com.marosmamrak.rfwatch.model.Track
import java.util.Locale

object Notifications {
    const val CHANNEL_MONITOR = "monitor"
    const val CHANNEL_ALERTS = "drone_alerts"
    const val ID_MONITOR = 1
    const val ID_ALERT = 2

    fun ensureChannels(context: Context) {
        val nm = NotificationManagerCompat.from(context)
        nm.createNotificationChannel(
            NotificationChannelCompat.Builder(CHANNEL_MONITOR, NotificationManagerCompat.IMPORTANCE_LOW)
                .setName(context.getString(R.string.channel_monitor))
                .setDescription(context.getString(R.string.channel_monitor_desc))
                .setShowBadge(false)
                .build(),
        )
        nm.createNotificationChannel(
            NotificationChannelCompat.Builder(CHANNEL_ALERTS, NotificationManagerCompat.IMPORTANCE_HIGH)
                .setName(context.getString(R.string.channel_alerts))
                .setDescription(context.getString(R.string.channel_alerts_desc))
                .build(),
        )
    }

    fun canPost(context: Context): Boolean =
        NotificationManagerCompat.from(context).areNotificationsEnabled() &&
            (android.os.Build.VERSION.SDK_INT < 33 ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED)

    private fun openAppIntent(context: Context): PendingIntent = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_OPEN_NODE, true)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )

    /** One-line summary used by the ongoing notification. */
    fun summary(state: MonitorState): String = when {
        state.droneDetected -> "DRONE DETECTED"
        state.hasLiveData -> "clear · ${state.activeTracks.size} emitter(s)"
        state.connection == ConnectionState.Reconnecting -> "reconnecting…"
        else -> "connecting…"
    }

    fun monitor(context: Context, state: MonitorState): Notification {
        val stop = PendingIntent.getService(
            context,
            1,
            Intent(context, MonitorService::class.java).setAction(MonitorService.ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(context, CHANNEL_MONITOR)
            .setSmallIcon(R.drawable.ic_stat_drone)
            .setContentTitle("Monitoring ${state.node?.name ?: "node"}")
            .setContentText(summary(state))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .setContentIntent(openAppIntent(context))
            .addAction(0, "Stop", stop)
            .build()
    }

    fun droneText(track: Track): String = String.format(
        Locale.US,
        "%.1f MHz · %.0f dBm · %.0f%% confidence",
        track.centerMhz,
        track.peakDbm,
        track.confidence * 100,
    )

    /** Heads-up alert, also shown by Android Auto thanks to the [CarAppExtender]. */
    fun postDroneAlert(context: Context, nodeName: String, track: Track) {
        if (!canPost(context)) return
        val title = "Drone detected · $nodeName"
        val text = "Control link at ${droneText(track)}"
        val notification = NotificationCompat.Builder(context, CHANNEL_ALERTS)
            .setSmallIcon(R.drawable.ic_stat_drone)
            .setContentTitle(title)
            .setContentText(text)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setAutoCancel(true)
            .setContentIntent(openAppIntent(context))
            .extend(
                CarAppExtender.Builder()
                    .setContentTitle(title)
                    .setContentText(text)
                    .setSmallIcon(R.drawable.ic_stat_drone)
                    .setImportance(NotificationManagerCompat.IMPORTANCE_HIGH)
                    .build(),
            )
            .build()
        try {
            NotificationManagerCompat.from(context).notify(ID_ALERT, notification)
        } catch (_: SecurityException) {
            // Permission revoked between the check and the call.
        }
    }
}
