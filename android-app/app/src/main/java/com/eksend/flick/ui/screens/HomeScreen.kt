package com.eksend.flick.ui.screens

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AudioFile
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.ContentPaste
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.QrCodeScanner
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.Videocam
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.eksend.flick.data.ClipboardMode
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.PcOnlineState
import com.eksend.flick.data.TransferRecord
import com.eksend.flick.network.FlickHttpClient
import com.eksend.flick.network.UdpDiscoveryListener
import com.eksend.flick.service.HapticHelper
import com.eksend.flick.service.UploadProgressBus
import com.eksend.flick.service.UploadService
import com.eksend.flick.ui.components.BentoActionTile
import com.eksend.flick.ui.components.DiscoveredPairCard
import com.eksend.flick.ui.components.ScallopedDeviceBadge
import com.eksend.flick.ui.components.SpatialFlickDeck
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    preferences: FlickPreferences,
    udpListener: UdpDiscoveryListener,
    onNavigateToSettings: () -> Unit,
    onNavigateToScanner: () -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var activePc by remember { mutableStateOf(preferences.getActivePc()) }
    var onlineState by remember { mutableStateOf(PcOnlineState()) }
    var isPinging by remember { mutableStateOf(false) }
    var recentTransfers by remember { mutableStateOf(preferences.getRecentTransfers()) }
    var dismissedPcIds by remember { mutableStateOf(setOf<String>()) }
    var showSendTextDialog by remember { mutableStateOf(false) }
    var textInput by remember { mutableStateOf("") }
    var pendingFlickUris by remember { mutableStateOf<List<Uri>>(emptyList()) }

    val discoveredPcsMap by udpListener.discoveredPcs.collectAsState()
    val uploadState by UploadProgressBus.state.collectAsState()

    // File picker launcher
    val filePickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.GetMultipleContents()
    ) { uris: List<Uri> ->
        if (uris.isNotEmpty()) {
            pendingFlickUris = uris
        }
    }

    // Ping check function
    fun checkPcHealth() {
        val pc = activePc ?: return
        isPinging = true
        scope.launch(Dispatchers.IO) {
            val result = FlickHttpClient.ping(
                ip = pc.lastKnownIp,
                port = pc.port,
                authToken = pc.authToken,
                pcSpkiPin = pc.pcSpkiPin,
                connectTimeoutMs = 600
            )
            withContext(Dispatchers.Main) {
                onlineState = result.getOrElse { PcOnlineState(isOnline = false) }
                isPinging = false
            }
        }
    }

    // Periodically check active PC status
    LaunchedEffect(activePc) {
        if (activePc != null) {
            checkPcHealth()
        }
    }

    // Discovered PCs that are not yet paired
    val pairedIds = remember(preferences.getPairedPcs()) { preferences.getPairedPcs().map { it.pcId }.toSet() }
    val incomingPairRequests = discoveredPcsMap.values.filter {
        !pairedIds.contains(it.pcId) && !dismissedPcIds.contains(it.pcId)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    // Clean FLICK TX-1 Lockup (Without any dot)
                    Column {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                text = "FLICK",
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.Black,
                                letterSpacing = 2.sp,
                                color = MaterialTheme.colorScheme.onSurface
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(
                                text = "TX-1",
                                style = MaterialTheme.typography.labelSmall,
                                fontFamily = FontFamily.Monospace,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        }
                        Text(
                            text = "DIRECT PEER FIELD TRANSCEIVER",
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            fontSize = 8.sp,
                            letterSpacing = 0.8.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                },
                actions = {
                    IconButton(onClick = onNavigateToScanner) {
                        Icon(
                            imageVector = Icons.Rounded.QrCodeScanner,
                            contentDescription = "Scan PC QR",
                            tint = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    IconButton(onClick = onNavigateToSettings) {
                        Icon(
                            imageVector = Icons.Rounded.Tune,
                            contentDescription = "Device Settings",
                            tint = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background
                )
            )
        },
        containerColor = MaterialTheme.colorScheme.background
    ) { paddingValues ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp)
        ) {
            // 1. Incoming LAN Discovery Pair Request Cards
            items(incomingPairRequests) { pc ->
                DiscoveredPairCard(
                    pc = pc,
                    onPair = {
                        scope.launch(Dispatchers.IO) {
                            val res = FlickHttpClient.requestLanPair(
                                ip = pc.ip,
                                port = pc.port,
                                clientId = preferences.clientId,
                                clientName = preferences.clientName,
                                pairCode = pc.pcId.take(8).uppercase()
                            )
                            withContext(Dispatchers.Main) {
                                res.onSuccess { newPairedPc ->
                                    preferences.savePairedPc(newPairedPc)
                                    activePc = newPairedPc
                                    checkPcHealth()
                                    HapticHelper.triggerPairSuccess(context, newPairedPc.pcName)
                                }.onFailure { err ->
                                    HapticHelper.triggerError(context, err.message ?: "Pairing failed")
                                }
                            }
                        }
                    },
                    onDismiss = {
                        dismissedPcIds = dismissedPcIds + pc.pcId
                    }
                )
            }

            // 2. Hardware CRT / Active PC Telemetry Screen (Layered Contrast)
            val pc = activePc
            if (pc != null) {
                item {
                    Surface(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(24.dp))
                            .border(
                                width = 1.dp,
                                color = MaterialTheme.colorScheme.outline,
                                shape = RoundedCornerShape(24.dp)
                            ),
                        shape = RoundedCornerShape(24.dp),
                        color = MaterialTheme.colorScheme.surface,
                        shadowElevation = 3.dp
                    ) {
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(18.dp)
                        ) {
                            // Top Row: Status telemetry & TP-7 Motorized Spool
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    val statusTag = when {
                                        uploadState.isUploading -> "TX: ${uploadState.progressPercent}% // [${uploadState.batchIndex + 1}/${uploadState.batchTotal}]"
                                        isPinging -> "LINK: SYNCING..."
                                        onlineState.isOnline -> "LINK: CONNECTED // TLS 1.3"
                                        else -> "LINK: OFFLINE // TAP REEL"
                                    }
                                    Text(
                                        text = statusTag,
                                        style = MaterialTheme.typography.labelSmall,
                                        fontFamily = FontFamily.Monospace,
                                        fontWeight = FontWeight.Bold,
                                        color = if (uploadState.isUploading || onlineState.isOnline) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                                    )

                                    Spacer(modifier = Modifier.height(3.dp))

                                    // Machine Hostname (Clean Swiss Bold)
                                    Text(
                                        text = pc.pcName.uppercase(),
                                        style = MaterialTheme.typography.titleLarge,
                                        fontWeight = FontWeight.Black,
                                        letterSpacing = 0.5.sp,
                                        color = MaterialTheme.colorScheme.onSurface,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis
                                    )

                                    Text(
                                        text = "IP: ${pc.lastKnownIp} : ${pc.port}",
                                        style = MaterialTheme.typography.bodySmall,
                                        fontFamily = FontFamily.Monospace,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }

                                // TP-7 Motorized Tape Spool (Tap to Ping, spins during sync/stream/upload)
                                ScallopedDeviceBadge(
                                    isOnline = onlineState.isOnline,
                                    isSpinning = isPinging || uploadState.isUploading,
                                    size = 56.dp,
                                    onClick = { checkPcHealth() }
                                )
                            }

                            Spacer(modifier = Modifier.height(14.dp))

                            // Bottom Telemetry Bar: Speed Meter & RTT Latency
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(MaterialTheme.colorScheme.surfaceVariant)
                                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(12.dp))
                                    .padding(horizontal = 12.dp, vertical = 8.dp),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                // Multi-segment LED Throughput Speed Meter (Replaced VU with SPEED)
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(3.dp)
                                ) {
                                    val activeSpeedMb = uploadState.speedMbPerSec
                                    val speedLabel = if (uploadState.isUploading) {
                                        String.format(java.util.Locale.US, "SPEED: %.1f MB/S", activeSpeedMb)
                                    } else {
                                        "SPEED: "
                                    }
                                    Text(
                                        text = speedLabel,
                                        style = MaterialTheme.typography.labelSmall,
                                        fontFamily = FontFamily.Monospace,
                                        fontWeight = FontWeight.Bold,
                                        color = if (uploadState.isUploading) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                    val barCount = when {
                                        uploadState.isUploading -> when {
                                            activeSpeedMb > 45.0 -> 6
                                            activeSpeedMb > 25.0 -> 5
                                            activeSpeedMb > 12.0 -> 4
                                            activeSpeedMb > 5.0 -> 3
                                            activeSpeedMb > 0.0 -> 2
                                            else -> 1
                                        }
                                        onlineState.isOnline -> 5
                                        else -> 1
                                    }
                                    for (i in 1..6) {
                                        val barColor = when {
                                            i > barCount -> MaterialTheme.colorScheme.outlineVariant
                                            i <= 3 -> MaterialTheme.colorScheme.primary
                                            i <= 5 -> MaterialTheme.colorScheme.tertiary
                                            else -> MaterialTheme.colorScheme.primary
                                        }
                                        Box(
                                            modifier = Modifier
                                                .width(4.dp)
                                                .height(12.dp)
                                                .clip(RoundedCornerShape(1.dp))
                                                .background(barColor)
                                        )
                                    }
                                }

                                // RTT Latency / Upload ETA Tag
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                                ) {
                                    Text(
                                        text = if (uploadState.isUploading) "ETA:" else "RTT:",
                                        style = MaterialTheme.typography.labelSmall,
                                        fontFamily = FontFamily.Monospace,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                    Surface(
                                        shape = RoundedCornerShape(6.dp),
                                        color = if (uploadState.isUploading || onlineState.isOnline) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
                                        border = androidx.compose.foundation.BorderStroke(
                                            1.dp,
                                            if (uploadState.isUploading || onlineState.isOnline) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline
                                        )
                                    ) {
                                        val statusPillText = when {
                                            uploadState.isUploading -> if (uploadState.etaSeconds > 0) "${uploadState.etaSeconds}S" else "${uploadState.progressPercent}%"
                                            isPinging -> "SYNC..."
                                            onlineState.isOnline -> "${onlineState.pingMs} MS"
                                            else -> "OFFLINE"
                                        }
                                        Text(
                                            text = statusPillText,
                                            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                            style = MaterialTheme.typography.labelSmall,
                                            fontFamily = FontFamily.Monospace,
                                            fontWeight = FontWeight.Bold,
                                            color = if (uploadState.isUploading || onlineState.isOnline) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                // 3. Tactile Hardware Transmit Deck (3D Mechanical Keycaps)
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            text = "TRANSMIT DECK",
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 1.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(12.dp)
                        ) {
                            // F1 Master Key: 3D Safety Orange Keycap
                            BentoActionTile(
                                icon = Icons.Rounded.Description,
                                label = "Transmit Files",
                                subLabel = "PHOTOS // 4K // DOC",
                                functionKey = "F1",
                                isAccentOrange = true,
                                height = 96.dp,
                                onClick = { filePickerLauncher.launch("*/*") },
                                modifier = Modifier.weight(1f)
                            )

                            // F2 Key: 3D Dark Slate Keycap with Orange Bolt
                            BentoActionTile(
                                icon = Icons.Rounded.Bolt,
                                label = "Clipboard",
                                subLabel = "INSTANT PC PASTE",
                                functionKey = "F2",
                                isAccentOrange = false,
                                height = 96.dp,
                                onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                                    val clip = cm?.primaryClip
                                    val text = if (clip != null && clip.itemCount > 0) {
                                        clip.getItemAt(0).coerceToText(context).toString()
                                    } else ""
                                    textInput = text
                                    showSendTextDialog = true
                                },
                                modifier = Modifier.weight(1f)
                            )
                        }
                    }
                }

                // 4. Windows Clipboard Injection Target (Clean Swiss Sans + 3D Active State)
                item {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Text(
                                text = "WINDOWS CLIPBOARD INJECTION",
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                                letterSpacing = 1.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                            Text(
                                text = "CONFIG",
                                modifier = Modifier.clickable { onNavigateToSettings() },
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        }

                        // Recessed Channel Tray
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(16.dp))
                                .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(16.dp)),
                            shape = RoundedCornerShape(16.dp),
                            color = Color(0xFF0E1013) // Distinct deep recessed tray
                        ) {
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(4.dp),
                                horizontalArrangement = Arrangement.spacedBy(6.dp)
                            ) {
                                val modes = listOf(
                                    ClipboardMode.SMART_MULTI to "01 MULTI",
                                    ClipboardMode.FORCE_PATH to "02 PATH",
                                    ClipboardMode.FORCE_IMAGE to "03 BITMAP"
                                )
                                modes.forEach { (mode, title) ->
                                    val isSelected = onlineState.clipboardMode == mode
                                    val itemInteraction = remember { MutableInteractionSource() }
                                    val isItemPressed by itemInteraction.collectIsPressedAsState()
                                    val itemOffset by animateDpAsState(
                                        targetValue = if (isItemPressed) 2.dp else 0.dp,
                                        label = "ModePress"
                                    )

                                    Box(
                                        modifier = Modifier
                                            .weight(1f)
                                            .height(40.dp)
                                            .clip(RoundedCornerShape(12.dp))
                                            .background(
                                                if (isSelected) Color(0xFF882200) else Color.Transparent
                                            )
                                            .padding(bottom = if (isSelected) 3.dp else 0.dp)
                                            .offset(y = itemOffset)
                                            .clip(RoundedCornerShape(12.dp))
                                            .then(
                                                if (isSelected) {
                                                    Modifier.background(Brush.verticalGradient(listOf(Color(0xFFFF5500), Color(0xFFE63800))))
                                                } else {
                                                    Modifier.background(Color(0xFF16181E))
                                                }
                                            )
                                            .border(
                                                width = 1.dp,
                                                color = if (isSelected) Color.White.copy(alpha = 0.3f) else Color(0xFF282C36),
                                                shape = RoundedCornerShape(12.dp)
                                            )
                                            .clickable(
                                                interactionSource = itemInteraction,
                                                indication = null,
                                                onClick = { onNavigateToSettings() }
                                            ),
                                        contentAlignment = Alignment.Center
                                    ) {
                                        Text(
                                            text = title,
                                            style = MaterialTheme.typography.labelMedium,
                                            fontWeight = FontWeight.Bold,
                                            letterSpacing = 0.3.sp,
                                            color = if (isSelected) Color.White else MaterialTheme.colorScheme.onSurfaceVariant
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                // 5. Hardware Session History (Renamed from session track log, REC removed)
                item {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = "SESSION HISTORY",
                            style = MaterialTheme.typography.labelSmall,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 1.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )

                        if (recentTransfers.isNotEmpty()) {
                            Text(
                                text = "CLEAR",
                                modifier = Modifier.clickable {
                                    preferences.clearRecentTransfers()
                                    recentTransfers = emptyList()
                                },
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary
                            )
                        }
                    }
                }

                if (recentTransfers.isEmpty()) {
                    item {
                        Surface(
                            modifier = Modifier
                                .fillMaxWidth()
                                .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(18.dp)),
                            shape = RoundedCornerShape(18.dp),
                            color = MaterialTheme.colorScheme.surface
                        ) {
                            Text(
                                text = "No recent transfers logged. Transmit photos, files or text to see session records here.",
                                modifier = Modifier.padding(18.dp),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    }
                } else {
                    val rows = recentTransfers.take(6).chunked(2)
                    items(rows) { rowRecords ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            for (record in rowRecords) {
                                BentoHistoryCard(
                                    record = record,
                                    modifier = Modifier.weight(1f)
                                )
                            }
                            if (rowRecords.size == 1) {
                                Spacer(modifier = Modifier.weight(1f))
                            }
                        }
                    }
                }

            } else {
                // Empty state when no PC is paired
                item {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 40.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(16.dp)
                    ) {
                        ScallopedDeviceBadge(isOnline = false, size = 80.dp)
                        Text(
                            text = "NO LINK DETECTED",
                            style = MaterialTheme.typography.headlineMedium,
                            fontWeight = FontWeight.Black,
                            letterSpacing = 1.sp,
                            color = MaterialTheme.colorScheme.onSurface
                        )
                        Text(
                            text = "Right-click the Flick icon in your Windows System Tray and select 'Pair New Phone (Show QR)' to connect instantly.",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(horizontal = 16.dp)
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(
                            onClick = onNavigateToScanner,
                            shape = RoundedCornerShape(16.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = MaterialTheme.colorScheme.primary,
                                contentColor = Color.White
                            ),
                            modifier = Modifier.height(52.dp)
                        ) {
                            Icon(Icons.Rounded.QrCodeScanner, contentDescription = null)
                            Spacer(modifier = Modifier.width(10.dp))
                            Text(
                                "SCAN PC QR CODE",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    }
                }
            }

            item {
                Spacer(modifier = Modifier.height(30.dp))
            }
        }
    }

    // Quick Send Text / Clipboard Dialog
    if (showSendTextDialog) {
        AlertDialog(
            onDismissRequest = { showSendTextDialog = false },
            containerColor = MaterialTheme.colorScheme.surface,
            title = {
                Text(
                    "BEAM TEXT",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )
            },
            text = {
                OutlinedTextField(
                    value = textInput,
                    onValueChange = { textInput = it },
                    label = { Text("RAW TEXT OR URL") },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = MaterialTheme.colorScheme.primary,
                        unfocusedBorderColor = MaterialTheme.colorScheme.outline
                    )
                )
            },
            confirmButton = {
                Button(
                    shape = RoundedCornerShape(10.dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.primary,
                        contentColor = Color.White
                    ),
                    onClick = {
                        val pc = activePc
                        if (pc != null && textInput.isNotEmpty()) {
                            val serviceIntent = Intent(context, UploadService::class.java).apply {
                                action = UploadService.ACTION_UPLOAD
                                putExtra(UploadService.EXTRA_TEXT, textInput)
                            }
                            ContextCompat.startForegroundService(context, serviceIntent)
                            showSendTextDialog = false
                            scope.launch {
                                delay(1000)
                                recentTransfers = preferences.getRecentTransfers()
                            }
                        }
                    }
                ) {
                    Text("TRANSMIT", fontWeight = FontWeight.Bold)
                }
            },
            dismissButton = {
                TextButton(onClick = { showSendTextDialog = false }) {
                    Text("CANCEL", color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        )
    }

    // Declan Tan "Give it a little flick" Interactive Spatial Deck Overlay
    if (pendingFlickUris.isNotEmpty()) {
        SpatialFlickDeck(
            uris = pendingFlickUris,
            pcName = activePc?.pcName ?: "Windows PC",
            isOnline = onlineState.isOnline,
            onFlick = { selected, _ ->
                val serviceIntent = Intent(context, UploadService::class.java).apply {
                    action = UploadService.ACTION_UPLOAD
                    putParcelableArrayListExtra(UploadService.EXTRA_URIS, ArrayList(selected))
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                ContextCompat.startForegroundService(context, serviceIntent)
                pendingFlickUris = emptyList()
                scope.launch {
                    delay(1200)
                    recentTransfers = preferences.getRecentTransfers()
                }
            },
            onDismiss = {
                pendingFlickUris = emptyList()
            }
        )
    }
}

private fun formatBytes(bytes: Long): String {
    if (bytes <= 0) return "0 B"
    val kb = bytes / 1024.0
    val mb = kb / 1024.0
    return when {
        mb >= 1.0 -> String.format(java.util.Locale.US, "%.1f MB", mb)
        kb >= 1.0 -> String.format(java.util.Locale.US, "%.1f KB", kb)
        else -> "$bytes B"
    }
}

private fun formatRelativeTime(timestamp: Long): String {
    val diff = System.currentTimeMillis() - timestamp
    val seconds = (diff / 1000).coerceAtLeast(0)
    val minutes = seconds / 60
    val hours = minutes / 60
    val days = hours / 24
    return when {
        seconds < 60 -> "Just now"
        minutes < 60 -> "${minutes}m ago"
        hours < 24 -> "${hours}h ago"
        else -> "${days}d ago"
    }
}

@Composable
fun BentoHistoryCard(
    record: TransferRecord,
    modifier: Modifier = Modifier
) {
    val isVideo = record.mimeType.startsWith("video/")
    val isImage = record.mimeType.startsWith("image/")
    val isText = record.mimeType.startsWith("text/") || record.mimeType == "application/json"

    val category = when {
        isImage && record.fileName.contains("Screenshot", ignoreCase = true) -> "SCREENSHOT"
        isImage -> "IMAGE"
        isVideo -> "VIDEO"
        isText -> "TEXT"
        else -> "DOCUMENT"
    }

    Surface(
        modifier = modifier
            .clip(RoundedCornerShape(18.dp))
            .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(18.dp)),
        shape = RoundedCornerShape(18.dp),
        color = MaterialTheme.colorScheme.surface,
        shadowElevation = 2.dp
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(8.dp)
        ) {
            // Viewfinder Preview Window (Clean, unblocked preview)
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(98.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(Color(0xFF0D0E10))
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(12.dp))
            ) {
                TransferThumbnail(
                    record = record,
                    modifier = Modifier.fillMaxSize()
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            // Metadata Strip: Category & Relative Time
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Surface(
                    shape = RoundedCornerShape(4.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant
                ) {
                    Text(
                        text = category,
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 8.5.sp,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.5.dp)
                    )
                }

                Text(
                    text = formatRelativeTime(record.timestamp),
                    style = MaterialTheme.typography.labelSmall,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 9.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            Spacer(modifier = Modifier.height(4.dp))

            // Humanized title or cleaned file info
            val cleanTitle = if (record.fileName.startsWith("Screenshot_") || record.fileName.startsWith("VID_") || record.fileName.startsWith("IMG_")) {
                val extension = record.fileName.substringAfterLast(".", "").uppercase()
                "${category.lowercase().replaceFirstChar { it.uppercase() }} • $extension"
            } else {
                record.fileName
            }

            Text(
                text = cleanTitle,
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )

            val sizeAndDuration = if (record.elapsedMs > 0) {
                val durationText = if (record.elapsedMs >= 1000) {
                    String.format(java.util.Locale.US, "%.1fs", record.elapsedMs / 1000.0)
                } else {
                    "${record.elapsedMs}ms"
                }
                "${formatBytes(record.bytes)} • $durationText"
            } else {
                formatBytes(record.bytes)
            }

            Text(
                text = sizeAndDuration,
                style = MaterialTheme.typography.labelSmall,
                fontFamily = FontFamily.Monospace,
                fontSize = 9.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
fun TransferThumbnail(
    record: TransferRecord,
    modifier: Modifier = Modifier
) {
    var bitmap by remember(record.id, record.thumbnailPath) { mutableStateOf<Bitmap?>(null) }

    LaunchedEffect(record.id, record.thumbnailPath) {
        val path = record.thumbnailPath
        if (!path.isNullOrEmpty()) {
            withContext(Dispatchers.IO) {
                try {
                    val file = File(path)
                    if (file.exists()) {
                        val options = BitmapFactory.Options().apply {
                            inPreferredConfig = Bitmap.Config.ARGB_8888
                        }
                        bitmap = BitmapFactory.decodeFile(file.absolutePath, options)
                    }
                } catch (_: Exception) {}
            }
        }
    }

    val isVideo = record.mimeType.startsWith("video/")
    val isImage = record.mimeType.startsWith("image/")

    Surface(
        modifier = modifier.clip(RoundedCornerShape(10.dp)),
        shape = RoundedCornerShape(10.dp),
        color = MaterialTheme.colorScheme.surfaceVariant
    ) {
        val b = bitmap
        if (b != null) {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Image(
                    bitmap = b.asImageBitmap(),
                    contentDescription = record.fileName,
                    contentScale = ContentScale.Crop,
                    filterQuality = FilterQuality.High,
                    modifier = Modifier.fillMaxSize()
                )
                if (isVideo) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(Color.Black.copy(alpha = 0.35f)),
                        contentAlignment = Alignment.Center
                    ) {
                        Surface(
                            shape = CircleShape,
                            color = Color.Black.copy(alpha = 0.6f),
                            modifier = Modifier.size(22.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Rounded.PlayArrow,
                                    contentDescription = "Video",
                                    tint = Color.White,
                                    modifier = Modifier.size(14.dp)
                                )
                            }
                        }
                    }
                }
            }
        } else {
            val icon = when {
                isImage -> Icons.Rounded.Image
                isVideo -> Icons.Rounded.Videocam
                record.mimeType.startsWith("audio/") -> Icons.Rounded.AudioFile
                record.mimeType == "text/plain" -> Icons.Rounded.ContentPaste
                else -> Icons.Rounded.Description
            }
            Box(
                modifier = Modifier.fillMaxSize(),
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(20.dp)
                )
            }
        }
    }
}
