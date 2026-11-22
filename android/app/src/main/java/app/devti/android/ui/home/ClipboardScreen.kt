package app.devti.android.ui.home

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.devti.android.core.DevtiCore
import app.devti.android.core.readable
import app.devti.android.core.state.ClipboardEntry
import app.devti.android.core.state.CoreState
import app.devti.android.ui.components.GhostButton
import app.devti.android.ui.components.PrimaryButton
import app.devti.android.ui.theme.Devti
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.devti_ffi.DevtiException

@Composable
fun ClipboardScreen(core: DevtiCore, state: CoreState, onBack: () -> Unit) {
    val context = LocalContext.current
    val clipboard = remember(context) { context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager }
    val scope = rememberCoroutineScope()
    val c = Devti.colors
    var entries by remember { mutableStateOf(emptyList<ClipboardEntry>()) }
    var feedback by remember { mutableStateOf("") }
    var confirmClear by remember { mutableStateOf(false) }
    BackHandler(onBack = onBack)

    LaunchedEffect(core, state.clipboardRevision) {
        try {
            entries = withContext(Dispatchers.IO) { core.clipboardHistory() }
        } catch (e: DevtiException) {
            feedback = e.readable()
        }
    }

    fun update(action: () -> String) {
        scope.launch {
            try {
                feedback = withContext(Dispatchers.IO) { action() }
            } catch (e: DevtiException) {
                feedback = e.readable()
            }
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize().safeDrawingPadding().padding(horizontal = 24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            GhostButton("Back", onClick = onBack)
            Text("Clipboard", style = Devti.type.title, color = c.text)
            Text("Choose a device to send the text on this phone’s clipboard.", style = Devti.type.body, color = c.textDim)
            if (state.peers.isEmpty()) Text("Pair a computer to share text.", color = c.textDim)
            if (state.local?.paused == true) Text("Resume Devti to send text.", color = c.textDim)
        }
        items(state.peers, key = { it.info.id }) { peer ->
            val name = peer.info.name
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(name, style = Devti.type.heading, color = c.text)
                        Text(if (peer.online) "Online" else "Offline", color = c.textDim)
                    }
                    Switch(
                        checked = peer.permissions.clipboard,
                        onCheckedChange = { allowed ->
                            update {
                                core.setClipboardAccess(peer.info.id, allowed)
                                if (allowed) "Clipboard sharing with $name is on." else "Clipboard sharing with $name is off."
                            }
                        },
                        modifier = Modifier.semantics { contentDescription = "Allow clipboard sharing with $name" },
                    )
                }
                PrimaryButton(
                    "Send text to $name",
                    enabled = peer.online && peer.permissions.clipboard && state.local?.paused == false,
                    onClick = {
                        val clip = try { clipboard.primaryClip } catch (_: SecurityException) { null }
                        val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text?.toString()
                        if (text.isNullOrBlank()) {
                            feedback = "Copy some text on this phone first."
                        } else {
                            update {
                                core.shareClipboard(peer.info.id, text)
                                "Queued for $name."
                            }
                        }
                    },
                )
            }
        }
        item {
            if (feedback.isNotEmpty()) {
                Text(feedback, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }, color = c.text)
            }
            Text("Recent text", style = Devti.type.heading, color = c.text)
            if (entries.isEmpty()) Text("Shared text will appear here.", color = c.textDim)
            else GhostButton("Clear history", onClick = { confirmClear = true })
        }
        items(entries, key = { it.id }) { entry ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(entry.deviceName ?: "This phone", style = Devti.type.label, color = c.textDim)
                    Text(entry.text, maxLines = 2, overflow = TextOverflow.Ellipsis, color = c.text)
                }
                GhostButton("Copy", onClick = {
                    feedback = try {
                        clipboard.setPrimaryClip(ClipData.newPlainText("Devti", entry.text))
                        "Copied to this phone."
                    } catch (_: SecurityException) {
                        "Android could not write to the clipboard."
                    }
                })
            }
        }
    }

    if (confirmClear) AlertDialog(
        onDismissRequest = { confirmClear = false },
        title = { Text("Clear clipboard history on this phone?") },
        confirmButton = { TextButton(onClick = {
            confirmClear = false
            update {
                core.clearClipboardHistory()
                "History cleared."
            }
        }) { Text("Clear history") } },
        dismissButton = { TextButton(onClick = { confirmClear = false }) { Text("Cancel") } },
    )
}
