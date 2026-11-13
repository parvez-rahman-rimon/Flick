package com.eksend.flick.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Sensors
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.eksend.flick.data.DiscoveredPc

@Composable
fun DiscoveredPairCard(
    pc: DiscoveredPc,
    onPair: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(140.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        // Left Technical Receiver Card
        Card(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight()
                .border(
                    width = 1.dp,
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.6f),
                    shape = RoundedCornerShape(20.dp)
                ),
            shape = RoundedCornerShape(20.dp),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant
            )
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 18.dp, vertical = 14.dp),
                verticalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp)
                ) {
                    Icon(
                        imageVector = Icons.Rounded.Sensors,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(16.dp)
                    )
                    Text(
                        text = "RF LINK DETECTED // 45454-UDP",
                        style = MaterialTheme.typography.labelSmall,
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary,
                        letterSpacing = 0.5.sp
                    )
                }

                Column {
                    Text(
                        text = pc.pcName.uppercase(),
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1
                    )
                    Spacer(modifier = Modifier.height(2.dp))
                    Text(
                        text = "IP: ${pc.ip}:${pc.port} • ID: ${pc.pcId.take(8).uppercase()}",
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }

        // Right Physical Tactile Split Action Buttons
        Column(
            modifier = Modifier
                .width(62.dp)
                .fillMaxHeight(),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            // Dismiss Button
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(44.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(14.dp))
                    .clickable { onDismiss() },
                shape = RoundedCornerShape(14.dp),
                color = MaterialTheme.colorScheme.secondaryContainer
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        imageVector = Icons.Rounded.Close,
                        contentDescription = "Dismiss",
                        tint = MaterialTheme.colorScheme.onSecondaryContainer,
                        modifier = Modifier.size(18.dp)
                    )
                }
            }

            // Pair Master Trigger (Safety Orange)
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .clip(RoundedCornerShape(16.dp))
                    .border(1.dp, MaterialTheme.colorScheme.primary, RoundedCornerShape(16.dp))
                    .clickable { onPair() },
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.primary,
                shadowElevation = 3.dp
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        imageVector = Icons.Rounded.Check,
                        contentDescription = "Pair",
                        tint = Color.White,
                        modifier = Modifier.size(26.dp)
                    )
                }
            }
        }
    }
}
