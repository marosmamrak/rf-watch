package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.model.Track

/** Messages in the exact shape the Rust server sends (see README "Protocol"). */
object Fixtures {
    const val HELLO =
        """{"type":"hello","device":"rf-watch","version":"0.1.0","band":{"start_mhz":2400.0,"stop_mhz":2483.5,"bins":256,"fps":10}}"""

    fun frame(seq: Long, vararg tracks: String) =
        """{"type":"frame","seq":$seq,"t_ms":${seq * 100},"noise_floor_dbm":-96.8,"threshold_dbm":-86.8,""" +
            """"power_dbm":[-95.9,-97.1,-61.2],"tracks":[${tracks.joinToString(",")}]}"""

    fun trackJson(id: Long, kind: String, active: Boolean, peak: Double = -61.2) =
        """{"id":$id,"kind":"$kind","center_mhz":2441.8,"bandwidth_mhz":72.0,"peak_dbm":$peak,""" +
            """"confidence":0.9,"first_seen_ms":10100,"last_seen_ms":81200,"active":$active}"""

    fun track(id: Long, kind: String = Track.KIND_FHSS_CONTROL, active: Boolean = true, peak: Float = -61.2f) =
        Track(id = id, kind = kind, centerMhz = 2441.8f, peakDbm = peak, confidence = 0.9f, active = active)
}
