package com.eksend.flick.network

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import com.eksend.flick.crypto.KeystoreManager
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.PairedPc
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.NetworkInterface

object PcAutoRouter {

    private const val DISCOVERY_PORT = 45454
    private const val DISCOVERY_PROBE_PORT = 45457
    private val PROBE_BYTES = "FLICK_PROBE".toByteArray(Charsets.UTF_8)

    private fun ipv4ToInt(addr: Inet4Address): Int {
        val b = addr.address
        return ((b[0].toInt() and 0xFF) shl 24) or
            ((b[1].toInt() and 0xFF) shl 16) or
            ((b[2].toInt() and 0xFF) shl 8) or
            (b[3].toInt() and 0xFF)
    }

    private fun prefixMask(prefixLen: Short): Int {
        val len = prefixLen.toInt().coerceIn(0, 32)
        return if (len == 0) 0 else (-1 shl (32 - len))
    }

    private fun isIpInSubnet(targetInt: Int, ifaceAddr: Inet4Address, prefixLen: Short): Boolean {
        val mask = prefixMask(prefixLen)
        return (targetInt and mask) == (ipv4ToInt(ifaceAddr) and mask)
    }

    fun isIpInAnyLocalSubnet(ipStr: String): Boolean {
        val targetInt = try {
            val addr = InetAddress.getByName(ipStr) as? Inet4Address ?: return false
            ipv4ToInt(addr)
        } catch (_: Exception) {
            return false
        }
        return try {
            val ifaces = NetworkInterface.getNetworkInterfaces() ?: return false
            for (iface in ifaces) {
                if (!iface.isUp || iface.isLoopback) continue
                for (ia in iface.interfaceAddresses) {
                    val v4 = ia.address as? Inet4Address ?: continue
                    if (isIpInSubnet(targetInt, v4, ia.networkPrefixLength)) {
                        return true
                    }
                }
            }
            false
        } catch (_: Exception) {
            false
        }
    }

    /**
     * Returns true when [targetIp] belongs directly to the phone's own active Portable Hotspot /
     * Tether subnet (e.g. `ap0`, `swlan0`, `wlan1`, or `wlan0` in AP mode when Wi-Fi STA is off).
     */
    fun isOnDirectHotspotSubnet(context: Context, targetIp: String): Boolean {
        val targetInt = try {
            val addr = InetAddress.getByName(targetIp) as? Inet4Address ?: return false
            ipv4ToInt(addr)
        } catch (_: Exception) {
            return false
        }

        val hasWifiSta = try {
            val cm = context.applicationContext.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
            cm?.allNetworks?.any { net ->
                cm.getNetworkCapabilities(net)?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
            } ?: false
        } catch (_: Exception) {
            false
        }

        return try {
            val ifaces = NetworkInterface.getNetworkInterfaces() ?: return false
            for (iface in ifaces) {
                if (!iface.isUp || iface.isLoopback) continue
                val name = iface.name.lowercase()
                val isStaWlan0 = name == "wlan0"
                val isApOrTether = name.startsWith("ap") ||
                    name.startsWith("swlan") ||
                    name.startsWith("softap") ||
                    name.startsWith("rndis") ||
                    name.startsWith("wlan") && !isStaWlan0

                if (!isApOrTether && !(isStaWlan0 && !hasWifiSta)) continue

                for (ia in iface.interfaceAddresses) {
                    val v4 = ia.address as? Inet4Address ?: continue
                    if (v4.isLoopbackAddress) continue
                    if (isIpInSubnet(targetInt, v4, ia.networkPrefixLength)) {
                        return true
                    }
                }
            }
            false
        } catch (_: Exception) {
            false
        }
    }

    private fun getAllBroadcastTargets(): List<Pair<Inet4Address?, InetAddress>> {
        val targets = mutableListOf<Pair<Inet4Address?, InetAddress>>()
        try {
            val ifaces = NetworkInterface.getNetworkInterfaces()
            if (ifaces != null) {
                for (iface in ifaces) {
                    if (!iface.isUp || iface.isLoopback) continue
                    for (ia in iface.interfaceAddresses) {
                        val v4 = ia.address as? Inet4Address ?: continue
                        if (v4.isLoopbackAddress) continue
                        val bcast = ia.broadcast ?: run {
                            val mask = prefixMask(ia.networkPrefixLength)
                            val bInt = ipv4ToInt(v4) or mask.inv()
                            val bytes = byteArrayOf(
                                ((bInt ushr 24) and 0xFF).toByte(),
                                ((bInt ushr 16) and 0xFF).toByte(),
                                ((bInt ushr 8) and 0xFF).toByte(),
                                (bInt and 0xFF).toByte()
                            )
                            InetAddress.getByAddress(bytes)
                        }
                        targets.add(v4 to bcast)
                    }
                }
            }
        } catch (_: Exception) {}
        try {
            targets.add(null to InetAddress.getByName("255.255.255.255"))
        } catch (_: Exception) {}
        return targets
    }

    private fun parseVerifiedBeacon(
        packet: DatagramPacket,
        pc: PairedPc,
        clientId: String
    ): String? {
        val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
        val json = JSONObject(text)
        val proto = json.optString("protocol")
        val pcId = json.optString("pc_id")

        if ((proto != "FLICK_DISCOVERY_V2" && proto != "FLICK_DISCOVERY_V1") || pcId != pc.pcId) {
            return null
        }

        if (proto == "FLICK_DISCOVERY_V2" && pc.pairSecret.isNotEmpty()) {
            val authTag = json.optString("auth_tag")
            val authTagsObj = json.optJSONObject("auth_tags")
            val authTagsMap = mutableMapOf<String, String>()
            if (authTagsObj != null) {
                val keys = authTagsObj.keys()
                while (keys.hasNext()) {
                    val k = keys.next()
                    authTagsMap[k] = authTagsObj.optString(k)
                }
            }

            val isTagValid = KeystoreManager.verifyRollingBeaconTag(
                pairSecret = pc.pairSecret,
                pcId = pc.pcId,
                receivedTag = authTag,
                authTags = authTagsMap,
                clientId = clientId
            )
            if (!isTagValid) return null
        }

        val ip = packet.address?.hostAddress ?: ""
        return ip.takeIf { it.isNotEmpty() }
    }

    suspend fun resolveTargetIp(context: Context, pc: PairedPc): String? = withContext(Dispatchers.IO) {
        val prefs = FlickPreferences(context)

        // Step 1: Fast Path cached IP mTLS ping (short timeout if stale subnet)
        if (pc.lastKnownIp.isNotBlank()) {
            val timeoutMs = if (isIpInAnyLocalSubnet(pc.lastKnownIp)) 550L else 250L
            val fastPingResult = FlickHttpClient.ping(
                ip = pc.lastKnownIp,
                port = pc.port,
                authToken = pc.authToken,
                pcSpkiPin = pc.pcSpkiPin,
                connectTimeoutMs = timeoutMs
            )
            if (fastPingResult.isSuccess) {
                return@withContext pc.lastKnownIp
            }
        }

        // Step 1.5: Check UdpDiscoveryListener in-memory cache
        val liveDiscoveredIp = UdpDiscoveryListener.getLatestPcIp(pc.pcId)
        if (!liveDiscoveredIp.isNullOrBlank() && liveDiscoveredIp != pc.lastKnownIp) {
            val livePingResult = FlickHttpClient.ping(
                ip = liveDiscoveredIp,
                port = pc.port,
                authToken = pc.authToken,
                pcSpkiPin = pc.pcSpkiPin,
                connectTimeoutMs = 500
            )
            if (livePingResult.isSuccess) {
                prefs.updateLastKnownIp(pc.pcId, liveDiscoveredIp)
                return@withContext liveDiscoveredIp
            }
        }

        // Step 2: Instant Active Probe (:45457) + Passive Beacon Fallback (:45454)
        if (prefs.isAutoUpdateIpEnabled) {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager
            val multicastLock = try {
                wifiManager?.createMulticastLock("FlickAutoRouterLock")?.apply {
                    setReferenceCounted(false)
                    acquire()
                }
            } catch (_: Exception) { null }

            var discoveredIp: String? = null
            try {
                // 2a. Active FLICK_PROBE over all local subnets (including ap0 Hotspot) for <50ms discovery
                val targets = getAllBroadcastTargets()
                var probeSocket: DatagramSocket? = null
                try {
                    probeSocket = DatagramSocket().apply {
                        broadcast = true
                        soTimeout = 250
                    }
                    val buffer = ByteArray(4096)
                    val packet = DatagramPacket(buffer, buffer.size)

                    for (attempt in 0 until 3) {
                        for ((localV4, bcast) in targets) {
                            try {
                                val probePkt = DatagramPacket(PROBE_BYTES, PROBE_BYTES.size, bcast, DISCOVERY_PROBE_PORT)
                                probeSocket.send(probePkt)
                                if (localV4 != null) {
                                    // Also send from an interface-bound socket so Android routes via ap0 even when cellular is active
                                    DatagramSocket(InetSocketAddress(localV4, 0)).use { boundSock ->
                                        boundSock.broadcast = true
                                        boundSock.send(probePkt)
                                    }
                                }
                            } catch (_: Exception) {}
                        }

                        val waitEnd = System.currentTimeMillis() + 250
                        while (System.currentTimeMillis() < waitEnd) {
                            try {
                                probeSocket.receive(packet)
                                val ip = parseVerifiedBeacon(packet, pc, prefs.clientId)
                                if (ip != null) {
                                    discoveredIp = ip
                                    break
                                }
                            } catch (_: java.net.SocketTimeoutException) {
                                break
                            } catch (_: Exception) {}
                        }
                        if (discoveredIp != null) break
                    }
                } finally {
                    probeSocket?.close()
                }

                // 2b. Fallback passive listen on :45454 (1200ms window > 1000ms beacon cadence)
                if (discoveredIp == null) {
                    var passiveSocket: DatagramSocket? = null
                    try {
                        passiveSocket = DatagramSocket(null).apply {
                            reuseAddress = true
                            broadcast = true
                            bind(InetSocketAddress(DISCOVERY_PORT))
                            soTimeout = 1200
                        }
                        val buffer = ByteArray(4096)
                        val packet = DatagramPacket(buffer, buffer.size)
                        val endTime = System.currentTimeMillis() + 1200

                        while (System.currentTimeMillis() < endTime) {
                            try {
                                passiveSocket.receive(packet)
                                val ip = parseVerifiedBeacon(packet, pc, prefs.clientId)
                                if (ip != null) {
                                    discoveredIp = ip
                                    break
                                }
                            } catch (_: java.net.SocketTimeoutException) {
                                break
                            } catch (_: Exception) {}
                        }
                    } catch (_: Exception) {
                    } finally {
                        passiveSocket?.close()
                    }
                }
            } finally {
                try {
                    multicastLock?.let { if (it.isHeld) it.release() }
                } catch (_: Exception) {}
            }

            if (discoveredIp != null) {
                val verifyResult = FlickHttpClient.ping(
                    ip = discoveredIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    pcSpkiPin = pc.pcSpkiPin,
                    connectTimeoutMs = 400
                )
                if (verifyResult.isSuccess) {
                    prefs.updateLastKnownIp(pc.pcId, discoveredIp)
                    return@withContext discoveredIp
                }
            }
        }

        // Fallback: If discovery timed out, still return lastKnownIp so OkHttp can attempt connection
        if (pc.lastKnownIp.isNotBlank()) {
            return@withContext pc.lastKnownIp
        }

        return@withContext null
    }
}
