package app.devti.android.core

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.Network
import android.net.wifi.WifiManager
import android.os.BatteryManager
import android.os.Build
import android.provider.Settings
import app.devti.android.BuildConfig
import app.devti.android.core.state.ClipboardEntry
import app.devti.android.core.state.CoreState
import app.devti.android.core.state.PairingStage
import app.devti.android.core.state.Reducer
import androidx.core.content.ContextCompat
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.devti_ffi.DevtiException
import uniffi.devti_ffi.DevtiMobile
import uniffi.devti_ffi.EventListener
import uniffi.devti_ffi.MobileConfig

/**
 * The app's single Devti Core node and the observable [CoreState] the UI renders.
 *
 * Section by section:
 * - start(): creates the Rust node (identity in the Keystore, database in app storage).
 * - events: the core calls [onEvent] on its own thread; [Reducer] folds it into state.
 * - device signals: battery level and network changes are pushed into the core.
 * - multicast: Android drops mDNS packets unless a MulticastLock is held.
 */
class DevtiCore(private val context: Context) : EventListener {
    private val _state = MutableStateFlow(CoreState())
    val state: StateFlow<CoreState> = _state.asStateFlow()

    @Volatile private var node: DevtiMobile? = null
    private var multicast: WifiManager.MulticastLock? = null
    private var batteryReceiver: BroadcastReceiver? = null
    private var networkCallback: ConnectivityManager.NetworkCallback? = null

    @Synchronized
    fun start(): DevtiMobile? {
        node?.let { return it }
        return try {
            val created = DevtiMobile(
                MobileConfig(
                    dataDir = context.filesDir.resolve("devti").apply { mkdirs() }.absolutePath,
                    deviceName = deviceName(),
                    kind = if (isTablet()) "tablet" else "phone",
                    platform = "android",
                    osVersion = "Android ${Build.VERSION.RELEASE}",
                    model = "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}",
                    appVersion = BuildConfig.VERSION_NAME,
                    discovery = true,
                ),
                KeystoreSecretStore(context),
                this,
            )
            node = created
            refresh()
            watchDevice()
            created
        } catch (e: DevtiException) {
            _state.update { it.copy(error = e.readable()) }
            null
        } catch (e: Throwable) {
            _state.update { it.copy(error = "Devti couldn't start: ${e.message ?: e.javaClass.simpleName}") }
            null
        }
    }

    fun refresh() {
        val n = node ?: return
        val snapshot = Reducer.snapshot(n.localJson(), n.peersJson(), n.nearbyJson())
        _state.update { snapshot.copy(pairing = it.pairing, clipboardRevision = it.clipboardRevision + 1) }
    }

    override fun onEvent(eventJson: String) {
        if (eventJson.contains("\"resync\"")) {
            refresh()
            return
        }
        _state.update { Reducer.apply(it, eventJson) }
    }

    // ----------------------------------------------------------- actions

    fun pairWithUri(uri: String): String? = act {
        _state.update { it.copy(pairing = PairingStage.Connecting(-1)) }
        start()?.pairWithUri(uri)
    }

    fun pairWithCode(deviceId: String, code: String): String? = act {
        _state.update { it.copy(pairing = PairingStage.Connecting(-1)) }
        start()?.pairWithCode(deviceId, code)
    }

    fun confirm(accept: Boolean): String? = act {
        val verify = _state.value.pairing as? PairingStage.Verify ?: return@act
        if (accept) _state.update { it.copy(pairing = verify.copy(confirmed = true)) }
        node?.confirmPairing(verify.pairingId.toULong(), accept)
    }

    fun clearPairing() = _state.update { it.copy(pairing = null) }

    fun dismissRemovedNotice() = _state.update { it.copy(removedNotice = null) }

    fun rename(name: String): String? = act { node?.rename(name) }

    fun setPaused(paused: Boolean): String? = act { node?.setPaused(paused) }

    fun forget(deviceId: String): String? = act { node?.revoke(deviceId) }

    private fun runningNode(): DevtiMobile = checkNotNull(node) { "Devti is still starting." }

    fun clipboardHistory(): List<ClipboardEntry> =
        Reducer.json.decodeFromString(runningNode().clipboardHistoryJson())

    fun shareClipboard(deviceId: String, text: String) {
        runningNode().sendClipboardTo(deviceId, text)
        _state.update { it.copy(clipboardRevision = it.clipboardRevision + 1) }
    }

    fun setClipboardAccess(deviceId: String, allowed: Boolean) =
        runningNode().setClipboardAccess(deviceId, allowed)

    fun requestRing(deviceId: String, on: Boolean) = runningNode().ring(deviceId, on)

    fun setCommandsAccess(deviceId: String, allowed: Boolean) = runningNode().setCommandsAccess(deviceId, allowed)

    fun stopRinging() = _state.update { it.copy(ringingPeerId = null) }

    fun clearClipboardHistory() {
        runningNode().clearClipboardHistory()
        _state.update { it.copy(clipboardRevision = it.clipboardRevision + 1) }
    }

    fun reconnectNow() {
        node?.reconnectNow()
    }

    private inline fun act(block: () -> Unit): String? =
        try {
            block()
            null
        } catch (e: DevtiException) {
            val msg = e.readable()
            if (_state.value.pairing is PairingStage.Connecting) {
                _state.update { it.copy(pairing = PairingStage.Failed(-1, "other", msg)) }
            }
            msg
        }

    // ------------------------------------------------------- device signals

    private fun watchDevice() {
        val wifi = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
        multicast = wifi?.createMulticastLock("devti-mdns")?.apply {
            setReferenceCounted(false)
            acquire()
        }

        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context, intent: Intent) {
                val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
                val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
                val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
                if (level < 0 || scale <= 0) return
                val pct = (level * 100 / scale).coerceIn(0, 100)
                val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
                node?.setBattery(pct.toUByte(), charging)
            }
        }
        batteryReceiver = receiver
        // Protected system broadcast: still delivered to a not-exported receiver.
        ContextCompat.registerReceiver(
            context,
            receiver,
            IntentFilter(Intent.ACTION_BATTERY_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )

        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) = reconnectNow()
        }
        networkCallback = callback
        cm.registerDefaultNetworkCallback(callback)
    }

    private fun deviceName(): String {
        val user = Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
        return user?.takeIf { it.isNotBlank() } ?: Build.MODEL
    }

    private fun isTablet(): Boolean = context.resources.configuration.smallestScreenWidthDp >= 600
}

/** The human sentence inside a core error. */
fun DevtiException.readable(): String = when (this) {
    is DevtiException.InvalidInput -> reason
    is DevtiException.Failed -> reason
    is DevtiException.Paused -> "Devti is paused on this phone. Resume it in Settings."
}
