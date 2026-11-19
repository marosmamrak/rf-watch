package com.marosmamrak.rfwatch.discovery

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.annotation.RequiresApi
import com.marosmamrak.rfwatch.model.NodeAddress
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import java.net.Inet4Address
import java.net.InetAddress
import java.util.concurrent.Executor

/**
 * Browses `_rfwatch._tcp` with Android NSD and resolves each instance to host:port.
 * API 34+ uses [NsdManager.registerServiceInfoCallback] (follows address changes);
 * older versions resolve one service at a time, as the legacy API requires.
 */
class NsdDiscovery(context: Context) {

    sealed interface Status {
        data object Idle : Status
        data object Searching : Status
        data class Failed(val code: Int) : Status
    }

    private val nsd = context.applicationContext.getSystemService(NsdManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private val mainExecutor = Executor { main.post(it) }

    private val _nodes = MutableStateFlow<List<NodeAddress>>(emptyList())
    val nodes: StateFlow<List<NodeAddress>> = _nodes.asStateFlow()
    private val _status = MutableStateFlow<Status>(Status.Idle)
    val status: StateFlow<Status> = _status.asStateFlow()

    private var listener: NsdManager.DiscoveryListener? = null
    private val infoCallbacks = mutableMapOf<String, NsdManager.ServiceInfoCallback>()
    private val pending = ArrayDeque<NsdServiceInfo>()
    private var resolving = false

    fun start() {
        if (listener != null) return
        val l = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {
                _status.value = Status.Searching
            }

            override fun onServiceFound(info: NsdServiceInfo) = main.post { onFound(info) }.let { }

            override fun onServiceLost(info: NsdServiceInfo) = main.post { onLost(info.serviceName) }.let { }

            override fun onDiscoveryStopped(serviceType: String) {
                _status.value = Status.Idle
            }

            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                _status.value = Status.Failed(errorCode)
                main.post { listener = null }
            }

            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        }
        listener = l
        nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, l)
    }

    fun stop() {
        listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        listener = null
        if (Build.VERSION.SDK_INT >= 34) {
            infoCallbacks.values.forEach { runCatching { nsd.unregisterServiceInfoCallback(it) } }
        }
        infoCallbacks.clear()
        pending.clear()
        _status.value = Status.Idle
    }

    private fun onFound(info: NsdServiceInfo) {
        if (listener == null) return
        if (Build.VERSION.SDK_INT >= 34) {
            watch(info)
        } else {
            pending.addLast(info)
            resolveNext()
        }
    }

    private fun onLost(name: String) {
        if (Build.VERSION.SDK_INT >= 34) {
            infoCallbacks.remove(name)?.let { runCatching { nsd.unregisterServiceInfoCallback(it) } }
        }
        _nodes.update { list -> list.filterNot { it.name == name } }
    }

    private fun publish(info: NsdServiceInfo, address: InetAddress?) {
        val host = address?.hostAddress?.substringBefore('%') ?: return
        val attrs = info.attributes
        val node = NodeAddress(
            name = info.serviceName,
            host = host,
            port = info.port,
            wsPath = attrs["ws"]?.decodeToString() ?: "/ws",
            version = attrs["version"]?.decodeToString(),
        )
        _nodes.update { list -> (list.filterNot { it.name == node.name } + node).sortedBy { it.name } }
    }

    @RequiresApi(34)
    private fun watch(info: NsdServiceInfo) {
        if (infoCallbacks.containsKey(info.serviceName)) return
        val cb = object : NsdManager.ServiceInfoCallback {
            override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) {
                infoCallbacks.remove(info.serviceName)
            }

            override fun onServiceUpdated(updated: NsdServiceInfo) {
                val addrs = updated.hostAddresses
                publish(updated, addrs.firstOrNull { it is Inet4Address } ?: addrs.firstOrNull())
            }

            override fun onServiceLost() = onLost(info.serviceName)

            override fun onServiceInfoCallbackUnregistered() = Unit
        }
        infoCallbacks[info.serviceName] = cb
        nsd.registerServiceInfoCallback(info, mainExecutor, cb)
    }

    @Suppress("DEPRECATION")
    private fun resolveNext() {
        if (resolving || listener == null) return
        val next = pending.removeFirstOrNull() ?: return
        resolving = true
        nsd.resolveService(next, object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) = main.post {
                resolving = false
                resolveNext()
            }.let { }

            override fun onServiceResolved(info: NsdServiceInfo) = main.post {
                resolving = false
                if (listener != null) publish(info, info.host)
                resolveNext()
            }.let { }
        })
    }

    companion object {
        const val SERVICE_TYPE = "_rfwatch._tcp"
    }
}
