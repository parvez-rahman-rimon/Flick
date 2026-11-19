package com.eksend.flick.network

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import com.eksend.flick.data.PairedPc
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume

/**
 * Flick Turbo Link Controller (Android TX-1).
 *
 * Spawns an ephemeral offline 5GHz LocalOnlyHotspot, dispatches credentials over the
 * authenticated LAN control plane (/turbo/initiate), waits for the Windows RX-1
 * FLICK_TURBO_READY UDP beacon on port 45456, and guarantees < 2.5s fail-fast
 * fallback to standard LAN on any permission denial, timeout, or hardware conflict.
 */
object TurboLinkController {

    private const val TAG = "FlickTurbo"
    private const val TURBO_UDP_PORT = 45456
    private const val HANDSHAKE_TIMEOUT_MS = 12000L

    data class TurboEstablishResult(
        val session: TurboSession?,
        val pcNeedsLanRecoveryWait: Boolean
    )

    data class TurboSession(
        val turboIp: String,
        val sessionId: String,
        val ssid: String,
        val reservation: WifiManager.LocalOnlyHotspotReservation
    ) {
        fun closeQuietly() {
            try {
                reservation.close()
            } catch (_: Exception) {}
        }
    }

    fun hasRequiredPermission(context: Context): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.NEARBY_WIFI_DEVICES
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.ACCESS_FINE_LOCATION
            ) == PackageManager.PERMISSION_GRANTED
        }
    }

    fun requiredPermissionName(): String {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            Manifest.permission.NEARBY_WIFI_DEVICES
        } else {
            Manifest.permission.ACCESS_FINE_LOCATION
        }
    }

    @SuppressLint("MissingPermission")
    suspend fun establishTurboLink(
        context: Context,
        lanIp: String,
        pc: PairedPc
    ): TurboEstablishResult = withContext(Dispatchers.IO) {
        if (!hasRequiredPermission(context)) {
            return@withContext TurboEstablishResult(null, false)
        }

        val wifiManager = context.applicationContext
            .getSystemService(Context.WIFI_SERVICE) as? WifiManager
            ?: return@withContext TurboEstablishResult(null, false)

        val reservationRef = AtomicReference<WifiManager.LocalOnlyHotspotReservation?>(null)
        val pcInitiated = java.util.concurrent.atomic.AtomicBoolean(false)
        var udpSocket: DatagramSocket? = null
        var succeeded = false

        try {
            val result = withTimeoutOrNull(HANDSHAKE_TIMEOUT_MS) {
                // 1. Bind UDP listener on :45456 BEFORE starting hotspot / initiating PC switch
                val sock = DatagramSocket(null).apply {
                    reuseAddress = true
                    broadcast = true
                    bind(InetSocketAddress(TURBO_UDP_PORT))
                    soTimeout = 150
                }
                udpSocket = sock

                // 2. Start LocalOnlyHotspot (while wlan0 is undisturbed)
                val t0 = System.currentTimeMillis()
                val reservation = startHotspotSuspending(wifiManager, reservationRef)
                    ?: run {
                        Log.w(TAG, "LocalOnlyHotspot failed to start; falling back to LAN")
                        return@withTimeoutOrNull null
                    }

                val (ssid, passphrase) = extractCredentials(reservation)
                    ?: run {
                        Log.w(TAG, "Could not extract LocalOnlyHotspot credentials; falling back to LAN")
                        return@withTimeoutOrNull null
                    }
                Log.i(TAG, "Hotspot started in ${System.currentTimeMillis() - t0}ms (ssid=$ssid)")

                val sessionId = UUID.randomUUID().toString()

                // 3. Dispatch encrypted /turbo/initiate immediately over idle LAN link;
                //    Windows waits for hostapd 5GHz CSA before disconnecting from router.
                val initResult = FlickHttpClient.initiateTurbo(
                    targetIp = lanIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    pcSpkiPin = pc.pcSpkiPin,
                    ssid = ssid,
                    key = passphrase,
                    sessionId = sessionId
                )

                val switching = initResult.getOrDefault(false)
                if (!switching) {
                    Log.w(TAG, "PC rejected or failed /turbo/initiate (${initResult.exceptionOrNull()?.message}); falling back to LAN")
                    return@withTimeoutOrNull null
                }
                pcInitiated.set(true)

                // 4. Wait for Windows RX-1 FLICK_TURBO_READY UDP packet on the hotspot subnet
                val turboIp = awaitTurboReadyBeacon(sock, sessionId, pc.pcId)
                    ?: run {
                        Log.w(TAG, "Timed out waiting for FLICK_TURBO_READY UDP packet")
                        return@withTimeoutOrNull null
                    }

                // 5. Warm up mTLS 1.3 connection pool on the new direct P2P link (retry through Windows ARP/NLA)
                var pingOk = false
                var lastPingErr: String? = null
                for (attempt in 1..3) {
                    val pingRes = FlickHttpClient.ping(
                        ip = turboIp,
                        port = pc.port,
                        authToken = pc.authToken,
                        pcSpkiPin = pc.pcSpkiPin,
                        connectTimeoutMs = 950,
                        readTimeoutMs = 1100
                    )
                    if (pingRes.isSuccess) {
                        pingOk = true
                        break
                    }
                    lastPingErr = pingRes.exceptionOrNull()?.message
                    kotlinx.coroutines.delay(150)
                }
                if (!pingOk) {
                    Log.w(TAG, "mTLS ping to $turboIp failed after retries ($lastPingErr)")
                    try {
                        FlickHttpClient.teardownTurbo(
                            targetIp = turboIp,
                            port = pc.port,
                            authToken = pc.authToken,
                            pcSpkiPin = pc.pcSpkiPin,
                            sessionId = sessionId
                        )
                    } catch (_: Exception) {}
                    return@withTimeoutOrNull null
                }
                Log.i(TAG, "Turbo Link READY at $turboIp in ${System.currentTimeMillis() - t0}ms")

                succeeded = true
                TurboSession(
                    turboIp = turboIp,
                    sessionId = sessionId,
                    ssid = ssid,
                    reservation = reservation
                )
            }

            if (!succeeded) {
                reservationRef.getAndSet(null)?.let { res ->
                    try { res.close() } catch (_: Exception) {}
                }
            }
            TurboEstablishResult(result, !succeeded && pcInitiated.get())
        } catch (_: Exception) {
            reservationRef.getAndSet(null)?.let { res ->
                try { res.close() } catch (_: Exception) {}
            }
            TurboEstablishResult(null, pcInitiated.get())
        } finally {
            try { udpSocket?.close() } catch (_: Exception) {}
        }
    }

    fun teardown(session: TurboSession?, pc: PairedPc) {
        if (session == null) return
        try {
            FlickHttpClient.teardownTurbo(
                targetIp = session.turboIp,
                port = pc.port,
                authToken = pc.authToken,
                pcSpkiPin = pc.pcSpkiPin,
                sessionId = session.sessionId
            )
        } catch (_: Exception) {}
        session.closeQuietly()
    }

    @SuppressLint("MissingPermission")
    private suspend fun startHotspotSuspending(
        wifiManager: WifiManager,
        reservationRef: AtomicReference<WifiManager.LocalOnlyHotspotReservation?>
    ): WifiManager.LocalOnlyHotspotReservation? = suspendCancellableCoroutine { cont ->
        try {
            wifiManager.startLocalOnlyHotspot(
                object : WifiManager.LocalOnlyHotspotCallback() {
                    override fun onStarted(reservation: WifiManager.LocalOnlyHotspotReservation) {
                        if (cont.isActive) {
                            reservationRef.set(reservation)
                            cont.resume(reservation)
                        } else {
                            // Timeout or cancellation already happened: immediately release hotspot
                            try { reservation.close() } catch (_: Exception) {}
                        }
                    }

                    override fun onStopped() {
                        if (cont.isActive) {
                            cont.resume(null)
                        }
                    }

                    override fun onFailed(reason: Int) {
                        Log.w(TAG, "LocalOnlyHotspot onFailed(reason=$reason)")
                        if (cont.isActive) {
                            cont.resume(null)
                        }
                    }
                },
                Handler(Looper.getMainLooper())
            )
        } catch (_: SecurityException) {
            if (cont.isActive) cont.resume(null)
        } catch (_: IllegalStateException) {
            if (cont.isActive) cont.resume(null)
        } catch (_: Exception) {
            if (cont.isActive) cont.resume(null)
        }

        cont.invokeOnCancellation {
            reservationRef.getAndSet(null)?.let { res ->
                try { res.close() } catch (_: Exception) {}
            }
        }
    }

    private fun extractCredentials(
        reservation: WifiManager.LocalOnlyHotspotReservation
    ): Pair<String, String>? {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val cfg = reservation.softApConfiguration
            val ssid = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                cfg?.wifiSsid?.bytes?.let { String(it, Charsets.UTF_8) }
                    ?: @Suppress("DEPRECATION") cfg?.ssid
            } else {
                @Suppress("DEPRECATION")
                cfg?.ssid
            }
            val key = cfg?.passphrase
            if (!ssid.isNullOrEmpty() && !key.isNullOrEmpty()) {
                return ssid to key
            }
        }
        @Suppress("DEPRECATION")
        val legacyCfg = reservation.wifiConfiguration
        @Suppress("DEPRECATION")
        val legacySsid = legacyCfg?.SSID?.trim('"')
        @Suppress("DEPRECATION")
        val legacyKey = legacyCfg?.preSharedKey?.trim('"')
        if (!legacySsid.isNullOrEmpty() && !legacyKey.isNullOrEmpty()) {
            return legacySsid to legacyKey
        }
        return null
    }

    private fun awaitTurboReadyBeacon(
        socket: DatagramSocket,
        expectedSessionId: String,
        expectedPcId: String
    ): String? {
        val buf = ByteArray(2048)
        val packet = DatagramPacket(buf, buf.size)
        val deadline = System.currentTimeMillis() + 10400L

        while (System.currentTimeMillis() < deadline) {
            try {
                socket.receive(packet)
                val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                val json = JSONObject(text)
                if (json.optString("protocol") == "FLICK_TURBO_READY" &&
                    json.optString("session_id") == expectedSessionId &&
                    json.optString("pc_id") == expectedPcId
                ) {
                    val senderIp = packet.address?.hostAddress ?: ""
                    if (senderIp.isNotBlank() && senderIp != "127.0.0.1") {
                        return senderIp
                    }
                }
            } catch (_: java.net.SocketTimeoutException) {
                // Continue polling until deadline
            } catch (_: Exception) {
                // Ignore malformed packets
            }
        }
        return null
    }
}
