package com.eksend.flick.ui.components

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutLinearInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.PhotoLibrary
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.zIndex
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.eksend.flick.service.HapticHelper
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.InputStream
import kotlin.math.roundToInt

data class FlickCardItem(
    val uri: Uri?,
    val fileName: String,
    val fileSizeStr: String,
    val isImage: Boolean,
    val bitmap: Bitmap? = null
)

@Composable
fun SpatialFlickDeck(
    uris: List<Uri>,
    textPayload: String? = null,
    pcName: String = "PC",
    isOnline: Boolean = true,
    onFlick: (selectedUris: List<Uri>, isSingle: Boolean) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val haptic = LocalHapticFeedback.current

    val deckItems = remember { mutableStateListOf<FlickCardItem>() }

    // Load metadata and preview images for all URIs
    LaunchedEffect(uris, textPayload) {
        deckItems.clear()
        if (uris.isNotEmpty()) {
            withContext(Dispatchers.IO) {
                val loaded = uris.mapIndexed { idx, uri ->
                    val mimeType = context.contentResolver.getType(uri) ?: "application/octet-stream"
                    val isImg = mimeType.startsWith("image/")
                    var name = "Photo ${idx + 1}"
                    var sizeStr = ""
                    try {
                        context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                            val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                            val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                            if (cursor.moveToFirst()) {
                                if (nameIndex != -1) name = cursor.getString(nameIndex) ?: name
                                if (sizeIndex != -1) {
                                    val bytes = cursor.getLong(sizeIndex)
                                    sizeStr = formatBytes(bytes)
                                }
                            }
                        }
                    } catch (_: Exception) {}

                    val bmp = if (isImg) loadThumbnail(context, uri, maxDimension = 800) else null
                    FlickCardItem(
                        uri = uri,
                        fileName = name,
                        fileSizeStr = sizeStr,
                        isImage = isImg,
                        bitmap = bmp
                    )
                }
                withContext(Dispatchers.Main) {
                    deckItems.addAll(loaded)
                }
            }
        } else if (!textPayload.isNullOrEmpty()) {
            deckItems.add(
                FlickCardItem(
                    uri = null,
                    fileName = "Clipboard Snippet",
                    fileSizeStr = "${textPayload.length} characters",
                    isImage = false,
                    bitmap = null
                )
            )
        }
    }

    var focusedIndex by remember { mutableIntStateOf(0) }

    LaunchedEffect(deckItems.size) {
        if (deckItems.isNotEmpty() && focusedIndex >= deckItems.size) {
            focusedIndex = (deckItems.size / 2).coerceAtMost(deckItems.size - 1)
        }
    }

    // Physical drag & shoot animation state
    val dragY = remember { Animatable(0f) }
    var isFlicking by remember { mutableStateOf(false) }

    fun executeFlick(isSingle: Boolean) {
        if (isFlicking) return
        isFlicking = true
        try {
            haptic.performHapticFeedback(HapticFeedbackType.LongPress)
        } catch (_: Exception) {}
        HapticHelper.triggerSpatialFlick(context)

        scope.launch {
            // Rapid upward launch off screen
            dragY.animateTo(
                targetValue = -2000f,
                animationSpec = tween(durationMillis = 280, easing = FastOutLinearInEasing)
            )
            delay(120)
            val activeItem = deckItems.getOrNull(focusedIndex)
            val toSend = if (isSingle && activeItem?.uri != null) {
                listOf(activeItem.uri)
            } else if (isSingle && uris.isNotEmpty()) {
                listOf(uris.first())
            } else {
                uris
            }
            onFlick(toSend, isSingle)
        }
    }

    // Physical Wrist Flick & Phone Shake Sensor Detection
    val sensorManager = remember { context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager }
    DisposableEffect(sensorManager, isFlicking) {
        val accelerometer = sensorManager?.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
        val listener = object : SensorEventListener {
            private var lastFlickTime = 0L

            override fun onSensorChanged(event: SensorEvent?) {
                if (event == null || isFlicking) return
                val x = event.values[0]
                val y = event.values[1]
                val z = event.values[2]

                // Total acceleration magnitude normalized against Earth gravity
                val gForce = kotlin.math.sqrt((x * x + y * y + z * z).toDouble()).toFloat() / SensorManager.GRAVITY_EARTH

                val now = System.currentTimeMillis()
                // Physical wrist flick towards PC or upward shake:
                // 1) gForce exceeds 1.55g (approx 15.2 m/s^2)
                // 2) or sharp upward acceleration jerk (y > 14.5 m/s^2)
                // 3) or forward flick tilt snap (z > 14.0 m/s^2)
                if ((gForce > 1.55f || y > 14.5f || z > 14.0f) && (now - lastFlickTime > 1200)) {
                    lastFlickTime = now
                    executeFlick(isSingle = false)
                }
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) {}
        }

        if (accelerometer != null) {
            sensorManager?.registerListener(listener, accelerometer, SensorManager.SENSOR_DELAY_GAME)
        }

        onDispose {
            sensorManager?.unregisterListener(listener)
        }
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0xE6080A0E))
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    var isUpwardDrag = false
                    var totalDeltaY = 0f

                    while (true) {
                        val event = awaitPointerEvent(pass = PointerEventPass.Initial)
                        val change = event.changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) break

                        val deltaY = change.position.y - change.previousPosition.y
                        totalDeltaY += deltaY

                        // As soon as user swipes upward anywhere on screen (including over buttons!)
                        if (totalDeltaY < -10f || isUpwardDrag) {
                            isUpwardDrag = true
                            change.consume()
                            if (!isFlicking) {
                                val newY = (dragY.value + deltaY).coerceAtMost(40f)
                                scope.launch { dragY.snapTo(newY) }
                            }
                        }
                    }

                    if (isUpwardDrag) {
                        if (dragY.value < -70f) {
                            executeFlick(isSingle = false)
                        } else {
                            scope.launch {
                                dragY.animateTo(
                                    0f,
                                    spring(
                                        dampingRatio = Spring.DampingRatioMediumBouncy,
                                        stiffness = Spring.StiffnessMedium
                                    )
                                )
                            }
                        }
                    }
                }
            }
            .padding(horizontal = 24.dp, vertical = 20.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier.fillMaxSize(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            // 1. Top HUD Header
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .clip(RoundedCornerShape(12.dp))
                        .background(Color(0xFF161922))
                        .border(1.dp, Color(0xFF282C38), RoundedCornerShape(12.dp))
                        .padding(horizontal = 12.dp, vertical = 6.dp)
                ) {
                    Box(
                        modifier = Modifier
                            .size(8.dp)
                            .clip(CircleShape)
                            .background(if (isOnline) Color(0xFF00E676) else Color(0xFFFF5252))
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Text(
                        text = pcName.uppercase(),
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        color = Color.White
                    )
                }

                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .background(Color(0xFF1E212B))
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Close,
                        contentDescription = "Cancel",
                        tint = Color.White,
                        modifier = Modifier.size(18.dp)
                    )
                }
            }

            // 2. Center 3D Fanned Arc Deck (Tap to Bring Any Photo to Front)
            val currentY = dragY.value
            val deckAlpha = (1f + (currentY / 1200f)).coerceIn(0.15f, 1.0f)
            val deckScale = (1f + (currentY / 2000f)).coerceIn(0.85f, 1.0f)

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .offset { IntOffset(0, currentY.roundToInt()) }
                    .scale(deckScale)
                    .alpha(deckAlpha),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                if (deckItems.isEmpty()) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth(0.80f)
                            .aspectRatio(0.85f)
                            .clip(RoundedCornerShape(24.dp))
                            .background(Color(0xFF191B24))
                    )
                } else if (deckItems.size == 1) {
                    val item = deckItems[0]
                    FannedDeckCard(
                        item = item,
                        index = 0,
                        isFocused = true,
                        diff = 0,
                        modifier = Modifier
                            .fillMaxWidth(0.80f)
                            .aspectRatio(0.85f)
                    )
                } else {
                    // Splayed Fan Deck (Hand of Cards)
                    val density = LocalDensity.current
                    val stepXPx = with(density) { 38.dp.toPx() }
                    val maxSpreadXPx = with(density) { 130.dp.toPx() }
                    val elevateYPx = with(density) { (-16).dp.toPx() }
                    val stepYPx = with(density) { 10.dp.toPx() }

                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(350.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        deckItems.forEachIndexed { index, item ->
                            val diff = index - focusedIndex
                            val absDiff = kotlin.math.abs(diff)

                            // Physics-based spring targets for fanned arc
                            val targetRotation = (diff * 8.5f).coerceIn(-28f, 28f)
                            val targetTranslationX = (diff * stepXPx).coerceIn(-maxSpreadXPx, maxSpreadXPx)
                            val targetTranslationY = if (diff == 0) elevateYPx else (absDiff * stepYPx)
                            val targetScale = if (diff == 0) 1.05f else (1.0f - absDiff * 0.05f).coerceAtLeast(0.84f)
                            val zIndex = if (diff == 0) 100f else (50f - absDiff)

                            val animRotation by animateFloatAsState(
                                targetValue = targetRotation,
                                animationSpec = spring(
                                    dampingRatio = Spring.DampingRatioMediumBouncy,
                                    stiffness = Spring.StiffnessMediumLow
                                ),
                                label = "rot_$index"
                            )
                            val animTransX by animateFloatAsState(
                                targetValue = targetTranslationX,
                                animationSpec = spring(
                                    dampingRatio = Spring.DampingRatioMediumBouncy,
                                    stiffness = Spring.StiffnessMediumLow
                                ),
                                label = "tx_$index"
                            )
                            val animTransY by animateFloatAsState(
                                targetValue = targetTranslationY,
                                animationSpec = spring(
                                    dampingRatio = Spring.DampingRatioMediumBouncy,
                                    stiffness = Spring.StiffnessMediumLow
                                ),
                                label = "ty_$index"
                            )
                            val animScale by animateFloatAsState(
                                targetValue = targetScale,
                                animationSpec = spring(
                                    dampingRatio = Spring.DampingRatioMediumBouncy,
                                    stiffness = Spring.StiffnessMediumLow
                                ),
                                label = "scale_$index"
                            )

                            Box(
                                modifier = Modifier
                                    .zIndex(zIndex)
                                    .graphicsLayer {
                                        this.rotationZ = animRotation
                                        this.translationX = animTransX
                                        this.translationY = animTransY
                                        this.scaleX = animScale
                                        this.scaleY = animScale
                                        this.cameraDistance = 16f * density.density
                                    }
                                    .clickable {
                                        if (focusedIndex != index) {
                                            focusedIndex = index
                                            try {
                                                haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                            } catch (_: Exception) {}
                                        }
                                    },
                                contentAlignment = Alignment.Center
                            ) {
                                FannedDeckCard(
                                    item = item,
                                    index = index,
                                    isFocused = (diff == 0),
                                    diff = diff,
                                    modifier = Modifier
                                        .fillMaxWidth(0.74f)
                                        .aspectRatio(0.85f)
                                )
                            }
                        }
                    }
                }

                Spacer(modifier = Modifier.height(14.dp))

                // Active Card Telemetry & File Name
                val activeItem = deckItems.getOrNull(focusedIndex)
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    modifier = Modifier.padding(horizontal = 24.dp)
                ) {
                    Text(
                        text = activeItem?.fileName ?: "Ready to flick",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                        color = Color.White,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        textAlign = TextAlign.Center
                    )
                    Spacer(modifier = Modifier.height(3.dp))
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.Center
                    ) {
                        Text(
                            text = if (deckItems.size > 1) "PHOTO ${focusedIndex + 1} OF ${deckItems.size}" else "READY TO TELEPORT",
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Bold,
                            color = Color(0xFFFF5722),
                            fontSize = 11.sp
                        )
                        if (!activeItem?.fileSizeStr.isNullOrEmpty()) {
                            Text(
                                text = " • ${activeItem!!.fileSizeStr}",
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = FontFamily.Monospace,
                                color = Color(0xFF9E9E9E),
                                fontSize = 11.sp
                            )
                        }
                    }

                    // Mini Thumbnail Dock (Underneath the cards, just like the reference image!)
                    if (deckItems.size > 1) {
                        Spacer(modifier = Modifier.height(10.dp))
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            deckItems.forEachIndexed { i, itm ->
                                val isSelected = i == focusedIndex
                                Box(
                                    modifier = Modifier
                                        .size(if (isSelected) 34.dp else 26.dp)
                                        .clip(RoundedCornerShape(8.dp))
                                        .background(Color(0xFF222633))
                                        .border(
                                            width = if (isSelected) 2.dp else 1.dp,
                                            color = if (isSelected) Color(0xFFFF5722) else Color(0x44FFFFFF),
                                            shape = RoundedCornerShape(8.dp)
                                        )
                                        .clickable {
                                            if (focusedIndex != i) {
                                                focusedIndex = i
                                                try {
                                                    haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                                } catch (_: Exception) {}
                                            }
                                        }
                                ) {
                                    if (itm.bitmap != null) {
                                        Image(
                                            bitmap = itm.bitmap.asImageBitmap(),
                                            contentDescription = null,
                                            contentScale = ContentScale.Crop,
                                            modifier = Modifier.fillMaxSize()
                                        )
                                    } else {
                                        Box(
                                            modifier = Modifier.fillMaxSize(),
                                            contentAlignment = Alignment.Center
                                        ) {
                                            Text(
                                                text = "${i + 1}",
                                                style = MaterialTheme.typography.labelSmall,
                                                color = Color.White,
                                                fontSize = 9.sp,
                                                fontFamily = FontFamily.Monospace
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 3. Bottom Controls & Declan Tan "Give it a little flick"
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = "Give it a little flick.",
                        style = MaterialTheme.typography.headlineSmall,
                        fontWeight = FontWeight.Black,
                        letterSpacing = 0.5.sp,
                        color = Color.White
                    )
                    Spacer(modifier = Modifier.height(4.dp))
                    Text(
                        text = "Hold your phone or swipe up with a finger.",
                        style = MaterialTheme.typography.bodySmall,
                        color = Color(0xFF9E9E9E)
                    )
                }

                // Action Pill Buttons
                if (uris.size <= 1) {
                    Button(
                        onClick = { executeFlick(isSingle = true) },
                        modifier = Modifier
                            .fillMaxWidth(0.85f)
                            .height(52.dp),
                        shape = RoundedCornerShape(26.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = Color(0xFFE64A19),
                            contentColor = Color.White
                        )
                    ) {
                        Icon(
                            imageVector = Icons.Rounded.Bolt,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp)
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = "Flick to PC",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold
                        )
                    }
                } else {
                    Row(
                        modifier = Modifier.fillMaxWidth(0.92f),
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        // Left Pill: Flick Single
                        Button(
                            onClick = { executeFlick(isSingle = true) },
                            modifier = Modifier
                                .weight(1f)
                                .height(50.dp),
                            shape = RoundedCornerShape(25.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = Color(0xFF262A36),
                                contentColor = Color.White
                            ),
                            border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF42475A))
                        ) {
                            Text(
                                text = "Flick single",
                                style = MaterialTheme.typography.bodyMedium,
                                fontWeight = FontWeight.Bold
                            )
                        }

                        // Right Pill: Whole Stack
                        Button(
                            onClick = { executeFlick(isSingle = false) },
                            modifier = Modifier
                                .weight(1.2f)
                                .height(50.dp),
                            shape = RoundedCornerShape(25.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = Color(0xFFE64A19),
                                contentColor = Color.White
                            )
                        ) {
                            Icon(
                                imageVector = Icons.Rounded.Bolt,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp)
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(
                                text = "Whole stack",
                                style = MaterialTheme.typography.bodyMedium,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    }
                }

                // Subtle Cancel Button
                Text(
                    text = "Put papers back",
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = Color(0xFF757575),
                    modifier = Modifier
                        .clickable { onDismiss() }
                        .padding(horizontal = 16.dp, vertical = 6.dp)
                )
            }
        }
    }
}

@Composable
private fun FannedDeckCard(
    item: FlickCardItem,
    index: Int,
    isFocused: Boolean,
    diff: Int,
    modifier: Modifier = Modifier
) {
    Box(
        modifier = modifier
            .shadow(
                elevation = if (isFocused) 32.dp else 10.dp,
                shape = RoundedCornerShape(22.dp),
                spotColor = if (isFocused) Color(0x88FF5722) else Color(0x44000000)
            )
            .clip(RoundedCornerShape(22.dp))
            .background(Color(0xFF191B24))
            .border(
                width = if (isFocused) 2.dp else 1.dp,
                color = if (isFocused) Color(0xFFFF6D3A) else Color(0xFF383E4F),
                shape = RoundedCornerShape(22.dp)
            ),
        contentAlignment = Alignment.Center
    ) {
        if (item.bitmap != null) {
            Image(
                bitmap = item.bitmap.asImageBitmap(),
                contentDescription = item.fileName,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize()
            )
        } else if (item.isImage) {
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Rounded.PhotoLibrary,
                    contentDescription = null,
                    tint = Color(0x55FFFFFF),
                    modifier = Modifier.size(44.dp)
                )
            }
        } else {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(20.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Icon(
                    imageVector = Icons.Rounded.Bolt,
                    contentDescription = null,
                    tint = Color(0xFFFF5722),
                    modifier = Modifier.size(44.dp)
                )
                Spacer(modifier = Modifier.height(10.dp))
                Text(
                    text = item.fileName,
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                    color = Color.White,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center
                )
            }
        }

        // Ambient depth darkening for non-focused cards behind
        if (!isFocused) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = (kotlin.math.abs(diff) * 0.12f).coerceIn(0.15f, 0.40f)))
            )
        }

        // Top-Left Pill Badge ("01", "02", "03" - matching user reference)
        Box(
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(10.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(if (isFocused) Color(0xDDFF5722) else Color(0xCC161922))
                .border(
                    width = 1.dp,
                    color = if (isFocused) Color(0xAAFFFFFF) else Color(0x33FFFFFF),
                    shape = RoundedCornerShape(10.dp)
                )
                .padding(horizontal = 8.dp, vertical = 3.dp)
        ) {
            Text(
                text = String.format(java.util.Locale.US, "%02d", index + 1),
                style = MaterialTheme.typography.labelSmall,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Bold,
                fontSize = 10.sp,
                color = Color.White
            )
        }
    }
}

private fun loadThumbnail(context: Context, uri: Uri, maxDimension: Int): Bitmap? {
    return try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val source = ImageDecoder.createSource(context.contentResolver, uri)
            ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
                val sample = maxOf(info.size.width / maxDimension, info.size.height / maxDimension, 1)
                decoder.setTargetSampleSize(sample)
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            }
        } else {
            var sample = 1
            context.contentResolver.openInputStream(uri)?.use { stream ->
                val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeStream(stream, null, opts)
                sample = maxOf(opts.outWidth / maxDimension, opts.outHeight / maxDimension, 1)
            }
            context.contentResolver.openInputStream(uri)?.use { stream ->
                val opts = BitmapFactory.Options().apply { inSampleSize = sample }
                BitmapFactory.decodeStream(stream, null, opts)
            }
        }
    } catch (_: Exception) {
        null
    }
}

private fun formatBytes(bytes: Long): String {
    if (bytes <= 0) return ""
    val kb = bytes / 1024.0
    val mb = kb / 1024.0
    return when {
        mb >= 1.0 -> String.format(java.util.Locale.US, "%.1f MB", mb)
        kb >= 1.0 -> String.format(java.util.Locale.US, "%.0f KB", kb)
        else -> "$bytes B"
    }
}
