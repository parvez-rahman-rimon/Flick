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

    val pagerState = rememberPagerState(initialPage = 0, pageCount = { deckItems.size.coerceAtLeast(1) })

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
            val activeItem = deckItems.getOrNull(pagerState.currentPage)
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

            // 2. Center 3D Cover Flow Deck with Interactive Swipe-Up
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
                            .fillMaxWidth(0.82f)
                            .aspectRatio(0.88f)
                            .clip(RoundedCornerShape(24.dp))
                            .background(Color(0xFF191B24))
                    )
                } else if (deckItems.size == 1) {
                    val item = deckItems[0]
                    CoverFlowCard(
                        item = item,
                        absOffset = 0f,
                        modifier = Modifier
                            .fillMaxWidth(0.82f)
                            .aspectRatio(0.88f)
                    )
                } else {
                    // Dynamic 3D Cover Flow Carousel
                    HorizontalPager(
                        state = pagerState,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(340.dp),
                        contentPadding = PaddingValues(horizontal = 72.dp),
                        pageSpacing = (-22).dp
                    ) { page ->
                        val item = deckItems.getOrNull(page)
                        val pageOffset = ((pagerState.currentPage - page) + pagerState.currentPageOffsetFraction)
                        val absOffset = kotlin.math.abs(pageOffset).coerceAtMost(2.5f)

                        // 3D Perspective Rotation:
                        // Left cards: positive rotationY (angles inward toward center)
                        // Right cards: negative rotationY (angles inward toward center)
                        val rotationY = (pageOffset * -46f).coerceIn(-54f, 54f)
                        val scale = (1f - (absOffset * 0.16f)).coerceIn(0.70f, 1.0f)
                        val alpha = (1f - (absOffset * 0.25f)).coerceIn(0.50f, 1.0f)
                        val zIndex = 100f - absOffset * 10f

                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .zIndex(zIndex)
                                .graphicsLayer {
                                    this.rotationY = rotationY
                                    this.scaleX = scale
                                    this.scaleY = scale
                                    this.alpha = alpha
                                    this.cameraDistance = 16f * density.density
                                },
                            contentAlignment = Alignment.Center
                        ) {
                            if (item != null) {
                                CoverFlowCard(
                                    item = item,
                                    absOffset = absOffset,
                                    modifier = Modifier
                                        .fillMaxWidth(0.92f)
                                        .aspectRatio(0.86f)
                                )
                            }
                        }
                    }
                }

                Spacer(modifier = Modifier.height(14.dp))

                // Metadata Telemetry (Active Card Info)
                val activeItem = deckItems.getOrNull(pagerState.currentPage)
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
                            text = if (deckItems.size > 1) "PHOTO ${pagerState.currentPage + 1} OF ${deckItems.size}" else "READY TO TELEPORT",
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

                    // Pagination indicator dots
                    if (deckItems.size > 1) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Row(
                            horizontalArrangement = Arrangement.Center,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            val maxDots = deckItems.size.coerceAtMost(10)
                            for (i in 0 until maxDots) {
                                val isSelected = i == pagerState.currentPage.coerceAtMost(maxDots - 1)
                                Box(
                                    modifier = Modifier
                                        .padding(horizontal = 3.dp)
                                        .size(if (isSelected) 6.dp else 4.dp)
                                        .clip(CircleShape)
                                        .background(if (isSelected) Color(0xFFFF5722) else Color(0x558892B0))
                                )
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
private fun CoverFlowCard(
    item: FlickCardItem,
    absOffset: Float,
    modifier: Modifier = Modifier
) {
    Box(
        modifier = modifier
            .shadow(
                elevation = if (absOffset < 0.5f) 28.dp else 10.dp,
                shape = RoundedCornerShape(24.dp),
                spotColor = Color(0x66FF5722)
            )
            .clip(RoundedCornerShape(24.dp))
            .background(Color(0xFF191B24))
            .border(
                1.5.dp,
                if (absOffset < 0.5f) Color(0xFF4C5369) else Color(0xFF2E3342),
                RoundedCornerShape(24.dp)
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
                    modifier = Modifier.size(48.dp)
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
                    modifier = Modifier.size(48.dp)
                )
                Spacer(modifier = Modifier.height(12.dp))
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

        // Perspective lighting depth: angled side cards receive realistic ambient shadow tint
        if (absOffset > 0.08f) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = (absOffset * 0.32f).coerceIn(0f, 0.44f)))
            )
        }

        // Soft ground reflection / ambient rim at bottom
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(30.dp)
                .background(
                    Brush.verticalGradient(
                        colors = listOf(Color.Transparent, Color(0x44000000))
                    )
                )
        )
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
