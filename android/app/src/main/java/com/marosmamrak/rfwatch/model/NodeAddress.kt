package com.marosmamrak.rfwatch.model

/** An rf-watch node reachable at [host]:[port], found over NSD or typed in by the user. */
data class NodeAddress(
    val name: String,
    val host: String,
    val port: Int,
    val wsPath: String = "/ws",
    val version: String? = null,
) {
    /** Host as it appears in a URL: IPv6 literals need brackets. */
    private val urlHost: String get() = if (host.contains(':')) "[$host]" else host

    val hostPort: String get() = "$urlHost:$port"
    val httpUrl: String get() = "http://$hostPort/"
    val wsUrl: String get() = "ws://$hostPort" + (if (wsPath.startsWith("/")) wsPath else "/$wsPath")

    companion object {
        const val DEFAULT_PORT = 8787

        /**
         * Parses manual input such as `10.0.2.2:8787`, `rf-watch.local`, `[fe80::1]:8787`
         * or `http://192.168.1.20:8787/`. Missing port means [DEFAULT_PORT].
         */
        fun parse(input: String): NodeAddress? {
            var s = input.trim()
            s = s.removePrefix("http://").removePrefix("ws://")
            s = s.substringBefore('/')
            if (s.isEmpty() || s.contains(' ')) return null

            val host: String
            val portText: String?
            if (s.startsWith("[")) {
                val end = s.indexOf(']')
                if (end < 2) return null
                host = s.substring(1, end)
                val rest = s.substring(end + 1)
                portText = when {
                    rest.isEmpty() -> null
                    rest.startsWith(":") -> rest.substring(1)
                    else -> return null
                }
            } else if (s.count { it == ':' } > 1) {
                host = s // bare IPv6 literal without port
                portText = null
            } else {
                host = s.substringBefore(':')
                portText = if (s.contains(':')) s.substringAfter(':') else null
            }
            if (!isValidHost(host)) return null
            val port = if (portText == null) DEFAULT_PORT else portText.toIntOrNull() ?: return null
            if (port !in 1..65535) return null
            return NodeAddress(name = host, host = host, port = port)
        }

        private val hostLabel = Regex("[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?")

        /** Hostname, IPv4 (all four octets) or IPv6 literal; rejects partial input like `10.`. */
        private fun isValidHost(host: String): Boolean {
            if (host.contains(':')) return host.all { it.isLetterOrDigit() || it == ':' || it == '.' || it == '%' }
            val labels = host.split('.')
            if (labels.any { !hostLabel.matches(it) }) return false
            if (host.all { it.isDigit() || it == '.' }) {
                return labels.size == 4 && labels.all { (it.toIntOrNull() ?: 256) in 0..255 }
            }
            return true
        }
    }
}
