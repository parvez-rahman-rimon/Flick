package com.eksend.flick

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.core.content.ContextCompat
import com.eksend.flick.service.HapticHelper
import com.eksend.flick.service.UploadService

class TeleportShareActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        try {
            val intentAction = intent?.action
            val uriList = ArrayList<Uri>()
            var textPayload: String? = null

            when (intentAction) {
                Intent.ACTION_SEND -> {
                    val streamUri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
                    }

                    if (streamUri != null) {
                        uriList.add(streamUri)
                    } else {
                        textPayload = intent.getStringExtra(Intent.EXTRA_TEXT)
                            ?: intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
                    }
                }

                Intent.ACTION_SEND_MULTIPLE -> {
                    val incoming = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
                    }
                    if (incoming != null) {
                        uriList.addAll(incoming)
                    }
                }
            }

            // Security Hardening: Only accept content:// URIs and reject Flick's own authority
            // Prevents malicious co-resident apps from forcing Flick to exfiltrate its private files
            val safeUris = ArrayList(uriList.filter { uri ->
                uri.scheme.equals("content", ignoreCase = true) &&
                !uri.authority.equals(packageName, ignoreCase = true)
            })

            if (safeUris.isNotEmpty() || !textPayload.isNullOrEmpty()) {
                HapticHelper.triggerQuickTick(this)
                val serviceIntent = Intent(this, UploadService::class.java).apply {
                    action = UploadService.ACTION_UPLOAD
                    putParcelableArrayListExtra(UploadService.EXTRA_URIS, safeUris)
                    putExtra(UploadService.EXTRA_TEXT, textPayload)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }

                // Explicitly attach ClipData to pass temporary URI read permissions to the service
                if (safeUris.isNotEmpty()) {
                    val clipData = ClipData.newUri(contentResolver, "FlickFiles", safeUris[0])
                    for (i in 1 until safeUris.size) {
                        clipData.addItem(ClipData.Item(safeUris[i]))
                    }
                    serviceIntent.clipData = clipData
                }

                ContextCompat.startForegroundService(this, serviceIntent)
            }
        } finally {
            // Dismiss instantly in < 50ms with zero transition animation
            finish()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
            } else {
                @Suppress("DEPRECATION")
                overridePendingTransition(0, 0)
            }
        }
    }
}
