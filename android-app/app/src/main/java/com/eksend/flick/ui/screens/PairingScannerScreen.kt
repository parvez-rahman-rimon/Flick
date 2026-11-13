package com.eksend.flick.ui.screens

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.CameraAlt
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.network.FlickHttpClient
import com.eksend.flick.service.HapticHelper
import android.util.Log
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.LuminanceSource
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.GlobalHistogramBinarizer
import com.google.zxing.common.HybridBinarizer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.util.concurrent.Executors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairingScannerScreen(
    preferences: FlickPreferences,
    onPairSuccess: () -> Unit,
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var hasCameraPermission by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
        )
    }

    var isPairing by remember { mutableStateOf(false) }
    var scanCompleted by remember { mutableStateOf(false) }

    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { granted ->
        hasCameraPermission = granted
    }

    BackHandler {
        onBack()
    }

    LaunchedEffect(Unit) {
        if (!hasCameraPermission) {
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                text = "OPTICAL MATRIX",
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.Black,
                                letterSpacing = 1.sp,
                                color = Color.White
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(
                                text = "CAM-01",
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        }
                        Text(
                            text = "HIGH-SPEED QR SYNCHRONIZER",
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            fontSize = 8.sp,
                            color = Color.LightGray
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Rounded.ArrowBack,
                            contentDescription = "Back",
                            tint = Color.White
                        )
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = Color.Black.copy(alpha = 0.7f),
                    titleContentColor = Color.White,
                    navigationIconContentColor = Color.White
                )
            )
        },
        containerColor = Color.Black
    ) { paddingValues ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
        ) {
            if (hasCameraPermission) {
                // Camera Preview and Analyzer
                AndroidView(
                    factory = { ctx ->
                        val previewView = PreviewView(ctx)
                        val cameraProviderFuture = ProcessCameraProvider.getInstance(ctx)
                        val cameraExecutor = Executors.newSingleThreadExecutor()

                        cameraProviderFuture.addListener({
                            val cameraProvider = cameraProviderFuture.get()
                            val preview = Preview.Builder().build().also {
                                it.surfaceProvider = previewView.surfaceProvider
                            }

                            val hints = mapOf(
                                DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE),
                                DecodeHintType.TRY_HARDER to java.lang.Boolean.TRUE,
                                DecodeHintType.CHARACTER_SET to "UTF-8"
                            )
                            val reader = MultiFormatReader().apply { setHints(hints) }
                            val imageAnalysis = ImageAnalysis.Builder()
                                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                                .build()

                            imageAnalysis.setAnalyzer(cameraExecutor) { imageProxy ->
                                try {
                                    if (scanCompleted) {
                                        return@setAnalyzer
                                    }

                                    val qrContent = decodeQrFromImage(imageProxy, reader)
                                    if (!qrContent.isNullOrEmpty() && !scanCompleted) {
                                        scanCompleted = true
                                        scope.launch(Dispatchers.Main) {
                                            isPairing = true
                                        }

                                        scope.launch(Dispatchers.IO) {
                                            try {
                                                val pairRes = if (qrContent.startsWith("flick://pair") || qrContent.contains("v=2")) {
                                                    // Zero-Trust mTLS 1.3 Out-of-Band Pairing
                                                    val uri = android.net.Uri.parse(qrContent)
                                                    val ip = uri.getQueryParameter("ip") ?: ""
                                                    val port = uri.getQueryParameter("p")?.toIntOrNull() ?: 45455
                                                    val pin = uri.getQueryParameter("pin") ?: ""
                                                    val secret = uri.getQueryParameter("s") ?: ""
                                                    val pcId = uri.getQueryParameter("id") ?: ""

                                                    FlickHttpClient.pairV2(
                                                        ip = ip,
                                                        port = port,
                                                        pcSpkiPin = pin,
                                                        pairSecret = secret,
                                                        pcId = pcId,
                                                        clientId = preferences.clientId,
                                                        clientName = preferences.clientName
                                                    )
                                                } else {
                                                    // Legacy JSON fallback
                                                    val json = JSONObject(qrContent)
                                                    val ip = json.getString("ip")
                                                    val port = json.optInt("port", 45455)
                                                    val token = json.optString("token", "")

                                                    FlickHttpClient.pairWithToken(
                                                        ip = ip,
                                                        port = port,
                                                        clientId = preferences.clientId,
                                                        clientName = preferences.clientName,
                                                        token = token
                                                    )
                                                }

                                                withContext(Dispatchers.Main) {
                                                    isPairing = false
                                                    pairRes.onSuccess { pairedPc ->
                                                        preferences.savePairedPc(pairedPc)
                                                        HapticHelper.triggerPairSuccess(context, pairedPc.pcName)
                                                        onPairSuccess()
                                                    }.onFailure { err ->
                                                        scanCompleted = false
                                                        HapticHelper.triggerError(context, err.message ?: "Pairing failed")
                                                    }
                                                }
                                            } catch (e: Exception) {
                                                withContext(Dispatchers.Main) {
                                                    isPairing = false
                                                    scanCompleted = false
                                                    HapticHelper.triggerError(context, e.message ?: "Pairing error")
                                                }
                                            }
                                        }
                                    }
                                } catch (t: Throwable) {
                                    Log.e("PairingScanner", "Error analyzing frame", t)
                                } finally {
                                    try {
                                        imageProxy.close()
                                    } catch (_: Exception) {}
                                }
                            }

                            try {
                                var lifecycleOwner: androidx.lifecycle.LifecycleOwner? = ctx as? androidx.lifecycle.LifecycleOwner
                                if (lifecycleOwner == null && ctx is android.content.ContextWrapper) {
                                    var cur: android.content.Context? = ctx
                                    while (cur is android.content.ContextWrapper) {
                                        if (cur is androidx.lifecycle.LifecycleOwner) {
                                            lifecycleOwner = cur
                                            break
                                        }
                                        cur = cur.baseContext
                                    }
                                }

                                if (lifecycleOwner != null) {
                                    cameraProvider.unbindAll()
                                    cameraProvider.bindToLifecycle(
                                        lifecycleOwner,
                                        CameraSelector.DEFAULT_BACK_CAMERA,
                                        preview,
                                        imageAnalysis
                                    )
                                }
                            } catch (e: Exception) {
                                Log.e("PairingScanner", "Error binding camera", e)
                            }
                        }, ContextCompat.getMainExecutor(ctx))

                        previewView
                    },
                    modifier = Modifier.fillMaxSize()
                )

                // Teenage Engineering Technical Viewfinder Reticle
                val strokeColor = MaterialTheme.colorScheme.primary
                Canvas(modifier = Modifier.fillMaxSize()) {
                    val w = size.width
                    val h = size.height
                    val boxSize = 250.dp.toPx()
                    val left = (w - boxSize) / 2f
                    val top = (h - boxSize) / 2f - 30.dp.toPx()

                    // 1. Darkened Scrim
                    drawRect(color = Color.Black.copy(alpha = 0.65f))

                    // 2. Clear Center Aperture
                    drawRoundRect(
                        color = Color.Transparent,
                        topLeft = Offset(left, top),
                        size = Size(boxSize, boxSize),
                        cornerRadius = CornerRadius(20.dp.toPx(), 20.dp.toPx()),
                        blendMode = BlendMode.Clear
                    )

                    // 3. Technical Corner Brackets (Safety Orange)
                    val bracketLen = 32.dp.toPx()
                    val strokeW = 4.dp.toPx()

                    // Top-Left
                    drawLine(strokeColor, Offset(left, top), Offset(left + bracketLen, top), strokeW, StrokeCap.Square)
                    drawLine(strokeColor, Offset(left, top), Offset(left, top + bracketLen), strokeW, StrokeCap.Square)

                    // Top-Right
                    drawLine(strokeColor, Offset(left + boxSize, top), Offset(left + boxSize - bracketLen, top), strokeW, StrokeCap.Square)
                    drawLine(strokeColor, Offset(left + boxSize, top), Offset(left + boxSize, top + bracketLen), strokeW, StrokeCap.Square)

                    // Bottom-Left
                    drawLine(strokeColor, Offset(left, top + boxSize), Offset(left + bracketLen, top + boxSize), strokeW, StrokeCap.Square)
                    drawLine(strokeColor, Offset(left, top + boxSize), Offset(left, top + boxSize - bracketLen), strokeW, StrokeCap.Square)

                    // Bottom-Right
                    drawLine(strokeColor, Offset(left + boxSize, top + boxSize), Offset(left + boxSize - bracketLen, top + boxSize), strokeW, StrokeCap.Square)
                    drawLine(strokeColor, Offset(left + boxSize, top + boxSize), Offset(left + boxSize, top + boxSize - bracketLen), strokeW, StrokeCap.Square)

                    // Center Crosshair Tick
                    val cx = left + boxSize / 2f
                    val cy = top + boxSize / 2f
                    val crossLen = 8.dp.toPx()
                    drawLine(strokeColor.copy(alpha = 0.6f), Offset(cx - crossLen, cy), Offset(cx + crossLen, cy), 1.5.dp.toPx())
                    drawLine(strokeColor.copy(alpha = 0.6f), Offset(cx, cy - crossLen), Offset(cx, cy + crossLen), 1.5.dp.toPx())
                }

                // Machined Bottom Console Plate
                Surface(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .padding(20.dp)
                        .fillMaxWidth()
                        .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(20.dp)),
                    shape = RoundedCornerShape(20.dp),
                    color = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
                    shadowElevation = 8.dp
                ) {
                    Column(
                        modifier = Modifier.padding(20.dp),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        if (isPairing) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(28.dp),
                                strokeWidth = 3.dp,
                                color = MaterialTheme.colorScheme.primary
                            )
                            Spacer(modifier = Modifier.height(10.dp))
                            Text(
                                text = "ESTABLISHING ECDSA P-256 LINK...",
                                style = MaterialTheme.typography.titleSmall,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        } else {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(6.dp)
                                        .clip(CircleShape)
                                        .background(MaterialTheme.colorScheme.primary)
                                )
                                Text(
                                    text = "ALIGN PC MATRIX IN RETICLE",
                                    style = MaterialTheme.typography.labelSmall,
                                    fontFamily = FontFamily.Monospace,
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.primary
                                )
                            }
                            Spacer(modifier = Modifier.height(6.dp))
                            Text(
                                text = "Right-click Flick in System Tray or press Ctrl+Alt+F on your PC to show QR.",
                                style = MaterialTheme.typography.bodySmall,
                                fontFamily = FontFamily.Monospace,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                textAlign = TextAlign.Center
                            )
                        }
                    }
                }

            } else {
                // Permission Request Screen
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(32.dp),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Icon(
                        imageVector = Icons.Rounded.CameraAlt,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(64.dp)
                    )
                    Spacer(modifier = Modifier.height(20.dp))
                    Text(
                        text = "OPTICAL SENSOR ARMED",
                        style = MaterialTheme.typography.titleLarge,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Black,
                        color = Color.White
                    )
                    Spacer(modifier = Modifier.height(10.dp))
                    Text(
                        text = "Camera sensor access is required to optically decode the out-of-band TLS pairing matrix.",
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = FontFamily.Monospace,
                        color = Color.LightGray,
                        textAlign = TextAlign.Center
                    )
                    Spacer(modifier = Modifier.height(24.dp))
                    Button(
                        onClick = { permissionLauncher.launch(Manifest.permission.CAMERA) },
                        shape = RoundedCornerShape(12.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = MaterialTheme.colorScheme.primary,
                            contentColor = Color.White
                        )
                    ) {
                        Text(
                            "GRANT OPTICAL PERMISSION",
                            fontFamily = FontFamily.Monospace,
                            fontWeight = FontWeight.Bold
                        )
                    }
                }
            }
        }
    }
}

private fun decodeQrFromImage(imageProxy: ImageProxy, reader: MultiFormatReader): String? {
    return try {
        val plane = imageProxy.planes[0]
        val buffer = plane.buffer
        val bytes = ByteArray(buffer.remaining())
        buffer.get(bytes)

        val width = imageProxy.width
        val height = imageProxy.height

        // 1. Try decoding with raw unrotated YUV first
        // Note: ZXing's QRCodeReader is omnidirectional and detects orientation patterns in all 4 directions!
        val rawSource = PlanarYUVLuminanceSource(
            bytes, width, height, 0, 0, width, height, false
        )

        try {
            val binaryBitmap = BinaryBitmap(HybridBinarizer(rawSource))
            val result = reader.decodeWithState(binaryBitmap)
            return result.text
        } catch (_: Exception) {
        } finally {
            reader.reset()
        }

        try {
            val binaryBitmap = BinaryBitmap(GlobalHistogramBinarizer(rawSource))
            val result = reader.decodeWithState(binaryBitmap)
            return result.text
        } catch (_: Exception) {
        } finally {
            reader.reset()
        }

        // 2. If raw fails and camera sensor rotation is 90 or 270 degrees,
        // safely rotate the YUV byte array in software (PlanarYUVLuminanceSource does NOT support rotateCounterClockwise)
        val rotation = imageProxy.imageInfo.rotationDegrees
        if (rotation == 90 || rotation == 270) {
            val rotatedData = ByteArray(width * height)
            if (rotation == 90) {
                var i = 0
                for (x in 0 until width) {
                    for (y in height - 1 downTo 0) {
                        rotatedData[i++] = bytes[y * width + x]
                    }
                }
            } else {
                var i = 0
                for (x in width - 1 downTo 0) {
                    for (y in 0 until height) {
                        rotatedData[i++] = bytes[y * width + x]
                    }
                }
            }

            val rotatedSource = PlanarYUVLuminanceSource(
                rotatedData, height, width, 0, 0, height, width, false
            )

            try {
                val binaryBitmap = BinaryBitmap(HybridBinarizer(rotatedSource))
                val result = reader.decodeWithState(binaryBitmap)
                return result.text
            } catch (_: Exception) {
            } finally {
                reader.reset()
            }

            try {
                val binaryBitmap = BinaryBitmap(GlobalHistogramBinarizer(rotatedSource))
                val result = reader.decodeWithState(binaryBitmap)
                return result.text
            } catch (_: Exception) {
            } finally {
                reader.reset()
            }
        }

        null
    } catch (t: Throwable) {
        Log.e("PairingScanner", "Safe decode exception caught", t)
        null
    }
}
