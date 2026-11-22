package app.devti.android.ui

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.devti.android.core.DevtiCore
import app.devti.android.core.DevtiService
import app.devti.android.ui.components.Prism
import app.devti.android.ui.home.Home
import app.devti.android.ui.onboarding.Onboarding
import app.devti.android.ui.theme.Devti

/** Chooses between first-run setup and the home screen. */
@Composable
fun DevtiRoot(core: DevtiCore) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences("devti_ui", Context.MODE_PRIVATE) }
    val state by core.state.collectAsStateWithLifecycle()
    var onboarded by remember { mutableStateOf(prefs.getBoolean("onboarded", false)) }
    var pairing by rememberSaveable { mutableStateOf(false) }
    val c = Devti.colors
    // A system-camera link can start pairing while Home is visible.
    LaunchedEffect(state.pairing) {
        if (state.pairing != null) pairing = true
    }

    when {
        state.error != null -> Column(
            Modifier.fillMaxSize().background(c.bg).safeDrawingPadding().padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Prism(96.dp, emptyList(), breathe = false)
            Text("Devti couldn't start", style = Devti.type.title, color = c.text)
            Text(state.error ?: "", style = Devti.type.body, color = c.textDim)
        }
        !state.ready -> Box(Modifier.fillMaxSize().background(c.bg), contentAlignment = Alignment.Center) {
            Prism(96.dp, emptyList(), label = "Starting Devti")
        }
        !onboarded || pairing -> Onboarding(core, state, skipWelcome = onboarded, onFinished = {
            prefs.edit { putBoolean("onboarded", true) }
            onboarded = true
            pairing = false
            DevtiService.start(context)
        })
        else -> Home(core, state, onPairAnother = {
            core.clearPairing()
            pairing = true
        })
    }
}
