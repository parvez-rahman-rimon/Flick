package com.eksend.flick

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.lifecycleScope
import com.eksend.flick.crypto.KeystoreManager
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.network.TurboLinkController
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
    private val turboPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { _ -> }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        preferences = FlickPreferences(applicationContext)

        if (preferences.isTurboLinkEnabled && !TurboLinkController.hasRequiredPermission(this)) {
            turboPermissionLauncher.launch(TurboLinkController.requiredPermissionName())
        }

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

                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    AnimatedContent(
                        targetState = currentScreen,
                        transitionSpec = {
                            if (targetState.ordinal > initialState.ordinal) {
                                (slideInHorizontally(animationSpec = tween(240)) { it / 3 } + fadeIn(animationSpec = tween(240)))
                                    .togetherWith(slideOutHorizontally(animationSpec = tween(200)) { -it / 4 } + fadeOut(animationSpec = tween(200)))
                            } else {
                                (slideInHorizontally(animationSpec = tween(240)) { -it / 3 } + fadeIn(animationSpec = tween(240)))
                                    .togetherWith(slideOutHorizontally(animationSpec = tween(200)) { it / 4 } + fadeOut(animationSpec = tween(200)))
                            }
                        },
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
