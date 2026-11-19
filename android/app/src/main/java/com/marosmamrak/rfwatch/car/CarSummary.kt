package com.marosmamrak.rfwatch.car

import com.marosmamrak.rfwatch.monitor.ConnectionState
import com.marosmamrak.rfwatch.monitor.MonitorState
import java.util.Locale

/**
 * Car-screen content as plain data (unit-testable without a car host).
 *
 * The layout is deliberately fixed: a constant title, a "Status" row and [TRACK_SLOTS]
 * "Emitter N" rows. A ListTemplate update only counts as a *refresh* (not a new step
 * against the host's 5-template quota) when the title, the number of rows and every row
 * title stay the same, so all dynamic values live in the row texts.
 *
 * [changeKey] holds only what the driver must see at once: the status row and which
 * emitters are listed (id + kind). Levels and confidence change every frame and are left
 * out, so the screen can refresh them slowly without hitting the host's rate limit.
 */
data class CarSummary(val title: String, val rows: List<CarRow>, val changeKey: List<Any> = emptyList()) {

    enum class Tone { Alert, Clear, Neutral }

    data class CarRow(val title: String, val lines: List<String>, val tone: Tone)

    companion object {
        const val TITLE = "rf-watch"
        const val TRACK_SLOTS = 4

        fun from(state: MonitorState): CarSummary {
            val node = state.node
            val conn = when {
                node == null -> "Not monitoring: pick a node in the phone app"
                state.connection == ConnectionState.Connected -> "${node.name} · connected"
                state.connection == ConnectionState.Reconnecting -> "${node.name} · reconnecting"
                else -> "${node.name} · connecting"
            }
            val status = when {
                state.droneDetected -> CarRow("Status", listOf("DRONE DETECTED", conn), Tone.Alert)
                state.hasLiveData -> CarRow("Status", listOf("Clear", conn), Tone.Clear)
                else -> CarRow("Status", listOf("No data", conn), Tone.Neutral)
            }

            val active = if (state.hasLiveData) state.activeTracks else emptyList()
            val slots = (0 until TRACK_SLOTS).map { i ->
                val title = "Emitter ${i + 1}"
                val track = active.getOrNull(i)
                when {
                    track == null -> CarRow(title, listOf("—"), Tone.Neutral)
                    else -> {
                        val more = if (i == TRACK_SLOTS - 1 && active.size > TRACK_SLOTS) {
                            " · +${active.size - TRACK_SLOTS} more"
                        } else {
                            ""
                        }
                        CarRow(
                            title,
                            listOf(
                                String.format(Locale.US, "%s · %.1f MHz", track.kindLabel, track.centerMhz),
                                String.format(
                                    Locale.US,
                                    "%.0f dBm · %.0f%% confidence%s",
                                    track.peakDbm,
                                    track.confidence * 100,
                                    more,
                                ),
                            ),
                            if (track.isDroneLink) Tone.Alert else Tone.Neutral,
                        )
                    }
                }
            }
            val key = listOf<Any>(status.tone, status.lines) + active.take(TRACK_SLOTS).map { it.id to it.kind } +
                listOf(active.size > TRACK_SLOTS)
            return CarSummary(TITLE, listOf(status) + slots, key)
        }
    }
}
