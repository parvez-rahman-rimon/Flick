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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
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
import androidx.compose.material.icons.rounded.PlayArrow
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
    val isVideo: Boolean = false,
    val isPdf: Boolean = false,
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
                    var name = "File ${idx + 1}"
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

                    val isImg = mimeType.startsWith("image/")
                    val isVid = mimeType.startsWith("video/")
                    val isPdf = mimeType == "application/pdf" || name.endsWith(".pdf", ignoreCase = true)

                    val bmp = loadMediaThumbnail(context, uri, mimeType, name, maxDimension = 800)
                    FlickCardItem(
                        uri = uri,
                        fileName = name,
                        fileSizeStr = sizeStr,
                        isImage = isImg,
                        isVideo = isVid,
                        isPdf = isPdf,
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
                // Strict intentional flick / shake detection:
                // Earth gravity baseline is 1.0g. Unintentional hand jitter or walking stays below 1.6g.
                // An intentional, confident wrist flick or decisive shake reaches 2.65g+.
                // Requires either high gForce spike (>= 2.65g) or purposeful sharp directional jerk (y >= 22 m/s^2 or z >= 22 m/s^2).
                if ((gForce >= 2.65f || y >= 22.0f || z >= 22.0f) && (now - lastFlickTime > 1200)) {
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
            .statusBarsPadding()
            .navigationBarsPadding()
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
            .padding(horizontal = 24.dp, vertical = 12.dp),
        contentAlignment = Alignment.Center
    ) {
        Column(
            modifier = Modifier.fillMaxSize(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            // 1. Top HUD Header (Symmetrical Industrial Keycaps)
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 10.dp, bottom = 6.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Left Keycap: Connected PC & Online Status LED
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .height(34.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(
                            Brush.verticalGradient(
                                listOf(Color(0xFF1B1E29), Color(0xFF12141C))
                            )
                        )
                        .border(1.dp, Color(0xFF2B3144), RoundedCornerShape(10.dp))
                        .padding(horizontal = 14.dp)
                ) {
                    Box(
                        modifier = Modifier
                            .size(7.dp)
                            .clip(CircleShape)
                            .background(if (isOnline) Color(0xFFFF4400) else Color(0xFFFF5252))
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Text(
                        text = pcName.uppercase(),
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        fontSize = 11.sp,
                        color = Color.White
                    )
                }

                // Right Keycap: Symmetrical Tactile Close Button
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.Center,
                    modifier = Modifier
                        .height(34.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(
                            Brush.verticalGradient(
                                listOf(Color(0xFF1B1E29), Color(0xFF12141C))
                            )
                        )
                        .border(1.dp, Color(0xFF2B3144), RoundedCornerShape(10.dp))
                        .clickable { onDismiss() }
                        .padding(horizontal = 13.dp)
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Close,
                        contentDescription = "Close",
                        tint = Color.White,
                        modifier = Modifier.size(14.dp)
                    )
                    Spacer(modifier = Modifier.width(6.dp))
                    Text(
                        text = "CLOSE",
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        fontSize = 11.sp,
                        color = Color(0xFFA0A6B8)
                    )
                }
            }

            // 2. Center 3D Card Stack with Dynamic Spatial Tilt & Flight Physics
            val currentY = dragY.value
            val rotationAngle = (currentY / 60f).coerceIn(-15f, 15f)
            val cardScale = (1f + (currentY / 1800f)).coerceIn(0.80f, 1.0f)
            val cardAlpha = (1f + (currentY / 1200f)).coerceIn(0.15f, 1.0f)

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                // Cards Flight Capsule: Physical card lifts off, tilts in 3D perspective and shoots off into PC
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .graphicsLayer {
                            translationY = currentY
                            rotationZ = rotationAngle
                            rotationX = (-currentY / 50f).coerceIn(0f, 32f)
                            scaleX = cardScale
                            scaleY = cardScale
                            alpha = cardAlpha
                            cameraDistance = 16f * density.density
                        },
                    contentAlignment = Alignment.Center
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
                        totalCount = 1,
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
                                    totalCount = deckItems.size,
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

            // 3. Bottom Controls & Tactile 3D Keycaps
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
                        text = "Shake phone or swipe up to flick.",
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        color = Color(0xFFA6ABB6)
                    )
                }

                // 3D Master Hardware Action Keycaps
                if (uris.size <= 1) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth(0.88f)
                            .height(52.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .background(Color(0xFF882200))
                            .padding(bottom = 3.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .background(Brush.verticalGradient(listOf(Color(0xFFFF5500), Color(0xFFE63800))))
                            .border(1.dp, Color.White.copy(alpha = 0.25f), RoundedCornerShape(16.dp))
                            .clickable { executeFlick(isSingle = true) },
                        contentAlignment = Alignment.Center
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.Center
                        ) {
                            Icon(
                                imageVector = Icons.Rounded.Bolt,
                                contentDescription = null,
                                tint = Color.White,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(
                                text = "FLICK TO PC",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.Black,
                                letterSpacing = 0.5.sp,
                                color = Color.White
                            )
                        }
                    }
                } else {
                    Row(
                        modifier = Modifier.fillMaxWidth(0.92f),
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        // Left Keycap: Flick Single (Slate 3D Keycap)
                        Box(
                            modifier = Modifier
                                .weight(1f)
                                .height(50.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(Color(0xFF101216))
                                .padding(bottom = 3.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(Color(0xFF22252C))
                                .border(1.dp, Color(0xFF353942), RoundedCornerShape(14.dp))
                                .clickable { executeFlick(isSingle = true) },
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = "FLICK SINGLE",
                                style = MaterialTheme.typography.bodyMedium,
                                fontWeight = FontWeight.Bold,
                                color = Color.White
                            )
                        }

                        // Right Keycap: Whole Stack (Safety Orange 3D Keycap)
                        Box(
                            modifier = Modifier
                                .weight(1.2f)
                                .height(50.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(Color(0xFF882200))
                                .padding(bottom = 3.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(Brush.verticalGradient(listOf(Color(0xFFFF5500), Color(0xFFE63800))))
                                .border(1.dp, Color.White.copy(alpha = 0.25f), RoundedCornerShape(14.dp))
                                .clickable { executeFlick(isSingle = false) },
                            contentAlignment = Alignment.Center
                        ) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.Center
                            ) {
                                Icon(
                                    imageVector = Icons.Rounded.Bolt,
                                    contentDescription = null,
                                    tint = Color.White,
                                    modifier = Modifier.size(18.dp)
                                )
                                Spacer(modifier = Modifier.width(6.dp))
                                Text(
                                    text = "WHOLE STACK",
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.Black,
                                    color = Color.White
                                )
                            }
                        }
                    }
                }

                // Subtle Cancel Button with TE Industrial Tone
                Text(
                    text = "[ EJECT // STOW DECK ]",
                    style = MaterialTheme.typography.labelSmall,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Bold,
                    color = Color(0xFF6B7280),
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
    totalCount: Int = 1,
    isFocused: Boolean,
    diff: Int,
    modifier: Modifier = Modifier
) {
    Box(
        modifier = modifier
            .shadow(
                elevation = if (isFocused) 32.dp else 10.dp,
                shape = RoundedCornerShape(24.dp),
                spotColor = if (isFocused) Color(0x88FF4400) else Color(0x44000000)
            )
            .clip(RoundedCornerShape(24.dp))
            .background(Color(0xFF131519))
            .border(
                width = if (isFocused) 2.dp else 1.dp,
                color = if (isFocused) Color(0xFFFF4400) else Color(0xFF2C303A),
                shape = RoundedCornerShape(24.dp)
            ),
        contentAlignment = Alignment.Center
    ) {
        if (item.bitmap != null) {
            Image(
                bitmap = item.bitmap.asImageBitmap(),
                contentDescription = item.fileName,
                contentScale = if (item.isPdf) ContentScale.Fit else ContentScale.Crop,
                modifier = Modifier.fillMaxSize()
            )
            // Overlay Play icon for video previews
            if (item.isVideo) {
                Box(
                    modifier = Modifier
                        .size(48.dp)
                        .clip(CircleShape)
                        .background(Color.Black.copy(alpha = 0.55f))
                        .border(1.dp, Color.White.copy(alpha = 0.40f), CircleShape),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(
                        imageVector = Icons.Rounded.PlayArrow,
                        contentDescription = "Video",
                        tint = Color.White,
                        modifier = Modifier.size(30.dp)
                    )
                }
            }
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
                    imageVector = if (item.isPdf) Icons.Rounded.Description else Icons.Rounded.Bolt,
                    contentDescription = null,
                    tint = if (item.isPdf) Color(0xFFE53935) else Color(0xFFFF5722),
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

        // Top-Left Pill Badge ("01", "02", "03" - ONLY when multiple files exist)
        if (totalCount > 1) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(10.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (isFocused) Color(0xFFFF4400) else Color(0xCC161922))
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

        // PDF Badge on Top-Right for clarity
        if (item.isPdf) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(10.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(0xFFE53935))
                    .border(1.dp, Color.White.copy(alpha = 0.35f), RoundedCornerShape(8.dp))
                    .padding(horizontal = 7.dp, vertical = 2.dp)
            ) {
                Text(
                    text = "PDF",
                    style = MaterialTheme.typography.labelSmall,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Bold,
                    fontSize = 9.sp,
                    color = Color.White
                )
            }
        }
    }
}

private fun loadMediaThumbnail(
    context: Context,
    uri: Uri,
    mimeType: String,
    fileName: String,
    maxDimension: Int
): Bitmap? {
    return try {
        when {
            mimeType.startsWith("image/") -> {
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
            }
            mimeType.startsWith("video/") -> {
                val sysThumb = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    try {
                        context.contentResolver.loadThumbnail(uri, android.util.Size(maxDimension, maxDimension), null)
                    } catch (_: Exception) {
                        null
                    }
                } else {
                    null
                }
                sysThumb ?: run {
                    val retriever = android.media.MediaMetadataRetriever()
                    try {
                        retriever.setDataSource(context, uri)
                        retriever.getFrameAtTime(0, android.media.MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                    } catch (_: Exception) {
                        null
                    } finally {
                        try { retriever.release() } catch (_: Exception) {}
                    }
                }
            }
            mimeType == "application/pdf" || fileName.endsWith(".pdf", ignoreCase = true) -> {
                context.contentResolver.openFileDescriptor(uri, "r")?.use { pfd ->
                    try {
                        val renderer = android.graphics.pdf.PdfRenderer(pfd)
                        if (renderer.pageCount > 0) {
                            val page = renderer.openPage(0)
                            val scale = maxDimension.toFloat() / maxOf(page.width, page.height).coerceAtLeast(1)
                            val targetW = (page.width * scale).toInt().coerceAtLeast(1)
                            val targetH = (page.height * scale).toInt().coerceAtLeast(1)
                            val bmp = Bitmap.createBitmap(targetW, targetH, Bitmap.Config.ARGB_8888)
                            bmp.eraseColor(android.graphics.Color.WHITE)
                            page.render(bmp, null, null, android.graphics.pdf.PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                            page.close()
                            renderer.close()
                            bmp
                        } else {
                            renderer.close()
                            null
                        }
                    } catch (_: Exception) {
                        null
                    }
                }
            }
            else -> null
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
