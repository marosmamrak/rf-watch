package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.Fixtures.track
import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.model.Hello
import com.marosmamrak.rfwatch.model.NodeAddress
import com.marosmamrak.rfwatch.model.Track
import com.marosmamrak.rfwatch.monitor.ClientEvent
import com.marosmamrak.rfwatch.monitor.ConnectionState
import com.marosmamrak.rfwatch.monitor.DroneAlertTracker
import com.marosmamrak.rfwatch.monitor.MonitorState
import com.marosmamrak.rfwatch.monitor.backoffMillis
import com.marosmamrak.rfwatch.monitor.reduce
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MonitorStateTest {

    private val node = NodeAddress("rf-watch", "10.0.2.2", 8787)

    private fun connected(vararg tracks: Track) = MonitorState(node = node)
        .reduce(ClientEvent.Connecting(0))
        .reduce(ClientEvent.Open)
        .reduce(ClientEvent.Message(Hello("rf-watch", "0.1.0")))
        .reduce(ClientEvent.Message(Frame(seq = 1, tracks = tracks.toList())))

    @Test
    fun connectionLifecycle() {
        var s = MonitorState(node = node).reduce(ClientEvent.Connecting(0))
        assertEquals(ConnectionState.Connecting, s.connection)
        s = s.reduce(ClientEvent.Open).reduce(ClientEvent.Message(Hello("rf-watch", "0.1.0")))
        assertEquals(ConnectionState.Connected, s.connection)
        assertEquals("0.1.0", s.version)
        assertFalse("no frame yet: not live", s.hasLiveData)
        s = s.reduce(ClientEvent.Disconnected("timeout")).reduce(ClientEvent.Connecting(1))
        assertEquals(ConnectionState.Reconnecting, s.connection)
        assertEquals("timeout", s.lastError)
    }

    @Test
    fun droneDetectedFromFrames() {
        assertTrue(connected(track(1)).droneDetected)
        assertFalse(connected(track(1, active = false)).droneDetected)
        assertFalse(connected(track(2, kind = Track.KIND_WIDEBAND)).droneDetected)
        assertFalse(connected().droneDetected)
    }

    @Test
    fun disconnectClearsStaleDetection() {
        val s = connected(track(1)).reduce(ClientEvent.Disconnected("reset"))
        assertFalse(s.droneDetected)
        assertFalse(s.hasLiveData)
        assertTrue(s.tracks.isEmpty())
    }

    @Test
    fun activeTracksPutDroneLinksFirst() {
        val s = connected(
            track(1, kind = Track.KIND_WIDEBAND, peak = -40f),
            track(2, peak = -70f),
            track(3, active = false),
        )
        assertEquals(listOf(2L, 1L), s.activeTracks.map { it.id })
        assertEquals(2L, s.strongestDroneLink!!.id)
    }

    @Test
    fun alertsOncePerNewDroneTrack() {
        val alerts = DroneAlertTracker()
        assertEquals(listOf(3L), alerts.newDroneLinks(listOf(track(3), track(4, kind = Track.KIND_WIDEBAND))).map { it.id })
        assertTrue("same track again: no alert", alerts.newDroneLinks(listOf(track(3))).isEmpty())
        assertTrue("inactive: no alert", alerts.newDroneLinks(listOf(track(3), track(5, active = false))).isEmpty())
        assertEquals("track 5 became active", listOf(5L), alerts.newDroneLinks(listOf(track(3), track(5))).map { it.id })
        alerts.newDroneLinks(emptyList()) // tracks expired
        assertEquals("re-appearing id alerts again", listOf(3L), alerts.newDroneLinks(listOf(track(3))).map { it.id })
    }

    @Test
    fun backoffGrowsAndCaps() {
        assertEquals(500L, backoffMillis(0))
        assertEquals(1_000L, backoffMillis(1))
        assertEquals(4_000L, backoffMillis(3))
        assertEquals(15_000L, backoffMillis(10))
        assertEquals(15_000L, backoffMillis(1_000))
    }
}
