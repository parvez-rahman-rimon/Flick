package com.eksend.flick.service

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.VibrationAttributes
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.widget.Toast
import com.eksend.flick.data.FlickPreferences

object HapticHelper {

    private fun getVibrator(context: Context): Vibrator? {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val vibratorManager = context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager
            vibratorManager?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }
    }

    private fun performVibration(context: Context, timings: LongArray, amplitudes: IntArray? = null) {
        val prefs = FlickPreferences(context)
        if (!prefs.isHapticFeedbackEnabled) return

        try {
            val vibrator = getVibrator(context) ?: return
            if (!vibrator.hasVibrator()) return

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                val attrs = VibrationAttributes.Builder()
                    .setUsage(VibrationAttributes.USAGE_TOUCH)
                    .build()
                val effect = if (amplitudes != null && amplitudes.size == timings.size) {
                    VibrationEffect.createWaveform(timings, amplitudes, -1)
                } else if (timings.size == 2) {
                    VibrationEffect.createOneShot(timings[1], 240)
                } else {
                    VibrationEffect.createWaveform(timings, -1)
                }
                vibrator.vibrate(effect, attrs)
            } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val effect = if (amplitudes != null && amplitudes.size == timings.size) {
                    VibrationEffect.createWaveform(timings, amplitudes, -1)
                } else if (timings.size == 2) {
                    VibrationEffect.createOneShot(timings[1], 240)
                } else {
                    VibrationEffect.createWaveform(timings, -1)
                }
                vibrator.vibrate(effect)
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(timings, -1)
            }
        } catch (_: Exception) {}
    }

    fun triggerQuickTick(context: Context) {
        triggerSpatialFlick(context)
    }

    fun triggerSpatialFlick(context: Context) {
        val prefs = FlickPreferences(context)
        if (!prefs.isHapticFeedbackEnabled) return

        try {
            val vibrator = getVibrator(context) ?: return
            if (!vibrator.hasVibrator()) return

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                // 45ms velvety pulse with USAGE_TOUCH (guaranteed execution even when notification vibrations are OFF)
                val effect = VibrationEffect.createOneShot(45, VibrationEffect.DEFAULT_AMPLITUDE)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    val attrs = VibrationAttributes.Builder()
                        .setUsage(VibrationAttributes.USAGE_TOUCH)
                        .build()
                    vibrator.vibrate(effect, attrs)
                } else {
                    vibrator.vibrate(effect)
                }
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(45)
            }
        } catch (_: Exception) {
            try {
                val vibrator = getVibrator(context)
                @Suppress("DEPRECATION")
                vibrator?.vibrate(45)
            } catch (_: Exception) {}
        }
    }

    fun triggerSuccess(context: Context, customToast: String? = "⚡ Copied to PC Clipboard") {
        // Distinctive tactile double pulse: [45ms pulse, 50ms pause, 65ms crisp pulse]
        performVibration(
            context,
            longArrayOf(0, 45, 50, 65),
            intArrayOf(0, 200, 0, 255)
        )

        if (customToast != null) {
            Handler(Looper.getMainLooper()).post {
                Toast.makeText(context, customToast, Toast.LENGTH_SHORT).show()
            }
        }
    }

    fun triggerPairSuccess(context: Context, pcName: String) {
        // Heavy pairing success celebration pulse: [60ms pulse, 60ms pause, 85ms pulse]
        performVibration(
            context,
            longArrayOf(0, 60, 60, 85),
            intArrayOf(0, 220, 0, 255)
        )

        Handler(Looper.getMainLooper()).post {
            Toast.makeText(context, "✅ Paired with $pcName", Toast.LENGTH_SHORT).show()
        }
    }

    fun triggerError(context: Context, message: String) {
        // Triple sharp warning pulse: [50ms, 40ms, 50ms, 40ms, 50ms]
        performVibration(
            context,
            longArrayOf(0, 50, 40, 50, 40, 50),
            intArrayOf(0, 255, 0, 255, 0, 255)
        )

        Handler(Looper.getMainLooper()).post {
            Toast.makeText(context, "❌ $message", Toast.LENGTH_LONG).show()
        }
    }
}
