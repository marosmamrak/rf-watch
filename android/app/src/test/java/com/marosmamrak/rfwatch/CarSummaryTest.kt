package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.Fixtures.track
import com.marosmamrak.rfwatch.car.CarSummary
import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.model.NodeAddress
import com.marosmamrak.rfwatch.model.Track
import com.marosmamrak.rfwatch.monitor.ClientEvent
import com.marosmamrak.rfwatch.monitor.MonitorState
import com.marosmamrak.rfwatch.monitor.reduce
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class CarSummaryTest {

    private val node = NodeAddress("rf-watch", "10.0.2.2", 8787)

    private fun live(vararg tracks: Track) = MonitorState(node = node)
        .reduce(ClientEvent.Open)
        .reduce(ClientEvent.Message(Frame(seq = 7, tracks = tracks.toList())))

    /** Car host rule: same title + same row count/titles = refresh, not a new step. */
    @Test
    fun layoutIsStableAcrossStates() {
        val states = listOf(
            MonitorState(),
            MonitorState(node = node),
            live(),
            live(track(1)),
            live(*(1L..9L).map { track(it) }.toTypedArray()),
            live(track(1)).reduce(ClientEvent.Disconnected("x")),
        )
        val shapes = states.map { s -> CarSummary.from(s).let { it.title to it.rows.map { r -> r.title } } }.toSet()
        assertEquals(1, shapes.size)
        assertEquals(1 + CarSummary.TRACK_SLOTS, shapes.single().second.size)
    }

    @Test
    fun statusRowShowsDroneOrClear() {
        assertEquals("DRONE DETECTED", CarSummary.from(live(track(1))).rows[0].lines[0])
        assertEquals(CarSummary.Tone.Alert, CarSummary.from(live(track(1))).rows[0].tone)
        assertEquals("Clear", CarSummary.from(live(track(2, kind = Track.KIND_WIDEBAND))).rows[0].lines[0])
        assertEquals("No data", CarSummary.from(MonitorState(node = node)).rows[0].lines[0])
    }

    @Test
    fun emitterRowsListActiveTracks() {
        val rows = CarSummary.from(live(track(1, kind = Track.KIND_WIDEBAND, peak = -40f), track(2))).rows
        assertEquals(listOf("FHSS control · 2441.8 MHz", "-61 dBm · 90% confidence"), rows[1].lines)
        assertTrue(rows[2].lines[0].startsWith("Wideband"))
        assertEquals(listOf("—"), rows[3].lines)
    }

    @Test
    fun overflowIsSummarisedInLastSlot() {
        val rows = CarSummary.from(live(*(1L..6L).map { track(it) }.toTypedArray())).rows
        assertTrue(rows.last().lines[1].endsWith("+2 more"))
    }
}
