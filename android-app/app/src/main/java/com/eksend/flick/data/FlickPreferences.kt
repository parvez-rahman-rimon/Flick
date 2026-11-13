package com.eksend.flick.data

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

class FlickPreferences(private val context: Context) {
    private val prefs: SharedPreferences =
        context.applicationContext.getSharedPreferences("flick_prefs", Context.MODE_PRIVATE)

    companion object {
        private const val KEY_CLIENT_ID = "client_id"
        private const val KEY_CLIENT_NAME = "client_name"
        private const val KEY_ACTIVE_PC_ID = "active_pc_id"
        private const val KEY_PAIRED_PCS = "paired_pcs_json"
        private const val KEY_CLIPBOARD_MODE = "clipboard_mode_override"
        private const val KEY_HAPTIC_ENABLED = "haptic_feedback_enabled"
        private const val KEY_AUTO_UPDATE_IP = "auto_update_ip_enabled"
        private const val KEY_RECENT_TRANSFERS = "recent_transfers_json"
        private const val MAX_TRANSFERS = 20
    }

    val clientId: String
        get() {
            var id = prefs.getString(KEY_CLIENT_ID, null)
            if (id == null) {
                id = UUID.randomUUID().toString()
                prefs.edit().putString(KEY_CLIENT_ID, id).apply()
            }
            return id
        }

    val clientName: String
        get() {
            return prefs.getString(KEY_CLIENT_NAME, null)
                ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"
        }

    var activePcId: String?
        get() = prefs.getString(KEY_ACTIVE_PC_ID, null)
        set(value) = prefs.edit().putString(KEY_ACTIVE_PC_ID, value).apply()

    var clipboardModeOverride: String
        get() = prefs.getString(KEY_CLIPBOARD_MODE, "DEFAULT") ?: "DEFAULT"
        set(value) = prefs.edit().putString(KEY_CLIPBOARD_MODE, value).apply()

    var isHapticFeedbackEnabled: Boolean
        get() = prefs.getBoolean(KEY_HAPTIC_ENABLED, true)
        set(value) = prefs.edit().putBoolean(KEY_HAPTIC_ENABLED, value).apply()

    var isAutoUpdateIpEnabled: Boolean
        get() = prefs.getBoolean(KEY_AUTO_UPDATE_IP, true)
        set(value) = prefs.edit().putBoolean(KEY_AUTO_UPDATE_IP, value).apply()

    fun getPairedPcs(): List<PairedPc> {
        val jsonStr = prefs.getString(KEY_PAIRED_PCS, "[]") ?: "[]"
        val list = mutableListOf<PairedPc>()
        try {
            val arr = JSONArray(jsonStr)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                list.add(
                    PairedPc(
                        pcId = obj.getString("pcId"),
                        pcName = obj.getString("pcName"),
                        lastKnownIp = obj.getString("lastKnownIp"),
                        port = obj.optInt("port", 45455),
                        authToken = obj.optString("authToken", ""),
                        lastSeenEpochMs = obj.optLong("lastSeenEpochMs", System.currentTimeMillis()),
                        pcSpkiPin = obj.optString("pcSpkiPin", ""),
                        pairSecret = obj.optString("pairSecret", "")
                    )
                )
            }
        } catch (_: Exception) {}
        return list
    }

    fun savePairedPc(pc: PairedPc) {
        val current = getPairedPcs().toMutableList()
        val index = current.indexOfFirst { it.pcId == pc.pcId }
        if (index >= 0) {
            current[index] = pc
        } else {
            current.add(pc)
        }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("pcId", item.pcId)
                put("pcName", item.pcName)
                put("lastKnownIp", item.lastKnownIp)
                put("port", item.port)
                put("authToken", item.authToken)
                put("lastSeenEpochMs", item.lastSeenEpochMs)
                put("pcSpkiPin", item.pcSpkiPin)
                put("pairSecret", item.pairSecret)
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()

        // If no active PC, set this as active
        if (activePcId == null || activePcId == pc.pcId) {
            activePcId = pc.pcId
        }
    }

    fun removePairedPc(pcId: String) {
        val current = getPairedPcs().filter { it.pcId != pcId }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("pcId", item.pcId)
                put("pcName", item.pcName)
                put("lastKnownIp", item.lastKnownIp)
                put("port", item.port)
                put("authToken", item.authToken)
                put("lastSeenEpochMs", item.lastSeenEpochMs)
                put("pcSpkiPin", item.pcSpkiPin)
                put("pairSecret", item.pairSecret)
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()
        if (activePcId == pcId) {
            activePcId = current.firstOrNull()?.pcId
        }
    }

    fun getActivePc(): PairedPc? {
        val currentList = getPairedPcs()
        val activeId = activePcId
        return currentList.firstOrNull { it.pcId == activeId } ?: currentList.firstOrNull()
    }

    fun updateLastKnownIp(pcId: String, newIp: String) {
        val pcs = getPairedPcs().toMutableList()
        val idx = pcs.indexOfFirst { it.pcId == pcId }
        if (idx >= 0) {
            val old = pcs[idx]
            pcs[idx] = old.copy(lastKnownIp = newIp, lastSeenEpochMs = System.currentTimeMillis())
            val arr = JSONArray()
            for (item in pcs) {
                val obj = JSONObject().apply {
                    put("pcId", item.pcId)
                    put("pcName", item.pcName)
                    put("lastKnownIp", item.lastKnownIp)
                    put("port", item.port)
                    put("authToken", item.authToken)
                    put("lastSeenEpochMs", item.lastSeenEpochMs)
                    put("pcSpkiPin", item.pcSpkiPin)
                    put("pairSecret", item.pairSecret)
                }
                arr.put(obj)
            }
            prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()
        }
    }

    fun getRecentTransfers(): List<TransferRecord> {
        val jsonStr = prefs.getString(KEY_RECENT_TRANSFERS, "[]") ?: "[]"
        val list = mutableListOf<TransferRecord>()
        try {
            val arr = JSONArray(jsonStr)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                list.add(
                    TransferRecord(
                        id = obj.getString("id"),
                        fileName = obj.getString("fileName"),
                        mimeType = obj.getString("mimeType"),
                        bytes = obj.getLong("bytes"),
                        elapsedMs = obj.getLong("elapsedMs"),
                        success = obj.getBoolean("success"),
                        timestamp = obj.optLong("timestamp", System.currentTimeMillis()),
                        thumbnailPath = obj.optString("thumbnailPath", "").takeIf { it.isNotEmpty() }
                    )
                )
            }
        } catch (_: Exception) {}
        return list
    }

    fun recordTransfer(record: TransferRecord) {
        val current = getRecentTransfers().toMutableList()
        current.add(0, record) // newest first
        if (current.size > MAX_TRANSFERS) {
            current.removeAt(current.size - 1)
        }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("id", item.id)
                put("fileName", item.fileName)
                put("mimeType", item.mimeType)
                put("bytes", item.bytes)
                put("elapsedMs", item.elapsedMs)
                put("success", item.success)
                put("timestamp", item.timestamp)
                put("thumbnailPath", item.thumbnailPath ?: "")
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_RECENT_TRANSFERS, arr.toString()).apply()
    }

    fun clearRecentTransfers() {
        prefs.edit().putString(KEY_RECENT_TRANSFERS, "[]").apply()
        try {
            val thumbDir = java.io.File(context.cacheDir, "thumbnails")
            if (thumbDir.exists()) {
                thumbDir.deleteRecursively()
            }
        } catch (_: Exception) {}
    }
}
