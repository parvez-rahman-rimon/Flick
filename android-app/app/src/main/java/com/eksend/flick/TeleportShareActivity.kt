package com.eksend.flick

import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.service.UploadService
import com.eksend.flick.ui.components.SpatialFlickDeck
import com.eksend.flick.ui.theme.FlickTheme

class TeleportShareActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

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
        val safeUris = ArrayList(uriList.filter { uri ->
            uri.scheme.equals("content", ignoreCase = true) &&
            !uri.authority.equals(packageName, ignoreCase = true)
        })

        if (safeUris.isEmpty() && textPayload.isNullOrEmpty()) {
            finish()
            return
        }

        val prefs = FlickPreferences(applicationContext)
        val pc = prefs.getActivePc()
        val pcName = pc?.pcName ?: "Windows PC"

        fun dispatchUpload(toUpload: List<Uri>) {
            val serviceIntent = Intent(this, UploadService::class.java).apply {
                action = UploadService.ACTION_UPLOAD
                putParcelableArrayListExtra(UploadService.EXTRA_URIS, ArrayList(toUpload))
                putExtra(UploadService.EXTRA_TEXT, textPayload)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }

            if (toUpload.isNotEmpty()) {
                val clipData = ClipData.newUri(contentResolver, "FlickFiles", toUpload[0])
                for (i in 1 until toUpload.size) {
                    clipData.addItem(ClipData.Item(toUpload[i]))
                }
                serviceIntent.clipData = clipData
            }

            ContextCompat.startForegroundService(this, serviceIntent)
            finish()
        }

        setContent {
            FlickTheme {
                SpatialFlickDeck(
                    uris = safeUris,
                    textPayload = textPayload,
                    pcName = pcName,
                    isOnline = true,
                    onFlick = { selected, _ ->
                        dispatchUpload(selected)
                    },
                    onDismiss = {
                        finish()
                    }
                )
            }
        }
    }
}
