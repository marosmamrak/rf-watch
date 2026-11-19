package com.marosmamrak.rfwatch.monitor

import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.model.Hello
import com.marosmamrak.rfwatch.model.NodeAddress
import com.marosmamrak.rfwatch.model.Track
import com.marosmamrak.rfwatch.model.droneDetected

enum class ConnectionState { Idle, Connecting, Connected, Reconnecting }

/** Everything the phone UI, the notification and the car screen show about the monitored node. */
data class MonitorState(
    val node: NodeAddress? = null,
    val connection: ConnectionState = ConnectionState.Idle,
    val device: String? = null,
    val version: String? = null,
    val tracks: List<Track> = emptyList(),
    val lastSeq: Long = -1,
    val lastError: String? = null,
) {
    /** True only when data is live: a disconnected node is "unknown", never "clear". */
    val hasLiveData: Boolean get() = connection == ConnectionState.Connected && lastSeq >= 0

    val droneDetected: Boolean get() = hasLiveData && droneDetected(tracks)

    /** Active tracks, drone links first, then strongest first. */
    val activeTracks: List<Track>
        get() = tracks.filter { it.active }
            .sortedWith(compareByDescending<Track> { it.isDroneLink }.thenByDescending { it.peakDbm })

    val strongestDroneLink: Track? get() = activeTracks.firstOrNull { it.isDroneLink }
}

/** What the WebSocket client reports. */
sealed interface ClientEvent {
    data class Connecting(val attempt: Int) : ClientEvent
    data object Open : ClientEvent
    data class Message(val msg: com.marosmamrak.rfwatch.model.ServerMsg) : ClientEvent
    data class Disconnected(val reason: String) : ClientEvent
}

fun MonitorState.reduce(event: ClientEvent): MonitorState = when (event) {
    is ClientEvent.Connecting -> copy(
        connection = if (event.attempt == 0) ConnectionState.Connecting else ConnectionState.Reconnecting,
    )
    ClientEvent.Open -> copy(connection = ConnectionState.Connected, lastError = null)
    is ClientEvent.Message -> when (val msg = event.msg) {
        is Hello -> copy(connection = ConnectionState.Connected, device = msg.device, version = msg.version)
        is Frame -> copy(connection = ConnectionState.Connected, tracks = msg.tracks, lastSeq = msg.seq)
    }
    // Stale tracks must not keep showing "DRONE DETECTED" (or "clear") while offline.
    is ClientEvent.Disconnected -> copy(
        connection = ConnectionState.Reconnecting,
        tracks = emptyList(),
        lastSeq = -1,
        lastError = event.reason,
    )
}

/**
 * Decides which drone links deserve a notification: each active `fhss_control` track id
 * alerts once. Ids that disappear from the frames are forgotten, so a new track alerts again.
 * Reconnecting to the same node keeps its ids, so a reconnect does not repeat the alert.
 */
class DroneAlertTracker {
    private val alerted = mutableSetOf<Long>()

    @Synchronized
    fun newDroneLinks(tracks: List<Track>): List<Track> {
        alerted.retainAll(tracks.map { it.id }.toSet())
        val fresh = tracks.filter { it.isDroneLink && it.id !in alerted }
        alerted += fresh.map { it.id }
        return fresh
    }

    @Synchronized
    fun reset() = alerted.clear()
}

/** Reconnect delay: 0.5 s, 1 s, 2 s ... capped at [maxMs]. */
fun backoffMillis(attempt: Int, baseMs: Long = 500, maxMs: Long = 15_000): Long {
    val shift = attempt.coerceIn(0, 20)
    return (baseMs shl shift).coerceAtMost(maxMs)
}
