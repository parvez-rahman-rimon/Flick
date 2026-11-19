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
                    VibrationEffect.createOneShot(timings[1], 110)
                } else {
                    VibrationEffect.createWaveform(timings, -1)
                }
                vibrator.vibrate(effect, attrs)
            } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val effect = if (amplitudes != null && amplitudes.size == timings.size) {
                    VibrationEffect.createWaveform(timings, amplitudes, -1)
                } else if (timings.size == 2) {
                    VibrationEffect.createOneShot(timings[1], 110)
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

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q &&
                vibrator.areAllEffectsSupported(VibrationEffect.EFFECT_CLICK) == Vibrator.VIBRATION_EFFECT_SUPPORT_YES
            ) {
                val effect = VibrationEffect.createPredefined(VibrationEffect.EFFECT_CLICK)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    val attrs = VibrationAttributes.Builder()
                        .setUsage(VibrationAttributes.USAGE_TOUCH)
                        .build()
                    vibrator.vibrate(effect, attrs)
                } else {
                    vibrator.vibrate(effect)
                }
            } else {
                performVibration(
                    context,
                    longArrayOf(0, 30),
                    intArrayOf(0, 80)
                )
            }
        } catch (_: Exception) {
            performVibration(context, longArrayOf(0, 25), intArrayOf(0, 80))
        }
    }

    fun triggerSuccess(context: Context, customToast: String? = null) {
        val prefs = FlickPreferences(context)
        if (!prefs.isHapticFeedbackEnabled) return

        try {
            val vibrator = getVibrator(context)
            if (vibrator != null && vibrator.hasVibrator()) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q &&
                    vibrator.areAllEffectsSupported(VibrationEffect.EFFECT_DOUBLE_CLICK) == Vibrator.VIBRATION_EFFECT_SUPPORT_YES
                ) {
                    // Pristine, silky-smooth native hardware double-click haptic
                    val effect = VibrationEffect.createPredefined(VibrationEffect.EFFECT_DOUBLE_CLICK)
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        val attrs = VibrationAttributes.Builder()
                            .setUsage(VibrationAttributes.USAGE_TOUCH)
                            .build()
                        vibrator.vibrate(effect, attrs)
                    } else {
                        vibrator.vibrate(effect)
                    }
                } else {
                    // Soft, warm double-pulse fallback
                    performVibration(
                        context,
                        longArrayOf(0, 25, 45, 35),
                        intArrayOf(0, 60, 0, 95)
                    )
                }
            }
        } catch (_: Exception) {
            performVibration(
                context,
                longArrayOf(0, 25, 45, 35),
                intArrayOf(0, 60, 0, 95)
            )
        }

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
            Toast.makeText(context, "Paired with $pcName", Toast.LENGTH_SHORT).show()
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
            Toast.makeText(context, message, Toast.LENGTH_LONG).show()
        }
    }
}
