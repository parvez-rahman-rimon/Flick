package com.eksend.flick.network

import android.content.Context
import com.eksend.flick.crypto.KeystoreManager
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.PairedPc
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

object PcAutoRouter {

    suspend fun resolveTargetIp(context: Context, pc: PairedPc): String? = withContext(Dispatchers.IO) {
        val prefs = FlickPreferences(context)

        // Step 1: Fast Path (600ms cached IP mTLS ping)
        if (pc.lastKnownIp.isNotBlank()) {
            val fastPingResult = FlickHttpClient.ping(
                ip = pc.lastKnownIp,
                port = pc.port,
                authToken = pc.authToken,
                pcSpkiPin = pc.pcSpkiPin,
                connectTimeoutMs = 600
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
                connectTimeoutMs = 600
            )
            if (livePingResult.isSuccess) {
                prefs.updateLastKnownIp(pc.pcId, liveDiscoveredIp)
                return@withContext liveDiscoveredIp
            }
        }

        // Step 2: Authenticated UDP Discovery Beacon Fallback (<= 1000ms)
        if (prefs.isAutoUpdateIpEnabled) {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? android.net.wifi.WifiManager
            val multicastLock = try {
                wifiManager?.createMulticastLock("FlickAutoRouterLock")?.apply {
                    setReferenceCounted(false)
                    acquire()
                }
            } catch (_: Exception) { null }

            var discoveredIp: String? = null
            var socket: DatagramSocket? = null
            try {
                socket = DatagramSocket(null).apply {
                    reuseAddress = true
                    broadcast = true
                    bind(InetSocketAddress(45454))
                    soTimeout = 1000
                }
                val buffer = ByteArray(4096)
                val packet = DatagramPacket(buffer, buffer.size)
                val endTime = System.currentTimeMillis() + 1000

                while (System.currentTimeMillis() < endTime) {
                    try {
                        socket.receive(packet)
                        val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                        val json = JSONObject(text)
                        val proto = json.optString("protocol")
                        val pcId = json.optString("pc_id")

                        if ((proto == "FLICK_DISCOVERY_V2" || proto == "FLICK_DISCOVERY_V1") && pcId == pc.pcId) {
                            // Verify rolling HMAC authenticator tag for v2 beacons
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
                                    clientId = prefs.clientId
                                )
                                if (!isTagValid) {
                                    // Discard rogue, replayed, or spoofed beacon
                                    continue
                                }
                            }

                            val ip = packet.address?.hostAddress ?: ""
                            if (ip.isNotEmpty()) {
                                discoveredIp = ip
                                break
                            }
                        }
                    } catch (_: java.net.SocketTimeoutException) {
                        break
                    }
                }
            } catch (_: Exception) {
            } finally {
                socket?.close()
                try {
                    multicastLock?.let { if (it.isHeld) it.release() }
                } catch (_: Exception) {}
            }

            if (discoveredIp != null) {
                // Verify new IP with quick authenticated ping
                val verifyResult = FlickHttpClient.ping(
                    ip = discoveredIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    pcSpkiPin = pc.pcSpkiPin,
                    connectTimeoutMs = 300
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
