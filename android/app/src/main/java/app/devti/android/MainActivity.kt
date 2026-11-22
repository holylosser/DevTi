package app.devti.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.devti.android.core.DevtiService
import app.devti.android.core.state.looksLikePairingLink
import app.devti.android.ui.DevtiRoot
import app.devti.android.ui.theme.DevtiTheme

class MainActivity : ComponentActivity() {
    private val core get() = (application as DevtiApp).core

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        core.start()
        if (getSharedPreferences("devti_ui", MODE_PRIVATE).getBoolean("onboarded", false)) {
            DevtiService.start(this)
        }
        if (savedInstanceState == null) handleLink(intent)
        setContent { DevtiTheme { DevtiRoot(core) } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleLink(intent)
    }

    override fun onResume() {
        super.onResume()
        core.reconnectNow()
    }

    /** A devti://pair link (from the system camera app) starts pairing directly. */
    private fun handleLink(intent: Intent?) {
        val link = intent?.data?.toString() ?: return
        if (looksLikePairingLink(link)) core.pairWithUri(link)
    }
}
