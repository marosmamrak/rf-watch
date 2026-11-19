package com.marosmamrak.rfwatch.monitor

import android.annotation.SuppressLint
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.model.NodeAddress
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

/**
 * Foreground service (type `connectedDevice`) that keeps the WebSocket to the selected node
 * open while the app is in the background or the screen is off, publishes the stream into
 * [NodeRepository] and posts a notification for every new drone control link.
 */
class MonitorService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val http = OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(10, TimeUnit.SECONDS) // detects a dead node / Wi-Fi drop
        .build()
    private val alerts = DroneAlertTracker()
    private var client: NodeClient? = null
    private var node: NodeAddress? = null
    private var notificationJob: Job? = null
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            shutdown()
            return START_NOT_STICKY
        }
        val target = intent?.toNode() ?: NodeRepository.state.value.node
        if (target == null) {
            shutdown()
            return START_NOT_STICKY
        }
        Notifications.ensureChannels(this)
        NodeRepository.select(target)
        try {
            ServiceCompat.startForeground(
                this,
                Notifications.ID_MONITOR,
                Notifications.monitor(this, NodeRepository.state.value),
                if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0,
            )
        } catch (e: Exception) {
            // E.g. ForegroundServiceStartNotAllowedException when restarted from the background.
            Log.w(TAG, "cannot start foreground", e)
            shutdown()
            return START_NOT_STICKY
        }
        connect(target)
        // Redeliver: if the process is killed, come back to the same node.
        return START_REDELIVER_INTENT
    }

    private fun connect(target: NodeAddress) {
        if (target == node && client != null) return
        client?.stop()
        alerts.reset()
        node = target
        acquireWakeLock()
        client = NodeClient(http, target.wsUrl, scope) { event ->
            NodeRepository.onEvent(target, event)
            val frame = (event as? ClientEvent.Message)?.msg as? Frame ?: return@NodeClient
            for (track in alerts.newDroneLinks(frame.tracks)) {
                Notifications.postDroneAlert(this, target.name, track)
            }
        }.also { it.start() }

        notificationJob?.cancel()
        notificationJob = scope.launch {
            NodeRepository.state
                .distinctUntilChangedBy { Notifications.summary(it) + it.node }
                .collect { state ->
                    if (state.node != target) return@collect
                    if (Notifications.canPost(this@MonitorService)) {
                        try {
                            NotificationManagerCompat.from(this@MonitorService)
                                .notify(Notifications.ID_MONITOR, Notifications.monitor(this@MonitorService, state))
                        } catch (_: SecurityException) {
                        }
                    }
                }
        }
    }

    // Frames arrive at 10 Hz; keep the CPU awake while monitoring so the socket and the
    // ping timer keep running with the screen off. Released as soon as monitoring stops.
    @SuppressLint("WakelockTimeout")
    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return
        wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "rfwatch:monitor")
            .apply {
                setReferenceCounted(false)
                acquire()
            }
    }

    private fun shutdown() {
        client?.stop()
        client = null
        node = null
        notificationJob?.cancel()
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
        NodeRepository.select(null)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        client?.stop()
        wakeLock?.let { if (it.isHeld) it.release() }
        scope.cancel()
        http.dispatcher.executorService.shutdown()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "MonitorService"
        const val ACTION_START = "com.marosmamrak.rfwatch.START"
        const val ACTION_STOP = "com.marosmamrak.rfwatch.STOP"
        private const val EXTRA_NAME = "name"
        private const val EXTRA_HOST = "host"
        private const val EXTRA_PORT = "port"
        private const val EXTRA_WS = "ws"
        private const val EXTRA_VERSION = "version"

        /** Start (or switch) background monitoring of [node]. Call from the foreground UI. */
        fun start(context: Context, node: NodeAddress) {
            NodeRepository.select(node)
            val intent = Intent(context, MonitorService::class.java)
                .setAction(ACTION_START)
                .putExtra(EXTRA_NAME, node.name)
                .putExtra(EXTRA_HOST, node.host)
                .putExtra(EXTRA_PORT, node.port)
                .putExtra(EXTRA_WS, node.wsPath)
                .putExtra(EXTRA_VERSION, node.version)
            ContextCompat.startForegroundService(context, intent)
        }

        fun stop(context: Context) {
            NodeRepository.select(null)
            context.startService(Intent(context, MonitorService::class.java).setAction(ACTION_STOP))
        }

        private fun Intent.toNode(): NodeAddress? {
            val host = getStringExtra(EXTRA_HOST) ?: return null
            return NodeAddress(
                name = getStringExtra(EXTRA_NAME) ?: host,
                host = host,
                port = getIntExtra(EXTRA_PORT, NodeAddress.DEFAULT_PORT),
                wsPath = getStringExtra(EXTRA_WS) ?: "/ws",
                version = getStringExtra(EXTRA_VERSION),
            )
        }
    }
}
