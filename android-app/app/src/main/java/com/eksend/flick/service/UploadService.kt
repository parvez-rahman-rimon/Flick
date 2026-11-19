package com.eksend.flick.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.provider.OpenableColumns
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.eksend.flick.R
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.TransferRecord
import com.eksend.flick.network.FlickHttpClient
import com.eksend.flick.network.PcAutoRouter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class UploadService : Service() {

    private val serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    companion object {
        const val ACTION_UPLOAD = "com.eksend.flick.action.UPLOAD"
        const val EXTRA_URIS = "extra_uris"
        const val EXTRA_TEXT = "extra_text"
        const val EXTRA_CLIPBOARD_MODE = "extra_clipboard_mode"
        const val EXTRA_TARGET_PC_ID = "extra_target_pc_id"

        private const val CHANNEL_ID = "flick_transfers"
        private const val NOTIFICATION_ID = 45455
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onDestroy() {
        UploadProgressBus.clear()
        serviceScope.cancel()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notification = buildForegroundNotification("Sending to PC...")
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }

        if (intent != null) {
            handleIntent(intent)
        } else {
            stopSelf()
        }

        return START_NOT_STICKY
    }

    private fun handleIntent(intent: Intent) {
        val prefs = FlickPreferences(applicationContext)
        val pc = prefs.getActivePc()

        if (pc == null) {
            HapticHelper.triggerError(applicationContext, "No PC paired. Open Flick to pair.")
            stopSelf()
            return
        }

        // Proactively ensure client keypair exists in Keystore for mTLS
        com.eksend.flick.crypto.KeystoreManager.ensureClientKeypair(prefs.clientId)

        val textPayload = intent.getStringExtra(EXTRA_TEXT)
        val uris: List<Uri> = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(EXTRA_URIS, Uri::class.java) ?: emptyList()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra(EXTRA_URIS) ?: emptyList()
        }
        val modeOverride = intent.getStringExtra(EXTRA_CLIPBOARD_MODE) ?: prefs.clipboardModeOverride

        serviceScope.launch {
            // 1. Resolve Target IP with automatic ping & fallback
            val targetIp = PcAutoRouter.resolveTargetIp(applicationContext, pc)
            if (targetIp.isNullOrBlank()) {
                HapticHelper.triggerError(applicationContext, "${pc.pcName} is offline or Flick is not running")
                UploadProgressBus.clear()
                stopSelf()
                return@launch
            }

            // 2. Handle Plain Text Payload
            if (!textPayload.isNullOrEmpty() && uris.isEmpty()) {
                val start = System.currentTimeMillis()
                val result = FlickHttpClient.sendClipboardText(
                    targetIp = targetIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    pcSpkiPin = pc.pcSpkiPin,
                    text = textPayload
                )
                val elapsed = System.currentTimeMillis() - start
                if (result.isSuccess) {
                    prefs.recordTransfer(
                        TransferRecord(
                            fileName = if (textPayload.length > 30) textPayload.take(30) + "..." else textPayload,
                            mimeType = "text/plain",
                            bytes = textPayload.toByteArray().size.toLong(),
                            elapsedMs = elapsed,
                            success = true
                        )
                    )
                    HapticHelper.triggerSuccess(applicationContext)
                } else {
                    HapticHelper.triggerError(applicationContext, "Text teleport failed")
                }
                stopSelf()
                return@launch
            }

            // 3. Handle File URIs Payload
            if (uris.isNotEmpty()) {
                val totalCount = uris.size
                val batchId = if (totalCount > 1) java.util.UUID.randomUUID().toString() else null
                val notifManager = getSystemService(NotificationManager::class.java)
                var successCount = 0

                for ((index, uri) in uris.withIndex()) {
                    val metadata = queryUriMetadata(uri)
                    UploadProgressBus.updateProgress(
                        fileName = metadata.name,
                        bytesSent = 0L,
                        totalBytes = metadata.size,
                        speedBps = 0L,
                        batchIndex = index,
                        batchTotal = totalCount
                    )

                    if (totalCount > 1) {
                        notifManager?.notify(
                            NOTIFICATION_ID,
                            buildForegroundNotification("Sending ${index + 1}/$totalCount: ${metadata.name}")
                        )
                    }
                    val start = System.currentTimeMillis()

                    val uploadRes = FlickHttpClient.uploadStream(
                        targetIp = targetIp,
                        port = pc.port,
                        authToken = pc.authToken,
                        pcSpkiPin = pc.pcSpkiPin,
                        fileName = metadata.name,
                        mimeType = metadata.mimeType,
                        clipboardMode = modeOverride,
                        batchId = batchId,
                        batchIndex = index,
                        batchTotal = totalCount,
                        contentLength = metadata.size,
                        onProgress = { sent, total, speed ->
                            UploadProgressBus.updateProgress(
                                fileName = metadata.name,
                                bytesSent = sent,
                                totalBytes = total,
                                speedBps = speed,
                                batchIndex = index,
                                batchTotal = totalCount
                            )
                        }
                    ) {
                        val pfd = try {
                            contentResolver.openFileDescriptor(uri, "r")
                        } catch (_: Exception) {
                            null
                        }
                        val rawStream = if (pfd != null) {
                            android.os.ParcelFileDescriptor.AutoCloseInputStream(pfd)
                        } else {
                            contentResolver.openInputStream(uri)
                                ?: throw java.io.IOException("Cannot open input stream for $uri")
                        }
                        java.io.BufferedInputStream(rawStream, 512 * 1024)
                    }

                    val elapsed = System.currentTimeMillis() - start
                    if (uploadRes.isSuccess) {
                        successCount++
                        val thumbPath = generateThumbnail(uri, metadata.mimeType)
                        prefs.recordTransfer(
                            TransferRecord(
                                fileName = metadata.name,
                                mimeType = metadata.mimeType,
                                bytes = metadata.size,
                                elapsedMs = elapsed,
                                success = true,
                                thumbnailPath = thumbPath
                            )
                        )
                    }
                }

                if (successCount == totalCount) {
                    HapticHelper.triggerSuccess(applicationContext)
                    kotlinx.coroutines.delay(160)
                } else if (successCount > 0) {
                    HapticHelper.triggerError(applicationContext, "Sent $successCount of $totalCount files")
                    kotlinx.coroutines.delay(200)
                } else {
                    HapticHelper.triggerError(applicationContext, "Transfer failed")
                    kotlinx.coroutines.delay(200)
                }
            }

            UploadProgressBus.clear()
            stopSelf()
        }
    }

    private data class UriMetadata(val name: String, val mimeType: String, val size: Long)

    private fun queryUriMetadata(uri: Uri): UriMetadata {
        var name = "flick_${System.currentTimeMillis()}"
        var size = -1L
        val mimeType = contentResolver.getType(uri) ?: "application/octet-stream"

        try {
            contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                if (cursor.moveToFirst()) {
                    if (nameIndex != -1) name = cursor.getString(nameIndex) ?: name
                    if (sizeIndex != -1) size = cursor.getLong(sizeIndex)
                }
            }
        } catch (_: Exception) {}

        if (size <= 0L) {
            try {
                contentResolver.openAssetFileDescriptor(uri, "r")?.use { afd ->
                    if (afd.length > 0L) size = afd.length
                }
            } catch (_: Exception) {}
        }

        return UriMetadata(name, mimeType, size)
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Flick Transfers",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Silent background transfer indicator"
                setShowBadge(false)
                enableLights(false)
                enableVibration(false)
                setSound(null, null)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    private fun buildForegroundNotification(title: String): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText("Teleporting to PC Clipboard...")
            .setSmallIcon(R.drawable.ic_flick_bolt)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setOngoing(true)
            .build()
    }

    private fun generateThumbnail(uri: Uri, mimeType: String): String? {
        val isImage = mimeType.startsWith("image/")
        val isVideo = mimeType.startsWith("video/")
        val isPdf = mimeType == "application/pdf"
        if (!isImage && !isVideo && !isPdf) return null

        return try {
            val thumbDir = java.io.File(cacheDir, "thumbnails").apply { mkdirs() }
            val thumbFile = java.io.File(thumbDir, "${java.util.UUID.randomUUID()}.jpg")

            var bitmap: Bitmap? = null
            val targetDimension = 1080 // Full HD 1080p crisp rendering for modern high-DPI displays

            if (isImage) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    try {
                        val source = ImageDecoder.createSource(contentResolver, uri)
                        bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
                            val sample = maxOf(info.size.width / targetDimension, info.size.height / targetDimension, 1)
                            decoder.setTargetSampleSize(sample)
                            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                        }
                    } catch (_: Exception) {}
                }
                if (bitmap == null) {
                    contentResolver.openInputStream(uri)?.use { stream ->
                        val options = BitmapFactory.Options().apply {
                            inJustDecodeBounds = true
                        }
                        BitmapFactory.decodeStream(stream, null, options)
                        val sampleSize = calculateInSampleSize(options, targetDimension, targetDimension)
                        val decodeOptions = BitmapFactory.Options().apply {
                            inSampleSize = sampleSize
                            inPreferredConfig = Bitmap.Config.ARGB_8888
                        }
                        contentResolver.openInputStream(uri)?.use { s2 ->
                            bitmap = BitmapFactory.decodeStream(s2, null, decodeOptions)
                        }
                    }
                }
            } else if (isVideo) {
                // Decode directly from video stream to avoid low-res MediaStore caches
                val retriever = android.media.MediaMetadataRetriever()
                try {
                    retriever.setDataSource(applicationContext, uri)
                    bitmap = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
                        retriever.getScaledFrameAtTime(-1, android.media.MediaMetadataRetriever.OPTION_CLOSEST_SYNC, targetDimension, targetDimension)
                    } else {
                        null
                    } ?: retriever.getFrameAtTime(0, android.media.MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                } catch (_: Exception) {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                        try {
                            bitmap = contentResolver.loadThumbnail(uri, android.util.Size(targetDimension, targetDimension), null)
                        } catch (_: Exception) {}
                    }
                } finally {
                    try { retriever.release() } catch (_: Exception) {}
                }
            } else if (isPdf) {
                contentResolver.openFileDescriptor(uri, "r")?.use { pfd ->
                    try {
                        val renderer = android.graphics.pdf.PdfRenderer(pfd)
                        if (renderer.pageCount > 0) {
                            val page = renderer.openPage(0)
                            val scale = targetDimension.toFloat() / maxOf(page.width, page.height).coerceAtLeast(1)
                            val targetW = (page.width * scale).toInt().coerceAtLeast(1)
                            val targetH = (page.height * scale).toInt().coerceAtLeast(1)
                            val bmp = Bitmap.createBitmap(targetW, targetH, Bitmap.Config.ARGB_8888)
                            bmp.eraseColor(android.graphics.Color.WHITE)
                            page.render(bmp, null, null, android.graphics.pdf.PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                            page.close()
                            renderer.close()
                            bitmap = bmp
                        } else {
                            renderer.close()
                        }
                    } catch (_: Exception) {}
                }
            }

            if (bitmap != null) {
                val maxDim = maxOf(bitmap.width, bitmap.height)
                val targetBmp = if (maxDim > targetDimension) {
                    val scale = targetDimension.toFloat() / maxDim
                    Bitmap.createScaledBitmap(
                        bitmap,
                        (bitmap.width * scale).toInt().coerceAtLeast(1),
                        (bitmap.height * scale).toInt().coerceAtLeast(1),
                        true
                    )
                } else {
                    bitmap
                }

                java.io.FileOutputStream(thumbFile).use { out ->
                    targetBmp.compress(Bitmap.CompressFormat.JPEG, 95, out)
                }
                thumbFile.absolutePath
            } else {
                null
            }
        } catch (_: Exception) {
            null
        }
    }

    private fun calculateInSampleSize(
        options: android.graphics.BitmapFactory.Options,
        reqWidth: Int,
        reqHeight: Int
    ): Int {
        val height = options.outHeight
        val width = options.outWidth
        var inSampleSize = 1
        if (height > reqHeight || width > reqWidth) {
            val halfHeight = height / 2
            val halfWidth = width / 2
            while ((halfHeight / inSampleSize) >= reqHeight && (halfWidth / inSampleSize) >= reqWidth) {
                inSampleSize *= 2
            }
        }
        return inSampleSize
    }
}
