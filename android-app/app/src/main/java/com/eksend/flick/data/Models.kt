package com.eksend.flick.data

import java.util.UUID

enum class ClipboardMode(val id: String, val title: String, val subtitle: String) {
    SMART_MULTI("SMART_MULTI", "Smart Multi", "File + Path + Bitmap"),
    FORCE_PATH("FORCE_PATH", "Force Path", "Plain text file path only"),
    FORCE_IMAGE("FORCE_IMAGE", "Force Image", "Bitmap + File (no path text)"),
    FORCE_FILE("FORCE_FILE", "Force File", "Explorer CF_HDROP only");

    companion object {
        fun fromId(id: String): ClipboardMode {
            return entries.firstOrNull { it.id.equals(id, ignoreCase = true) } ?: SMART_MULTI
        }
    }
}

data class PairedPc(
    val pcId: String,
    val pcName: String,
    val lastKnownIp: String,
    val port: Int = 45455,
    val authToken: String = "",
    val lastSeenEpochMs: Long = System.currentTimeMillis(),
    val pcSpkiPin: String = "",
    val pairSecret: String = ""
)

data class DiscoveredPc(
    val pcId: String,
    val pcName: String,
    val ip: String,
    val port: Int = 45455,
    val version: String = "2.0.0",
    val timestamp: Long = System.currentTimeMillis(),
    val authTag: String = "",
    val authTags: Map<String, String> = emptyMap()
)

data class TransferRecord(
    val id: String = UUID.randomUUID().toString(),
    val fileName: String,
    val mimeType: String,
    val bytes: Long,
    val elapsedMs: Long,
    val success: Boolean,
    val timestamp: Long = System.currentTimeMillis(),
    val thumbnailPath: String? = null
)

data class PcOnlineState(
    val isOnline: Boolean = false,
    val pingMs: Long = -1,
    val clipboardMode: ClipboardMode = ClipboardMode.SMART_MULTI,
    val lastChecked: Long = System.currentTimeMillis()
)
