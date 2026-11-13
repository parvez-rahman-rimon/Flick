package com.eksend.flick.ui.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Teenage Engineering 3D Tactile Mechanical Keycap Button.
 * Recreates genuine physical push-button depression (5dp travel),
 * mechanical bottom shadow bevel, and high-contrast machined keycap gradients.
 */
@Composable
fun BentoActionTile(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    subLabel: String = "",
    functionKey: String = "",
    isAccentOrange: Boolean = false,
    height: Dp = 96.dp
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()

    // Tactile button travel animation (moves down by 5dp into chassis)
    val pressOffsetY by animateDpAsState(
        targetValue = if (isPressed) 5.dp else 0.dp,
        animationSpec = tween(durationMillis = 50),
        label = "KeycapPressTravel"
    )

    val shadowLipHeight by animateDpAsState(
        targetValue = if (isPressed) 1.dp else 6.dp,
        animationSpec = tween(durationMillis = 50),
        label = "KeycapShadowLip"
    )

    // Teenage Engineering Color Tokens with DRAMATIC 3D Contrast:
    // Base mechanical socket shadow under the keycap
    val baseShadowColor = if (isAccentOrange) Color(0xFF7A1E00) else Color(0xFF08090C)

    // Keycap Top Surface Gradient (High contrast so dark keycap pops in 3D!)
    val keycapGradient = if (isAccentOrange) {
        if (isPressed) {
            Brush.verticalGradient(listOf(Color(0xFFE63E00), Color(0xFFCC3300)))
        } else {
            Brush.verticalGradient(listOf(Color(0xFFFF5500), Color(0xFFE63800)))
        }
    } else {
        if (isPressed) {
            Brush.verticalGradient(listOf(Color(0xFF22252C), Color(0xFF181A1F)))
        } else {
            // Distinct machined graphite/gunmetal so it stands out from the dark chassis!
            Brush.verticalGradient(listOf(Color(0xFF353944), Color(0xFF22252D)))
        }
    }

    // Top highlight chamfer edge
    val highlightStroke = if (isAccentOrange) {
        Color.White.copy(alpha = 0.4f)
    } else {
        Color(0xFF4C5260)
    }

    val contentColor = Color.White

    // Outer mechanical socket housing
    Box(
        modifier = modifier
            .height(height)
            .clip(RoundedCornerShape(20.dp))
            .background(baseShadowColor)
            .border(1.dp, if (isAccentOrange) Color(0xFF992600) else Color(0xFF1E2128), RoundedCornerShape(20.dp))
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick
            )
    ) {
        // Upper Physical Keycap Plunger (Depresses into socket on tap)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(height - shadowLipHeight)
                .offset(y = pressOffsetY)
                .clip(RoundedCornerShape(20.dp))
                .background(keycapGradient)
                .border(1.dp, highlightStroke, RoundedCornerShape(20.dp))
                .padding(horizontal = 16.dp, vertical = 12.dp)
        ) {
            Column(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.SpaceBetween
            ) {
                // Top row: Function key tag (F1 / F2) & Action Icon
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (functionKey.isNotEmpty()) {
                        Surface(
                            shape = RoundedCornerShape(6.dp),
                            color = if (isAccentOrange) Color.Black.copy(alpha = 0.35f) else Color(0xFF14161B),
                            border = androidx.compose.foundation.BorderStroke(
                                1.dp,
                                if (isAccentOrange) Color.White.copy(alpha = 0.25f) else Color(0xFF3E4350)
                            )
                        ) {
                            Text(
                                text = functionKey,
                                modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.Bold,
                                color = if (isAccentOrange) Color.White else Color(0xFFFF5500)
                            )
                        }
                    } else {
                        Spacer(modifier = Modifier.size(1.dp))
                    }

                    Icon(
                        imageVector = icon,
                        contentDescription = label,
                        tint = if (isAccentOrange) Color.White else Color(0xFFFF5500),
                        modifier = Modifier.size(24.dp)
                    )
                }

                // Bottom Labels (Ample space so title and subtitle are ALWAYS clearly visible)
                Column(modifier = Modifier.fillMaxWidth()) {
                    Text(
                        text = label.uppercase(),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Black,
                        fontSize = 13.5.sp,
                        letterSpacing = 0.5.sp,
                        color = contentColor,
                        maxLines = 1
                    )
                    if (subLabel.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(2.dp))
                        Text(
                            text = subLabel,
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            fontSize = 9.sp,
                            color = contentColor.copy(alpha = 0.8f),
                            maxLines = 1
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun BentoSquareButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 54.dp,
    containerColor: Color = Color(0xFF262932),
    iconTint: Color = Color.White
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()

    val offsetY by animateDpAsState(
        targetValue = if (isPressed) 4.dp else 0.dp,
        animationSpec = tween(durationMillis = 50),
        label = "SquareButtonTravel"
    )

    Box(
        modifier = modifier
            .size(size)
            .clip(RoundedCornerShape(16.dp))
            .background(Color(0xFF0C0E11))
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                onClick = onClick
            )
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .offset(y = offsetY)
                .clip(RoundedCornerShape(16.dp))
                .background(containerColor)
                .border(1.dp, Color(0xFF3E4350), RoundedCornerShape(16.dp)),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = icon,
                contentDescription = contentDescription,
                tint = iconTint,
                modifier = Modifier.size(22.dp)
            )
        }
    }
}
