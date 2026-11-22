package app.devti.android.ui.home

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
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import app.devti.android.core.DevtiCore
import app.devti.android.core.state.CoreState
import app.devti.android.core.state.PeerSnapshot
import app.devti.android.ui.components.BatteryBar
import app.devti.android.ui.components.GhostButton
import app.devti.android.ui.components.Prism
import app.devti.android.ui.components.PrimaryButton
import app.devti.android.ui.components.QuietButton
import app.devti.android.ui.onboarding.VoiceDot
import app.devti.android.ui.theme.Devti
import app.devti.android.ui.theme.voiceFor

/** What works on Android in this release — shown honestly in the app. */
private val WORKS_NOW = listOf(
    "Pairing with a QR code or 6-digit code, verified on both screens",
    "Encrypted connection that reconnects by itself",
    "Sharing this phone's battery level with your computer",
    "Sending and receiving clipboard text with paired devices",
    "Pausing Devti or removing a computer at any time",
)

@Composable
fun Home(core: DevtiCore, state: CoreState, onPairAnother: () -> Unit) {
    var clipboardOpen by rememberSaveable { mutableStateOf(false) }
    if (clipboardOpen) {
        ClipboardScreen(core, state, onBack = { clipboardOpen = false })
        return
    }
    val c = Devti.colors
    val hub = state.hub
    val local = state.local
    var renaming by remember { mutableStateOf(false) }
    var forget by remember { mutableStateOf<PeerSnapshot?>(null) }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.bg)
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        state.removedNotice?.let {
            Notice(it, onDismiss = core::dismissRemovedNotice)
        }

        // The Hub, drawn as the prism; it lights up in this phone's voice when connected.
        val myVoice = c.voices[voiceFor(local?.info?.id ?: "")] ?: c.hub
        Box(Modifier.fillMaxWidth().padding(top = 16.dp), contentAlignment = Alignment.Center) {
            Prism(150.dp, if (hub?.online == true) listOf(myVoice) else emptyList(), label = "Your computer")
        }
        Text(hub?.info?.name ?: "No computer yet", style = Devti.type.hero, color = c.text)
        Text(
            when {
                local?.paused == true -> "Devti is paused on this phone."
                hub == null -> "Pair with your computer to join its ecosystem."
                hub.online -> "Connected" + (hub.connection?.let { ", ${it.rttMs} ms away" } ?: "")
                else -> "Waiting for ${hub.info.name}. Both devices need to be on the same network."
            },
            style = Devti.type.body,
            color = c.textDim,
        )
        if (hub == null) PrimaryButton("Pair with a computer", onClick = onPairAnother, modifier = Modifier.fillMaxWidth())

        // This phone.
        Card {
            Row(verticalAlignment = Alignment.CenterVertically) {
                VoiceDot(myVoice)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(local?.info?.name ?: "This phone", style = Devti.type.label, color = c.text)
                    Text("This phone", style = Devti.type.small, color = c.textDim)
                }
                local?.status?.battery?.let { BatteryBar(it.percent, myVoice) }
            }
            GhostButton("Rename", onClick = { renaming = true })
        }

        QuietButton("Clipboard", onClick = { clipboardOpen = true }, modifier = Modifier.fillMaxWidth())

        // Other paired devices.
        if (state.peers.isNotEmpty()) {
            Text("Paired", style = Devti.type.heading, color = c.text)
            state.peers.forEach { p ->
                val voice = c.voices[voiceFor(p.info.id)] ?: c.hub
                Card {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        VoiceDot(if (p.online) voice else c.textFaint)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(p.info.name, style = Devti.type.label, color = c.text)
                            Text(if (p.online) "Connected" else "Offline", style = Devti.type.small, color = c.textDim)
                        }
                        p.status.battery?.let { BatteryBar(it.percent, voice) }
                    }
                    GhostButton("Remove", onClick = { forget = p })
                }
            }
            QuietButton("Pair another computer", onClick = onPairAnother, modifier = Modifier.fillMaxWidth())
        }

        // Honest scope for this release.
        Text("What works on Android today", style = Devti.type.heading, color = c.text)
        Card {
            WORKS_NOW.forEach { Text("• $it", style = Devti.type.small, color = c.textDim) }
            Text(
                "File sharing, notifications, screen sharing and remote control arrive in later Android updates.",
                style = Devti.type.small,
                color = c.textFaint,
            )
        }

        QuietButton(
            if (local?.paused == true) "Resume Devti" else "Pause Devti",
            onClick = { core.setPaused(local?.paused != true) },
            modifier = Modifier.fillMaxWidth(),
            danger = local?.paused != true,
        )
        Spacer(Modifier.height(24.dp))
    }

    if (renaming) {
        var name by remember { mutableStateOf(local?.info?.name ?: "") }
        AlertDialog(
            onDismissRequest = { renaming = false },
            title = { Text("Rename this phone") },
            text = {
                OutlinedTextField(value = name, onValueChange = { name = it.take(48) }, singleLine = true, label = { Text("Name") })
            },
            confirmButton = { TextButton(onClick = { core.rename(name); renaming = false }) { Text("Save") } },
            dismissButton = { TextButton(onClick = { renaming = false }) { Text("Cancel") } },
        )
    }
    forget?.let { p ->
        AlertDialog(
            onDismissRequest = { forget = null },
            title = { Text("Remove ${p.info.name}?") },
            text = { Text("It will be disconnected and forgotten on both devices. You can pair again later.") },
            confirmButton = { TextButton(onClick = { core.forget(p.info.id); forget = null }) { Text("Remove") } },
            dismissButton = { TextButton(onClick = { forget = null }) { Text("Keep") } },
        )
    }
}

@Composable
private fun Card(content: @Composable () -> Unit) {
    val c = Devti.colors
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(c.surface)
            .border(1.dp, c.line, RoundedCornerShape(18.dp))
            .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) { content() }
}

@Composable
private fun Notice(text: String, onDismiss: () -> Unit) {
    val c = Devti.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(c.surface2)
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = Devti.type.small, color = c.text, modifier = Modifier.weight(1f))
        GhostButton("OK", onClick = onDismiss)
    }
}
