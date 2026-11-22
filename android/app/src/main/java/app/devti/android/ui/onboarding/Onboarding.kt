package app.devti.android.ui.onboarding

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import app.devti.android.core.DevtiCore
import app.devti.android.core.state.CoreState
import app.devti.android.core.state.DiscoveredDevice
import app.devti.android.core.state.PairingStage
import app.devti.android.ui.components.CodeDigits
import app.devti.android.ui.components.GhostButton
import app.devti.android.ui.components.Prism
import app.devti.android.ui.components.PrimaryButton
import app.devti.android.ui.components.QuietButton
import app.devti.android.ui.theme.Devti
import app.devti.android.ui.theme.VOICES

private enum class Step { Welcome, FindHub, Scan, Code, Permissions, Done }

/**
 * First run on the phone: install → find the Hub → scan its QR → compare codes →
 * permissions → done. Mirrors the desktop wizard.
 */
@Composable
fun Onboarding(core: DevtiCore, state: CoreState, skipWelcome: Boolean = false, onFinished: () -> Unit) {
    var step by rememberSaveable { mutableStateOf(if (skipWelcome) Step.FindHub else Step.Welcome) }
    var codeTarget by remember { mutableStateOf<DiscoveredDevice?>(null) }
    val pairing = state.pairing

    // While a pairing is in flight its stage decides what is on screen.
    val pairingView = pairing != null
    LaunchedEffect(pairing) {
        if (pairing is PairingStage.Done && step != Step.Permissions && step != Step.Done) {
            step = Step.Permissions
            core.clearPairing()
        }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Devti.colors.bg)
            .safeDrawingPadding(),
    ) {
        AnimatedContent(
            targetState = if (pairingView) "pairing" else step.name,
            transitionSpec = { (fadeIn() + slideInVertically { it / 12 }) togetherWith fadeOut() },
            label = "onboarding",
        ) { key ->
            when (key) {
                "pairing" -> PairingProgress(state, core, onRetry = { core.clearPairing(); step = Step.FindHub })
                Step.Welcome.name -> Welcome(onNext = { step = Step.FindHub })
                Step.FindHub.name -> FindHub(
                    nearby = state.nearby.filter { it.pairingOpen },
                    onScan = { step = Step.Scan },
                    onCode = { codeTarget = it; step = Step.Code },
                )
                Step.Scan.name -> ScanStep(onCode = { core.pairWithUri(it) }, onBack = { step = Step.FindHub })
                Step.Code.name -> CodeStep(
                    target = codeTarget,
                    onSubmit = { code -> codeTarget?.let { core.pairWithCode(it.id, code) } },
                    onBack = { step = Step.FindHub },
                )
                Step.Permissions.name -> PermissionsStep(onNext = { step = Step.Done })
                else -> DoneStep(hubName = state.hub?.info?.name, onFinished = onFinished)
            }
        }
    }
}

@Composable
private fun Page(content: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 32.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) { content() }
}

@Composable
private fun Welcome(onNext: () -> Unit) {
    val c = Devti.colors
    Page {
        Spacer(Modifier.height(24.dp))
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            Prism(size = 168.dp, voices = VOICES.mapNotNull { c.voices[it] }, label = "Devti")
        }
        Spacer(Modifier.height(16.dp))
        Text("Your phone, part of one living system.", style = Devti.type.hero, color = c.text)
        Text(
            "Devti connects this phone straight to your computer over your own Wi-Fi. Private, encrypted and free.",
            style = Devti.type.body,
            color = c.textDim,
        )
        Spacer(Modifier.height(8.dp))
        PrimaryButton("Get started", onClick = onNext, modifier = Modifier.fillMaxWidth())
        Text("No account. Nothing leaves your network.", style = Devti.type.small, color = c.textFaint)
    }
}

@Composable
private fun FindHub(nearby: List<DiscoveredDevice>, onScan: () -> Unit, onCode: (DiscoveredDevice) -> Unit) {
    val c = Devti.colors
    Page {
        Text("Find your computer", style = Devti.type.title, color = c.text)
        Text(
            "On your computer, open Devti and choose Pair a device. A QR code appears on its screen.",
            style = Devti.type.body,
            color = c.textDim,
        )
        PrimaryButton("Scan the QR code", onClick = onScan, modifier = Modifier.fillMaxWidth())
        if (nearby.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            Text("Ready to pair on your network", style = Devti.type.heading, color = c.text)
            Text("Can't scan? Pick your computer and type the code it shows.", style = Devti.type.small, color = c.textDim)
            nearby.forEach { d ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(c.surface2)
                        .clickable { onCode(d) }
                        .padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(d.name, style = Devti.type.label, color = c.text)
                        Text("Enter code", style = Devti.type.small, color = c.textDim)
                    }
                }
            }
        } else {
            Text(
                "Make sure this phone and your computer are on the same Wi-Fi network.",
                style = Devti.type.small,
                color = c.textFaint,
            )
        }
    }
}

@Composable
private fun ScanStep(onCode: (String) -> Unit, onBack: () -> Unit) {
    val c = Devti.colors
    Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text("Point at the code", style = Devti.type.title, color = c.text)
        Text("Hold the phone so the QR code on your computer fits inside the frame.", style = Devti.type.body, color = c.textDim)
        QrScanner(onCode = onCode, modifier = Modifier.fillMaxWidth().weight(1f))
        GhostButton("Back", onClick = onBack)
    }
}

@Composable
private fun CodeStep(target: DiscoveredDevice?, onSubmit: (String) -> Unit, onBack: () -> Unit) {
    val c = Devti.colors
    var code by remember { mutableStateOf("") }
    Page {
        Text("Enter the code", style = Devti.type.title, color = c.text)
        Text("Type the 6 digits shown on ${target?.name ?: "your computer"}.", style = Devti.type.body, color = c.textDim)
        OutlinedTextField(
            value = code,
            onValueChange = { v -> code = v.filter { it.isDigit() }.take(6) },
            label = { Text("Pairing code") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            modifier = Modifier.fillMaxWidth(),
        )
        PrimaryButton("Pair", onClick = { onSubmit(code) }, enabled = code.length == 6, modifier = Modifier.fillMaxWidth())
        GhostButton("Back", onClick = onBack)
    }
}

/** Connecting → compare codes → (done handled by caller) or failed. */
@Composable
fun PairingProgress(state: CoreState, core: DevtiCore, onRetry: () -> Unit) {
    val c = Devti.colors
    Page {
        when (val p = state.pairing) {
            is PairingStage.Connecting, null -> {
                Spacer(Modifier.height(48.dp))
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Prism(132.dp, emptyList()) }
                Text("Connecting", style = Devti.type.title, color = c.text)
                Text("Agreeing on a secret key with your computer.", style = Devti.type.body, color = c.textDim)
            }
            is PairingStage.Verify -> {
                Text("Do the codes match?", style = Devti.type.title, color = c.text)
                Text("${p.peer.name} shows a code too. Check that they're the same.", style = Devti.type.body, color = c.textDim)
                Box(
                    Modifier.fillMaxWidth().padding(vertical = 16.dp).semantics { contentDescription = "Verification code ${p.sas}" },
                    contentAlignment = Alignment.Center,
                ) { CodeDigits(p.sas) }
                if (p.confirmed) {
                    Text("Waiting for ${p.peer.name} to confirm…", style = Devti.type.body, color = c.textDim)
                } else {
                    PrimaryButton("The codes match", onClick = { core.confirm(true) }, modifier = Modifier.fillMaxWidth())
                    QuietButton("They're different", onClick = { core.confirm(false) }, modifier = Modifier.fillMaxWidth())
                }
                Text("Matching codes prove nobody is listening in between your devices.", style = Devti.type.small, color = c.textFaint)
            }
            is PairingStage.Failed -> {
                Text("Pairing didn't finish", style = Devti.type.title, color = c.text)
                Text(p.message, style = Devti.type.body, color = c.textDim)
                PrimaryButton("Try again", onClick = onRetry, modifier = Modifier.fillMaxWidth())
            }
            is PairingStage.Done -> {
                Text("${p.peer.info.name} is connected", style = Devti.type.title, color = c.text)
            }
        }
    }
}

@Composable
private fun PermissionsStep(onNext: () -> Unit) {
    val c = Devti.colors
    val context = LocalContext.current
    val notify = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    Page {
        Text("Stay connected", style = Devti.type.title, color = c.text)
        Text(
            "Two settings keep your phone in touch with your computer when the screen is off.",
            style = Devti.type.body,
            color = c.textDim,
        )
        PermissionRow(
            title = "Notifications",
            why = "Android shows a quiet notification while Devti keeps the connection open. You can collapse it.",
            action = if (Build.VERSION.SDK_INT >= 33) "Allow" else null,
            onAction = { notify.launch(Manifest.permission.POST_NOTIFICATIONS) },
        )
        val pm = context.getSystemService(PowerManager::class.java)
        val exempt = pm?.isIgnoringBatteryOptimizations(context.packageName) == true
        PermissionRow(
            title = "Battery optimization",
            why = if (exempt) {
                "Already unrestricted. Devti uses almost no battery while idle."
            } else {
                "Android may cut the connection to save power. Set Devti to Unrestricted so it stays connected."
            },
            action = if (exempt) null else "Open setting",
            onAction = {
                val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))
                runCatching { context.startActivity(intent) }
                    .onFailure { context.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }
            },
        )
        Spacer(Modifier.height(8.dp))
        PrimaryButton("Continue", onClick = onNext, modifier = Modifier.fillMaxWidth())
    }
}

@Composable
private fun PermissionRow(title: String, why: String, action: String?, onAction: () -> Unit) {
    val c = Devti.colors
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(c.surface)
            .border(1.dp, c.line, RoundedCornerShape(18.dp))
            .padding(18.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(title, style = Devti.type.label, color = c.text)
        Text(why, style = Devti.type.small, color = c.textDim)
        if (action != null) GhostButton(action, onClick = onAction)
    }
}

@Composable
private fun DoneStep(hubName: String?, onFinished: () -> Unit) {
    val c = Devti.colors
    Page {
        Spacer(Modifier.height(32.dp))
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            Box(Modifier.size(12.dp).clip(CircleShape).background(c.ok))
        }
        Text(if (hubName != null) "Connected to $hubName" else "Devti is ready", style = Devti.type.title, color = c.text)
        Text(
            "Your phone reconnects by itself whenever it's on the same network as your computer.",
            style = Devti.type.body,
            color = c.textDim,
        )
        PrimaryButton("Open Devti", onClick = onFinished, modifier = Modifier.fillMaxWidth())
    }
}

/** A dot in a device's voice — used in lists. */
@Composable
fun VoiceDot(color: Color) {
    Box(Modifier.size(10.dp).clip(CircleShape).background(color))
}
