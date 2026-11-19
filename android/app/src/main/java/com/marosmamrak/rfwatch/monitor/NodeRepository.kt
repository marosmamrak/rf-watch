package com.marosmamrak.rfwatch.monitor

import com.marosmamrak.rfwatch.model.NodeAddress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Process-wide state of the monitored node. [MonitorService] writes it; the phone UI,
 * the ongoing notification and the Android Auto screen observe the same [state].
 */
object NodeRepository {
    private val _state = MutableStateFlow(MonitorState())
    val state: StateFlow<MonitorState> = _state.asStateFlow()

    /** Starts over for [node] (or clears everything for `null`). */
    fun select(node: NodeAddress?) {
        _state.update { current ->
            if (node != null && current.node == node) current
            else MonitorState(node = node, connection = if (node == null) ConnectionState.Idle else ConnectionState.Connecting)
        }
    }

    fun onEvent(node: NodeAddress, event: ClientEvent) {
        _state.update { if (it.node == node) it.reduce(event) else it }
    }
}
