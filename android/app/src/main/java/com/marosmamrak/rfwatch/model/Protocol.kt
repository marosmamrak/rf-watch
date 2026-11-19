package com.marosmamrak.rfwatch.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

/**
 * Messages pushed by the rf-watch node on `/ws` (see `src/protocol.rs`), tagged by `type`.
 * Only the fields the app uses are modelled; everything else (e.g. `power_dbm`) is ignored.
 */
@Serializable
sealed interface ServerMsg

@Serializable
data class Band(
    @SerialName("start_mhz") val startMhz: Float,
    @SerialName("stop_mhz") val stopMhz: Float,
    val bins: Int = 0,
    val fps: Int = 0,
)

@Serializable
@SerialName("hello")
data class Hello(
    val device: String,
    val version: String,
    val band: Band? = null,
) : ServerMsg

@Serializable
@SerialName("frame")
data class Frame(
    val seq: Long,
    @SerialName("t_ms") val tMs: Long = 0,
    @SerialName("noise_floor_dbm") val noiseFloorDbm: Float? = null,
    @SerialName("threshold_dbm") val thresholdDbm: Float? = null,
    val tracks: List<Track> = emptyList(),
) : ServerMsg

@Serializable
data class Track(
    val id: Long,
    /** `fhss_control` or `wideband`; kept as a string so new kinds don't break parsing. */
    val kind: String,
    @SerialName("center_mhz") val centerMhz: Float,
    @SerialName("bandwidth_mhz") val bandwidthMhz: Float = 0f,
    @SerialName("peak_dbm") val peakDbm: Float,
    /** 0..1 */
    val confidence: Float = 0f,
    @SerialName("first_seen_ms") val firstSeenMs: Long = 0,
    @SerialName("last_seen_ms") val lastSeenMs: Long = 0,
    val active: Boolean = false,
) {
    /** An active frequency-hopping control link: what the node reports as a drone. */
    val isDroneLink: Boolean get() = active && kind == KIND_FHSS_CONTROL

    val kindLabel: String
        get() = when (kind) {
            KIND_FHSS_CONTROL -> "FHSS control"
            KIND_WIDEBAND -> "Wideband"
            else -> kind
        }

    companion object {
        const val KIND_FHSS_CONTROL = "fhss_control"
        const val KIND_WIDEBAND = "wideband"
    }
}

/** Same rule as the node's `/api/status` `drone_detected`. */
fun droneDetected(tracks: List<Track>): Boolean = tracks.any { it.isDroneLink }

object Protocol {
    val json = Json {
        ignoreUnknownKeys = true
        classDiscriminator = "type"
    }

    /** Parses one WebSocket text message; `null` for malformed or unknown messages. */
    fun parse(text: String): ServerMsg? = try {
        json.decodeFromString(ServerMsg.serializer(), text)
    } catch (_: SerializationException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }
}
