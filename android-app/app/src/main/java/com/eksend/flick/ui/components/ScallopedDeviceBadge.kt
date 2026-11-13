package com.eksend.flick.ui.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * Teenage Engineering TP-7 Motorized Spool (Tape Reel Mechanism).
 * Features brushed gunmetal disc, concentric tape grooves, 3 radial spoke cutouts,
 * and an iconic International Safety Orange (#FF4400) center spindle with lightning bolt.
 */
@Composable
fun ScallopedDeviceBadge(
    isOnline: Boolean,
    modifier: Modifier = Modifier,
    size: Dp = 56.dp,
    isSpinning: Boolean = false,
    onClick: (() -> Unit)? = null
) {
    val infiniteTransition = rememberInfiniteTransition(label = "SpoolSpinTransition")
    val rotationAngle by if (isSpinning) {
        infiniteTransition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 1200, easing = LinearEasing),
                repeatMode = RepeatMode.Restart
            ),
            label = "TapeSpoolFast"
        )
    } else if (isOnline) {
        infiniteTransition.animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(
                animation = tween(durationMillis = 10000, easing = LinearEasing),
                repeatMode = RepeatMode.Restart
            ),
            label = "TapeSpoolIdle"
        )
    } else {
        androidx.compose.runtime.remember { androidx.compose.runtime.mutableFloatStateOf(0f) }
    }

    val orangeColor = Color(0xFFFF4400)
    val outerPlatterColor = Color(0xFF181A1E)
    val rimBorderColor = Color(0xFF383D48)
    val spokeColor = Color(0xFF555B68)

    Box(
        modifier = modifier
            .size(size)
            .then(if (onClick != null) Modifier.clickable { onClick() } else Modifier),
        contentAlignment = Alignment.Center
    ) {
        Canvas(modifier = Modifier.matchParentSize()) {
            val cx = this.size.width / 2f
            val cy = this.size.height / 2f
            val radius = minOf(cx, cy) - 2.dp.toPx()

            // 1. Brushed Gunmetal Outer Disc Platter
            drawCircle(
                color = outerPlatterColor,
                radius = radius,
                center = Offset(cx, cy)
            )

            // 2. Machined Outer Edge Rim Stroke
            drawCircle(
                color = if (isOnline) rimBorderColor else Color(0xFF262930),
                radius = radius,
                center = Offset(cx, cy),
                style = Stroke(width = 2.dp.toPx())
            )

            // 3. Concentric Tape Reel Groove (Subtle mechanical ring)
            drawCircle(
                color = Color.White.copy(alpha = 0.08f),
                radius = radius * 0.72f,
                center = Offset(cx, cy),
                style = Stroke(width = 1.dp.toPx())
            )

            // 4. 3 Precision Radial Spoke Cuts (Rotates during streaming)
            rotate(degrees = rotationAngle, pivot = Offset(cx, cy)) {
                for (i in 0..2) {
                    val angle = (i * 120f) * (PI / 180f).toFloat()
                    val startX = cx + (radius * 0.38f) * cos(angle)
                    val startY = cy + (radius * 0.38f) * sin(angle)
                    val endX = cx + (radius * 0.82f) * cos(angle)
                    val endY = cy + (radius * 0.82f) * sin(angle)

                    drawLine(
                        color = spokeColor,
                        start = Offset(startX, startY),
                        end = Offset(endX, endY),
                        strokeWidth = 2.5.dp.toPx(),
                        cap = StrokeCap.Round
                    )
                }
            }

            // 5. Central Safety Orange Drive Spindle Hub
            val hubRadius = radius * 0.38f
            drawCircle(
                color = if (isOnline) orangeColor else Color(0xFF2B2E36),
                radius = hubRadius,
                center = Offset(cx, cy)
            )

            // Subtle highlight rim on the center hub
            if (isOnline) {
                drawCircle(
                    color = Color.White.copy(alpha = 0.4f),
                    radius = hubRadius,
                    center = Offset(cx, cy),
                    style = Stroke(width = 1.dp.toPx())
                )
            }
        }

        // Center Lightning Bolt Glyph (Crisp White)
        Icon(
            imageVector = Icons.Rounded.Bolt,
            contentDescription = "Sync",
            tint = if (isOnline) Color.White else Color(0xFF888F9C),
            modifier = Modifier.size(size * 0.38f)
        )
    }
}
