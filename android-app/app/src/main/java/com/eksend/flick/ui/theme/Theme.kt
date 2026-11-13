package com.eksend.flick.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// =========================================================================
// TEENAGE ENGINEERING HARDWARE PALETTE (FLICK TX-1)
// International Safety Orange (#FF4400) on Precision Machined Chassis
// =========================================================================

val FlickDarkColors = darkColorScheme(
    primary = Color(0xFFFF4400),            // International Safety Orange (Iconic TE Accent)
    onPrimary = Color(0xFFFFFFFF),          // Pure White
    primaryContainer = Color(0xFF38140A),   // Deep warm ember/orange groove
    onPrimaryContainer = Color(0xFFFF8555),
    secondary = Color(0xFF9EA3AD),          // Industrial Anodized Matte Silver
    onSecondary = Color(0xFF141618),
    secondaryContainer = Color(0xFF22252C), // Tactile Beveled Keycap Slate
    onSecondaryContainer = Color(0xFFF0F2F5),
    tertiary = Color(0xFFFFB700),           // Signal Amber / Speed Warning
    onTertiary = Color(0xFF1F1500),
    tertiaryContainer = Color(0xFF3A2B00),
    onTertiaryContainer = Color(0xFFFFE082),
    background = Color(0xFF0A0B0E),         // Deep Machined Obsidian Chassis
    onBackground = Color(0xFFEDEDED),
    surface = Color(0xFF131519),            // Recessed Hardware Plate / CRT Panel
    onSurface = Color(0xFFEDEDED),
    surfaceVariant = Color(0xFF1A1D23),     // Module / Beveled Sub-Chassis
    onSurfaceVariant = Color(0xFFA6ABB6),
    outline = Color(0xFF2C303A),            // Machined Chamfer Border
    outlineVariant = Color(0xFF1E2128),
    error = Color(0xFFFF3333),
    onError = Color(0xFFFFFFFF)
)

val FlickLightColors = lightColorScheme(
    primary = Color(0xFFFF4400),            // International Safety Orange
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFFFDACF),   // Soft Coral Orange
    onPrimaryContainer = Color(0xFF3C0E00),
    secondary = Color(0xFF5A5E66),          // Industrial Matte Graphite
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFDDD9D1), // Field Chalk / Off-White Plastic Keycaps
    onSecondaryContainer = Color(0xFF1A1C1E),
    tertiary = Color(0xFF9E6500),           // Amber Signal Accent
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFDF9E),
    onTertiaryContainer = Color(0xFF261900),
    background = Color(0xFFEFECE6),         // Warm Bone / Field Chalk Chassis (OP-1 Style)
    onBackground = Color(0xFF191B1D),
    surface = Color(0xFFE7E3DB),            // Brushed Off-White Metal Surface
    onSurface = Color(0xFF191B1D),
    surfaceVariant = Color(0xFFDDD9D0),     // Recessed Module Well
    onSurfaceVariant = Color(0xFF4C5058),
    outline = Color(0xFFB8B4AA),            // Crisp Outer Seam
    outlineVariant = Color(0xFFCBC7BD),
    error = Color(0xFFBA1A1A),
    onError = Color(0xFFFFFFFF)
)

@Composable
fun FlickTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = false, // Pure Teenage Engineering Safety Orange identity!
    content: @Composable () -> Unit
) {
    val colorScheme = if (darkTheme) FlickDarkColors else FlickLightColors

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography,
        content = content
    )
}
