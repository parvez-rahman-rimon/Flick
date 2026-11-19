package com.eksend.flick.network

import com.eksend.flick.crypto.KeystoreManager
import com.eksend.flick.data.ClipboardMode
import com.eksend.flick.data.PairedPc
import com.eksend.flick.data.PcOnlineState
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSink
import okio.source
import org.json.JSONObject
import java.io.InputStream
import java.net.URLEncoder
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

data class UploadResult(
    val savedPath: String,
    val bytesWritten: Long,
    val elapsedMs: Long,
    val clipboardFormats: List<String>
)

object FlickHttpClient {

    // Cached OkHttpClient instances per PC SPKI pin to maximize connection pooling and TLS 1.3 session resumption
    private val clientCache = ConcurrentHashMap<String, OkHttpClient>()

    fun clearCache() {
        clientCache.clear()
    }

    private fun getOrCreateClient(pcSpkiPin: String): OkHttpClient {
        val pinKey = pcSpkiPin.trim().lowercase()
        return clientCache.computeIfAbsent(pinKey) { pin ->
            val builder = OkHttpClient.Builder()
                .retryOnConnectionFailure(true)

            if (pin.isNotEmpty()) {
                try {
                    val (socketFactory, trustManager) = KeystoreManager.createTlsConfig(pin)
                    builder.sslSocketFactory(socketFactory, trustManager)
                    // Trust is cryptographically anchored directly to the pinned SPKI fingerprint
                    builder.hostnameVerifier { _, _ -> true }
                } catch (e: Exception) {
                    // Fall back to standard builder if keystore setup fails
                }
            }
            builder.build()
        }
    }

    fun ping(
        ip: String,
        port: Int = 45455,
        authToken: String? = null,
        pcSpkiPin: String = "",
        connectTimeoutMs: Long = 300
    ): Result<PcOnlineState> {
        val base = getOrCreateClient(pcSpkiPin)
        val client = base.newBuilder()
            .connectTimeout(connectTimeoutMs, TimeUnit.MILLISECONDS)
            .readTimeout(500, TimeUnit.MILLISECONDS)
            .build()

        val scheme = if (pcSpkiPin.isNotEmpty()) "https" else "http"
        val requestBuilder = Request.Builder()
            .url("$scheme://$ip:$port/ping")
            .get()

        if (!authToken.isNullOrBlank()) {
            requestBuilder.addHeader("X-Flick-Token", authToken)
        }

        val start = System.currentTimeMillis()
        return try {
            client.newCall(requestBuilder.build()).execute().use { resp ->
                val elapsed = System.currentTimeMillis() - start
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("HTTP error ${resp.code}"))
                }
                val bodyStr = resp.body?.string() ?: "{}"
                val json = JSONObject(bodyStr)
                val modeStr = json.optString("clipboard_mode", "SMART_MULTI")
                Result.success(
                    PcOnlineState(
                        isOnline = true,
                        pingMs = elapsed,
                        clipboardMode = ClipboardMode.fromId(modeStr),
                        lastChecked = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            // If https failed and we had no pin, try http fallback
            if (pcSpkiPin.isEmpty() && scheme == "http") {
                Result.failure(e)
            } else {
                Result.failure(e)
            }
        }
    }

    /**
     * Flick v2.0 Zero-Trust Out-of-Band Pairing:
     * Connects over HTTPS with TLS 1.3 and client certificate.
     * Transmits proof of possession: proof = HMAC-SHA256(pair_secret, client_der_cert).
     * PinningTrustManager guarantees the server matches the QR code's public key fingerprint.
     */
    fun pairV2(
        ip: String,
        port: Int = 45455,
        pcSpkiPin: String,
        pairSecret: String,
        pcId: String,
        clientId: String,
        clientName: String
    ): Result<PairedPc> {
        clearCache()
        KeystoreManager.ensureClientKeypair(clientId)
        val proof = KeystoreManager.generateProofOfPossession(pairSecret)

        val client = getOrCreateClient(pcSpkiPin)
            .newBuilder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .readTimeout(10, TimeUnit.SECONDS)
            .build()

        val jsonBody = JSONObject().apply {
            put("client_id", clientId)
            put("client_name", clientName)
            put("proof", proof)
        }

        val request = Request.Builder()
            .url("https://$ip:$port/v2/pair")
            .post(jsonBody.toString().toRequestBody("application/json".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Pairing failed (HTTP ${resp.code})"))
                }
                val json = JSONObject(resp.body?.string() ?: "{}")
                val returnedPcId = json.optString("pc_id", pcId)
                val pcName = json.optString("pc_name", "Windows PC")
                Result.success(
                    PairedPc(
                        pcId = returnedPcId,
                        pcName = pcName,
                        lastKnownIp = ip,
                        port = port,
                        authToken = "",
                        lastSeenEpochMs = System.currentTimeMillis(),
                        pcSpkiPin = pcSpkiPin,
                        pairSecret = pairSecret
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun pairWithToken(
        ip: String,
        port: Int = 45455,
        clientId: String,
        clientName: String,
        token: String
    ): Result<PairedPc> {
        val client = getOrCreateClient("")
            .newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .build()

        val jsonBody = JSONObject().apply {
            put("client_id", clientId)
            put("client_name", clientName)
            put("token", token)
        }

        val request = Request.Builder()
            .url("http://$ip:$port/pair")
            .post(jsonBody.toString().toRequestBody("application/json".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Pairing failed with code ${resp.code}"))
                }
                val json = JSONObject(resp.body?.string() ?: "{}")
                val pcId = json.getString("pc_id")
                val pcName = json.getString("pc_name")
                val authToken = json.getString("token")
                Result.success(
                    PairedPc(
                        pcId = pcId,
                        pcName = pcName,
                        lastKnownIp = ip,
                        port = port,
                        authToken = authToken,
                        lastSeenEpochMs = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun requestLanPair(
        ip: String,
        port: Int = 45455,
        clientId: String,
        clientName: String,
        pairCode: String
    ): Result<PairedPc> {
        val client = getOrCreateClient("")
            .newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(35, TimeUnit.SECONDS)
            .build()

        val jsonBody = JSONObject().apply {
            put("client_id", clientId)
            put("client_name", clientName)
            put("pair_code", pairCode)
        }

        val request = Request.Builder()
            .url("http://$ip:$port/pair")
            .post(jsonBody.toString().toRequestBody("application/json".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Pairing rejected on PC (code ${resp.code})"))
                }
                val json = JSONObject(resp.body?.string() ?: "{}")
                val pcId = json.getString("pc_id")
                val pcName = json.getString("pc_name")
                val authToken = json.getString("token")
                Result.success(
                    PairedPc(
                        pcId = pcId,
                        pcName = pcName,
                        lastKnownIp = ip,
                        port = port,
                        authToken = authToken,
                        lastSeenEpochMs = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun sendClipboardText(
        targetIp: String,
        port: Int = 45455,
        authToken: String = "",
        pcSpkiPin: String = "",
        text: String
    ): Result<Unit> {
        val client = getOrCreateClient(pcSpkiPin)
            .newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .build()

        val scheme = if (pcSpkiPin.isNotEmpty()) "https" else "http"
        val reqBuilder = Request.Builder()
            .url("$scheme://$targetIp:$port/clipboard")
            .post(text.toRequestBody("text/plain; charset=utf-8".toMediaTypeOrNull()))

        if (authToken.isNotEmpty()) {
            reqBuilder.addHeader("X-Flick-Token", authToken)
        }

        return try {
            client.newCall(reqBuilder.build()).execute().use { resp ->
                if (!resp.isSuccessful) {
                    Result.failure(Exception("Clipboard send failed with code ${resp.code}"))
                } else {
                    Result.success(Unit)
                }
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun uploadStream(
        targetIp: String,
        port: Int = 45455,
        authToken: String = "",
        pcSpkiPin: String = "",
        fileName: String,
        mimeType: String,
        clipboardMode: String = "DEFAULT",
        batchId: String? = null,
        batchIndex: Int = 0,
        batchTotal: Int = 1,
        contentLength: Long = -1L,
        onProgress: ((bytesSent: Long, totalBytes: Long, speedBps: Long) -> Unit)? = null,
        streamSupplier: () -> InputStream
    ): Result<UploadResult> {
        val client = getOrCreateClient(pcSpkiPin)
            .newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .writeTimeout(0, TimeUnit.MILLISECONDS) // 0 disables write timeout for large streaming uploads
            .readTimeout(180, TimeUnit.SECONDS)
            .build()

        val encodedFileName = URLEncoder.encode(fileName, "UTF-8")

        val body = object : RequestBody() {
            override fun contentType() = (mimeType.ifEmpty { "application/octet-stream" }).toMediaTypeOrNull()

            override fun contentLength(): Long = contentLength

            override fun writeTo(sink: BufferedSink) {
                streamSupplier().use { input ->
                    val buffer = ByteArray(512 * 1024) // 512 KB streaming buffer
                    var bytesRead: Int
                    var totalSent = 0L
                    var lastSpeedCalcTime = System.currentTimeMillis()
                    var bytesSinceLastCalc = 0L
                    var currentSpeedBps = 0L
                    var lastNotifyTime = 0L

                    while (input.read(buffer).also { bytesRead = it } != -1) {
                        sink.write(buffer, 0, bytesRead)
                        totalSent += bytesRead
                        bytesSinceLastCalc += bytesRead

                        val now = System.currentTimeMillis()
                        val elapsed = now - lastSpeedCalcTime
                        if (elapsed >= 150) {
                            currentSpeedBps = (bytesSinceLastCalc * 1000L) / elapsed.coerceAtLeast(1L)
                            lastSpeedCalcTime = now
                            bytesSinceLastCalc = 0L
                        }

                        // Throttle UI notification to 10 FPS to eliminate recomposition overhead
                        if (now - lastNotifyTime >= 100 || totalSent == contentLength) {
                            lastNotifyTime = now
                            onProgress?.invoke(totalSent, contentLength, currentSpeedBps)
                        }
                    }
                    sink.flush()
                }
            }
        }

        val scheme = if (pcSpkiPin.isNotEmpty()) "https" else "http"
        val reqBuilder = Request.Builder()
            .url("$scheme://$targetIp:$port/upload")
            .addHeader("X-File-Name", encodedFileName)
            .addHeader("X-Mime-Type", mimeType.ifEmpty { "application/octet-stream" })
            .addHeader("X-Clipboard-Mode", clipboardMode)
            .addHeader("X-Batch-Index", batchIndex.toString())
            .addHeader("X-Batch-Total", batchTotal.toString())
            .post(body)

        if (!batchId.isNullOrBlank()) {
            reqBuilder.addHeader("X-Batch-Id", batchId)
        }

        if (authToken.isNotEmpty()) {
            reqBuilder.addHeader("X-Flick-Token", authToken)
        }

        return try {
            client.newCall(reqBuilder.build()).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Upload failed with HTTP ${resp.code}"))
                }
                val bodyStr = resp.body?.string() ?: "{}"
                val json = JSONObject(bodyStr)
                val formatsArr = json.optJSONArray("clipboard_formats")
                val formatsList = mutableListOf<String>()
                if (formatsArr != null) {
                    for (i in 0 until formatsArr.length()) {
                        formatsList.add(formatsArr.getString(i))
                    }
                }
                Result.success(
                    UploadResult(
                        savedPath = json.optString("saved_path"),
                        bytesWritten = json.optLong("bytes_written"),
                        elapsedMs = json.optLong("elapsed_ms"),
                        clipboardFormats = formatsList
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
}
