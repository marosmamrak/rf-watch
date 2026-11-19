package com.marosmamrak.rfwatch.monitor

import com.marosmamrak.rfwatch.model.Protocol
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

/**
 * Keeps one WebSocket to a node's `/ws` open, reconnecting with exponential backoff.
 * Pure JVM (no Android APIs) so it can be tested against MockWebServer.
 * [onEvent] is called from OkHttp's threads.
 */
class NodeClient(
    private val http: OkHttpClient,
    private val wsUrl: String,
    private val scope: CoroutineScope,
    private val onEvent: (ClientEvent) -> Unit,
) {
    private var job: Job? = null

    fun start() {
        if (job?.isActive == true) return
        job = scope.launch { runLoop() }
    }

    fun stop() {
        job?.cancel()
        job = null
    }

    private suspend fun runLoop() {
        var attempt = 0
        while (currentCoroutineContext().isActive) {
            onEvent(ClientEvent.Connecting(attempt))
            val session = Session()
            val ws = http.newWebSocket(Request.Builder().url(wsUrl).build(), session)
            val reason = try {
                session.closed.await()
            } finally {
                // Also runs on cancellation (stop()): drop the socket, silence late callbacks.
                session.live = false
                ws.cancel()
            }
            if (session.gotMessage) attempt = 0
            onEvent(ClientEvent.Disconnected(reason))
            delay(backoffMillis(attempt))
            attempt++
        }
    }

    private inner class Session : WebSocketListener() {
        val closed = CompletableDeferred<String>()

        @Volatile var live = true
        @Volatile var gotMessage = false

        override fun onOpen(webSocket: WebSocket, response: Response) {
            if (live) onEvent(ClientEvent.Open)
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            if (!live) return
            val msg = Protocol.parse(text) ?: return
            gotMessage = true
            onEvent(ClientEvent.Message(msg))
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
            closed.complete("closed by node ($code)")
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            closed.complete("closed ($code)")
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            closed.complete(t.message ?: t.javaClass.simpleName)
        }
    }
}
