package com.eksend.flick.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

data class ActiveUploadState(
    val isUploading: Boolean = false,
    val fileName: String = "",
    val bytesSent: Long = 0L,
    val totalBytes: Long = 0L,
    val speedBps: Long = 0L,
    val batchIndex: Int = 0,
    val batchTotal: Int = 1
) {
    val progressPercent: Int
        get() = if (totalBytes > 0) ((bytesSent * 100) / totalBytes).toInt().coerceIn(0, 100) else 0

    val speedMbPerSec: Double
        get() = speedBps.toDouble() / (1024.0 * 1024.0)

    val etaSeconds: Long
        get() {
            if (speedBps <= 0L || totalBytes <= bytesSent) return 0L
            val remainingBytes = totalBytes - bytesSent
            return (remainingBytes / speedBps).coerceAtLeast(1L)
        }
}

object UploadProgressBus {
    private val _state = MutableStateFlow(ActiveUploadState())
    val state: StateFlow<ActiveUploadState> = _state.asStateFlow()

    fun updateProgress(
        fileName: String,
        bytesSent: Long,
        totalBytes: Long,
        speedBps: Long,
        batchIndex: Int,
        batchTotal: Int
    ) {
        _state.value = ActiveUploadState(
            isUploading = true,
            fileName = fileName,
            bytesSent = bytesSent,
            totalBytes = totalBytes,
            speedBps = speedBps,
            batchIndex = batchIndex,
            batchTotal = batchTotal
        )
    }

    fun clear() {
        _state.value = ActiveUploadState(isUploading = false)
    }
}
