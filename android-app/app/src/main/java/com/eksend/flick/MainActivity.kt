package com.eksend.flick

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.lifecycleScope
import com.eksend.flick.crypto.KeystoreManager
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.network.UdpDiscoveryListener
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import com.eksend.flick.ui.screens.DeviceSettingsScreen
import com.eksend.flick.ui.screens.HomeScreen
import com.eksend.flick.ui.screens.PairingScannerScreen
import com.eksend.flick.ui.theme.FlickTheme

enum class Screen {
    HOME,
    SETTINGS,
    SCANNER
}

class MainActivity : ComponentActivity() {

    private lateinit var preferences: FlickPreferences
    private val udpDiscoveryListener = UdpDiscoveryListener()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        preferences = FlickPreferences(applicationContext)

        // Proactively generate hardware-backed ECDSA P-256 client keypair on first launch
        lifecycleScope.launch(Dispatchers.IO) {
            KeystoreManager.ensureClientKeypair(preferences.clientId)
        }

        setContent {
            FlickTheme {
                var currentScreen by remember { mutableStateOf(Screen.HOME) }

                // Predictive / system gesture back handler returns to Home instead of exiting app!
                BackHandler(enabled = currentScreen != Screen.HOME) {
                    currentScreen = Screen.HOME
                }

                AnimatedContent(
                    targetState = currentScreen,
                    transitionSpec = { fadeIn() togetherWith fadeOut() },
                    label = "FlickScreenNavigation"
                ) { screen ->
                    when (screen) {
                        Screen.HOME -> {
                            HomeScreen(
                                preferences = preferences,
                                udpListener = udpDiscoveryListener,
                                onNavigateToSettings = { currentScreen = Screen.SETTINGS },
                                onNavigateToScanner = { currentScreen = Screen.SCANNER }
                            )
                        }

                        Screen.SETTINGS -> {
                            DeviceSettingsScreen(
                                preferences = preferences,
                                onBack = { currentScreen = Screen.HOME }
                            )
                        }

                        Screen.SCANNER -> {
                            PairingScannerScreen(
                                preferences = preferences,
                                onPairSuccess = { currentScreen = Screen.HOME },
                                onBack = { currentScreen = Screen.HOME }
                            )
                        }
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        udpDiscoveryListener.startListening(lifecycleScope)
    }

    override fun onPause() {
        udpDiscoveryListener.stopListening()
        super.onPause()
    }
}
