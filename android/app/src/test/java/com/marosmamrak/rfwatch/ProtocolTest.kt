package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.Fixtures.frame
import com.marosmamrak.rfwatch.Fixtures.trackJson
import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.model.Hello
import com.marosmamrak.rfwatch.model.Protocol
import com.marosmamrak.rfwatch.model.droneDetected
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProtocolTest {

    @Test
    fun parsesHello() {
        val hello = Protocol.parse(Fixtures.HELLO) as Hello
        assertEquals("rf-watch", hello.device)
        assertEquals("0.1.0", hello.version)
        val band = hello.band!!
        assertEquals(2483.5f, band.stopMhz)
        assertEquals(10, band.fps)
    }

    @Test
    fun parsesFrameWithTracksAndIgnoresSpectrum() {
        val msg = Protocol.parse(frame(812, trackJson(3, "fhss_control", active = true))) as Frame
        assertEquals(812L, msg.seq)
        assertEquals(-96.8f, msg.noiseFloorDbm)
        val t = msg.tracks.single()
        assertEquals(3L, t.id)
        assertEquals(2441.8f, t.centerMhz)
        assertEquals(-61.2f, t.peakDbm)
        assertEquals(0.9f, t.confidence)
        assertTrue(t.active)
        assertTrue(t.isDroneLink)
    }

    @Test
    fun droneDetectedOnlyForActiveFhssControl() {
        fun tracksOf(vararg json: String) = (Protocol.parse(frame(1, *json)) as Frame).tracks
        assertTrue(droneDetected(tracksOf(trackJson(1, "wideband", true), trackJson(2, "fhss_control", true))))
        assertFalse(droneDetected(tracksOf(trackJson(2, "fhss_control", active = false))))
        assertFalse(droneDetected(tracksOf(trackJson(1, "wideband", true))))
        assertFalse(droneDetected(tracksOf()))
    }

    @Test
    fun toleratesUnknownFieldsAndKinds() {
        val json = """{"type":"frame","seq":5,"future_field":{"a":1},"tracks":[""" +
            """{"id":9,"kind":"video_downlink","center_mhz":5800.0,"peak_dbm":-50.0,"active":true,"extra":true}]}"""
        val msg = Protocol.parse(json) as Frame
        assertEquals("video_downlink", msg.tracks.single().kind)
        assertFalse(droneDetected(msg.tracks))
    }

    @Test
    fun rejectsMalformedAndUnknownMessages() {
        assertNull(Protocol.parse("not json"))
        assertNull(Protocol.parse("""{"type":"bogus"}"""))
        assertNull(Protocol.parse("""{"type":"frame"}""")) // seq is required
        assertNull(Protocol.parse("""{"device":"x"}"""))
    }
}
