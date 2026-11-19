package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.model.NodeAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NodeAddressTest {

    @Test
    fun parsesHostAndPort() {
        val n = NodeAddress.parse(" 10.0.2.2:8787 ")!!
        assertEquals("10.0.2.2", n.host)
        assertEquals(8787, n.port)
        assertEquals("http://10.0.2.2:8787/", n.httpUrl)
        assertEquals("ws://10.0.2.2:8787/ws", n.wsUrl)
    }

    @Test
    fun defaultsPortAndStripsScheme() {
        assertEquals(8787, NodeAddress.parse("rf-watch.local")!!.port)
        val n = NodeAddress.parse("http://192.168.1.20:9000/")!!
        assertEquals("192.168.1.20", n.host)
        assertEquals(9000, n.port)
    }

    @Test
    fun handlesIpv6() {
        val n = NodeAddress.parse("[fe80::1]:8787")!!
        assertEquals("fe80::1", n.host)
        assertEquals("ws://[fe80::1]:8787/ws", n.wsUrl)
        assertEquals("fe80::1", NodeAddress.parse("fe80::1")!!.host)
    }

    @Test
    fun rejectsInvalidInput() {
        assertNull(NodeAddress.parse(""))
        assertNull(NodeAddress.parse("host:"))
        assertNull(NodeAddress.parse("host:abc"))
        assertNull(NodeAddress.parse("host:70000"))
        assertNull(NodeAddress.parse(":8787"))
        assertNull(NodeAddress.parse("two words"))
        assertNull("partial IPv4", NodeAddress.parse("10."))
        assertNull("partial IPv4", NodeAddress.parse("10.0.2:8787"))
        assertNull(NodeAddress.parse("10.0.2.256"))
        assertNull(NodeAddress.parse("bad..host"))
        assertNull(NodeAddress.parse("under_score"))
    }

    @Test
    fun usesAdvertisedWsPath() {
        assertEquals("ws://h:1/stream", NodeAddress("n", "h", 1, wsPath = "stream").wsUrl)
    }
}
