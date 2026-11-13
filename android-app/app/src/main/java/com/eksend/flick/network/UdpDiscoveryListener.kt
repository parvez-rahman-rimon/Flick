package com.eksend.flick.network

import com.eksend.flick.data.DiscoveredPc
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

class UdpDiscoveryListener {

    private val _discoveredPcs = MutableStateFlow<Map<String, DiscoveredPc>>(emptyMap())
    val discoveredPcs: StateFlow<Map<String, DiscoveredPc>> = _discoveredPcs.asStateFlow()

    private var socket: DatagramSocket? = null
    private var job: Job? = null

    fun startListening(scope: CoroutineScope, port: Int = 45454) {
        if (job != null && job?.isActive == true) return

        job = scope.launch(Dispatchers.IO) {
            try {
                val sock = DatagramSocket(null).apply {
                    reuseAddress = true
                    broadcast = true
                    bind(InetSocketAddress(port))
                    soTimeout = 2000
                }
                socket = sock
                val buffer = ByteArray(4096)

                while (isActive) {
                    val packet = DatagramPacket(buffer, buffer.size)
                    try {
                        sock.receive(packet)
                        val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                        val json = JSONObject(text)
                        val proto = json.optString("protocol")
                        if (proto == "FLICK_DISCOVERY_V2" || proto == "FLICK_DISCOVERY_V1") {
                            val pcId = json.getString("pc_id")
                            val rawPcName = json.getString("pc_name")
                            val pcName = rawPcName.take(32)
                            val httpPort = json.optInt("https_port", json.optInt("http_port", 45455))
                            val version = json.optString("version", "2.0.0")
                            val authTag = json.optString("auth_tag", "")
                            val authTagsObj = json.optJSONObject("auth_tags")
                            val authTagsMap = mutableMapOf<String, String>()
                            if (authTagsObj != null) {
                                val keys = authTagsObj.keys()
                                while (keys.hasNext()) {
                                    val k = keys.next()
                                    authTagsMap[k] = authTagsObj.optString(k)
                                }
                            }

                            // Security Hardening: Enforce actual UDP packet source IP (defeats beacon IP spoofing/redirection)
                            val packetSourceIp = packet.address?.hostAddress ?: ""
                            if (packetSourceIp.isBlank() || httpPort !in 1024..65535) continue

                            val discovered = DiscoveredPc(
                                pcId = pcId,
                                pcName = pcName,
                                ip = packetSourceIp,
                                port = httpPort,
                                version = version,
                                timestamp = System.currentTimeMillis(),
                                authTag = authTag,
                                authTags = authTagsMap
                            )

                            val updated = _discoveredPcs.value.toMutableMap()
                            // Limit map size to 32 entries to prevent memory flooding
                            if (updated.size >= 32 && !updated.containsKey(pcId)) {
                                val oldestKey = updated.minByOrNull { it.value.timestamp }?.key
                                if (oldestKey != null) updated.remove(oldestKey)
                            }
                            updated[pcId] = discovered
                            _discoveredPcs.value = updated
                        }
                    } catch (_: java.net.SocketTimeoutException) {
                        // Regular timeout to allow checking isActive
                    } catch (_: Exception) {
                        // Transient network or JSON parse errors
                    }
                }
            } catch (_: Exception) {
            } finally {
                socket?.close()
                socket = null
            }
        }
    }

    fun stopListening() {
        job?.cancel()
        job = null
        try {
            socket?.close()
        } catch (_: Exception) {}
        socket = null
    }
}
