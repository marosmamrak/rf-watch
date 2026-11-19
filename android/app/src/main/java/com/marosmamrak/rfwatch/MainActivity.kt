package com.marosmamrak.rfwatch

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.marosmamrak.rfwatch.monitor.MonitorService
import com.marosmamrak.rfwatch.monitor.NodeRepository
import com.marosmamrak.rfwatch.ui.DiscoveryScreen
import com.marosmamrak.rfwatch.ui.NodeScreen
import com.marosmamrak.rfwatch.ui.RfWatchTheme

class MainActivity : ComponentActivity() {

    /** Set by notification taps: jump to the monitored node. */
    private var openNodeRequest by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        openNodeRequest = intent.getBooleanExtra(EXTRA_OPEN_NODE, false)
        addOnNewIntentListener { i: Intent ->
            if (i.getBooleanExtra(EXTRA_OPEN_NODE, false)) openNodeRequest = true
        }

        setContent {
            RfWatchTheme {
                val state by NodeRepository.state.collectAsStateWithLifecycle()
                var showNode by rememberSaveable { mutableStateOf(false) }

                LaunchedEffect(openNodeRequest) {
                    if (openNodeRequest) {
                        showNode = true
                        openNodeRequest = false
                    }
                }

                val node = state.node
                if (showNode && node != null) {
                    // Back leaves monitoring running in the background; "Stop" ends it.
                    BackHandler { showNode = false }
                    NodeScreen(
                        node = node,
                        state = state,
                        onBack = { showNode = false },
                        onStop = {
                            MonitorService.stop(this)
                            showNode = false
                        },
                    )
                } else {
                    DiscoveryScreen(
                        monitor = state,
                        onSelect = { selected ->
                            MonitorService.start(this, selected)
                            showNode = true
                        },
                        onOpenMonitored = { showNode = true },
                        onStopMonitoring = { MonitorService.stop(this) },
                    )
                }
            }
        }
    }

    companion object {
        const val EXTRA_OPEN_NODE = "open_node"
    }
}
