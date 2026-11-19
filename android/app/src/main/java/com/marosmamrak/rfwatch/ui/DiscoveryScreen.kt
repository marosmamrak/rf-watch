package com.marosmamrak.rfwatch.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.marosmamrak.rfwatch.discovery.NsdDiscovery
import com.marosmamrak.rfwatch.model.NodeAddress
import com.marosmamrak.rfwatch.monitor.MonitorState

@Composable
fun DiscoveryScreen(
    monitor: MonitorState,
    onSelect: (NodeAddress) -> Unit,
    onOpenMonitored: () -> Unit,
    onStopMonitoring: () -> Unit,
) {
    val context = LocalContext.current
    val discovery = remember { NsdDiscovery(context) }
    DisposableEffect(discovery) {
        discovery.start()
        onDispose { discovery.stop() }
    }
    val nodes by discovery.nodes.collectAsStateWithLifecycle()
    val status by discovery.status.collectAsStateWithLifecycle()
    var manual by rememberSaveable { mutableStateOf("") }
    val parsed = NodeAddress.parse(manual)

    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        Column(
            Modifier
                .safeDrawingPadding()
                .imePadding()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("rf-watch", style = MaterialTheme.typography.headlineMedium)

            val monitored = monitor.node
            if (monitored != null) {
                Card(Modifier.fillMaxWidth()) {
                    Text(
                        "Monitoring ${monitored.name} in the background",
                        Modifier.padding(start = 16.dp, top = 12.dp, end = 16.dp),
                        style = MaterialTheme.typography.titleSmall,
                    )
                    StatusBanner(monitor, Modifier.padding(vertical = 8.dp))
                    Row(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) {
                        Button(onClick = onOpenMonitored) { Text("Open") }
                        Spacer(Modifier.width(8.dp))
                        OutlinedButton(onClick = onStopMonitoring) { Text("Stop") }
                    }
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Nodes on this network", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                when (val s = status) {
                    NsdDiscovery.Status.Searching -> CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    is NsdDiscovery.Status.Failed -> Text("NSD error ${s.code}", color = MaterialTheme.colorScheme.error)
                    NsdDiscovery.Status.Idle -> Unit
                }
            }

            LazyColumn(Modifier.weight(1f, fill = false)) {
                if (nodes.isEmpty()) {
                    item {
                        Text(
                            "Searching for ${NsdDiscovery.SERVICE_TYPE}… Nodes appear here when they " +
                                "advertise on the same Wi-Fi. The Android emulator cannot see LAN mDNS; " +
                                "use 10.0.2.2:8787 below.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                items(nodes, key = { it.name }) { node ->
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .clickable { onSelect(node) }
                            .padding(vertical = 12.dp),
                    ) {
                        Text(node.name, style = MaterialTheme.typography.titleMedium)
                        Text(
                            node.hostPort + (node.version?.let { " · v$it" } ?: ""),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    HorizontalDivider()
                }
            }

            Text("Manual connection", style = MaterialTheme.typography.titleMedium)
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = manual,
                    onValueChange = { manual = it },
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                    label = { Text("host:port") },
                    placeholder = { Text("10.0.2.2:8787") },
                    isError = manual.isNotBlank() && parsed == null,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                    keyboardActions = KeyboardActions(onGo = { parsed?.let(onSelect) }),
                )
                Spacer(Modifier.width(8.dp))
                Button(onClick = { parsed?.let(onSelect) }, enabled = parsed != null) { Text("Connect") }
            }
        }
    }
}
