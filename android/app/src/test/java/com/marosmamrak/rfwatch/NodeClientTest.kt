package com.marosmamrak.rfwatch

import com.marosmamrak.rfwatch.Fixtures.frame
import com.marosmamrak.rfwatch.Fixtures.trackJson
import com.marosmamrak.rfwatch.model.Frame
import com.marosmamrak.rfwatch.monitor.ClientEvent
import com.marosmamrak.rfwatch.monitor.ConnectionState
import com.marosmamrak.rfwatch.monitor.MonitorState
import com.marosmamrak.rfwatch.monitor.NodeClient
import com.marosmamrak.rfwatch.monitor.reduce
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** End to end over a real socket: MockWebServer plays the rf-watch node. */
class NodeClientTest {

    private val server = MockWebServer()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val http = OkHttpClient()

    @Before fun setUp() = server.start()

    @After fun tearDown() {
        scope.cancel()
        server.close()
        http.dispatcher.executorService.shutdown()
    }

    private fun node(vararg messages: String, closeAfter: Boolean = false) = MockResponse.Builder()
        .webSocketUpgrade(object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                messages.forEach { webSocket.send(it) }
                if (closeAfter) webSocket.close(1001, "bye")
            }
        })
        .build()

    @Test
    fun streamsHelloAndFramesIntoState() {
        server.enqueue(node(Fixtures.HELLO, "garbage", frame(1, trackJson(3, "fhss_control", true))))
        val events = CopyOnWriteArrayList<ClientEvent>()
        val gotFrame = CountDownLatch(1)
        val client = NodeClient(http, server.url("/ws").toString(), scope) { e ->
            events += e
            if (e is ClientEvent.Message && e.msg is Frame) gotFrame.countDown()
        }
        client.start()
        assertTrue("frame received", gotFrame.await(5, TimeUnit.SECONDS))
        client.stop()

        val state = events.fold(MonitorState()) { s, e -> s.reduce(e) }
        assertEquals(ConnectionState.Connected, state.connection)
        assertEquals("rf-watch", state.device)
        assertTrue(state.droneDetected)
        assertEquals("/ws", server.takeRequest().url.encodedPath)
    }

    @Test
    fun reconnectsAfterNodeCloses() {
        server.enqueue(node(Fixtures.HELLO, closeAfter = true))
        server.enqueue(node(Fixtures.HELLO, frame(2)))
        val events = CopyOnWriteArrayList<ClientEvent>()
        val gotFrame = CountDownLatch(1)
        val client = NodeClient(http, server.url("/ws").toString(), scope) { e ->
            events += e
            if (e is ClientEvent.Message && e.msg is Frame) gotFrame.countDown()
        }
        client.start()
        assertTrue("frame after reconnect", gotFrame.await(10, TimeUnit.SECONDS))
        client.stop()

        assertTrue(events.any { it is ClientEvent.Disconnected })
        assertTrue(events.any { it == ClientEvent.Connecting(1) })
        assertEquals(2, server.requestCount)
    }
}
