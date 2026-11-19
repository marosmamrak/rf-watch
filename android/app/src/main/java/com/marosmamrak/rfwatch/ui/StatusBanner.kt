package com.marosmamrak.rfwatch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.marosmamrak.rfwatch.monitor.ConnectionState
import com.marosmamrak.rfwatch.monitor.MonitorState
import com.marosmamrak.rfwatch.monitor.Notifications

/** Headline and detail line for the native status bar, derived from the `/ws` stream. */
data class StatusText(val headline: String, val detail: String, val color: Color)

fun statusText(state: MonitorState): StatusText {
    val conn = when (state.connection) {
        ConnectionState.Idle -> "idle"
        ConnectionState.Connecting -> "connecting…"
        ConnectionState.Connected -> "connected" + (state.version?.let { " · v$it" } ?: "")
        ConnectionState.Reconnecting -> "reconnecting" + (state.lastError?.let { " ($it)" } ?: "…")
    }
    val drone = state.strongestDroneLink
    return when {
        drone != null && state.droneDetected ->
            StatusText("DRONE DETECTED", "${Notifications.droneText(drone)} · $conn", AlertRed)
        state.hasLiveData ->
            StatusText("clear", "${state.activeTracks.size} active emitter(s) · $conn", ClearGreen)
        else -> StatusText("no data", conn, UnknownGrey)
    }
}

@Composable
fun StatusBanner(state: MonitorState, modifier: Modifier = Modifier) {
    val s = statusText(state)
    Column(
        modifier
            .fillMaxWidth()
            .background(s.color)
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text(s.headline, color = Color.White, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.titleMedium)
        Text(s.detail, color = Color.White, style = MaterialTheme.typography.bodySmall)
    }
}
