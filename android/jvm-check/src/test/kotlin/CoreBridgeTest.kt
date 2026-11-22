import app.devti.android.core.state.ClipboardEntry
import app.devti.android.core.state.CoreState
import app.devti.android.core.state.PairingStage
import app.devti.android.core.state.Reducer
import app.devti.android.core.state.formatCode
import app.devti.android.core.state.looksLikePairingLink
import java.nio.file.Files
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicReference
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import uniffi.devti_ffi.DevtiException
import uniffi.devti_ffi.DevtiMobile
import uniffi.devti_ffi.EventListener
import uniffi.devti_ffi.MobileConfig
import uniffi.devti_ffi.PlatformSecretStore
import uniffi.devti_ffi.coreVersion

/** A phone-side node driven exactly like the Android app drives it. */
private class Device(name: String, kind: String, platform: String) : EventListener, PlatformSecretStore {
    private val secrets = ConcurrentHashMap<String, ByteArray>()
    val state = AtomicReference(CoreState())
    val core: DevtiMobile = DevtiMobile(
        MobileConfig(
            dataDir = Files.createTempDirectory("devti-jvm").toString(),
            deviceName = name,
            kind = kind,
            platform = platform,
            osVersion = "test",
            model = null,
            appVersion = "0.1.0",
            discovery = false,
        ),
        this,
        this,
    )

    init {
        state.set(Reducer.snapshot(core.localJson(), core.peersJson(), core.nearbyJson()))
    }

    override fun onEvent(eventJson: String) {
        state.updateAndGet { Reducer.apply(it, eventJson) }
    }

    override fun get(key: String): ByteArray? = secrets[key]
    override fun set(key: String, value: ByteArray) { secrets[key] = value }
    override fun delete(key: String) { secrets.remove(key) }
    override fun backendName(): String = "test keystore"

    fun waitFor(what: String, check: (CoreState) -> Boolean): CoreState {
        val end = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < end) {
            val s = state.get()
            if (check(s)) return s
            Thread.sleep(25)
        }
        error("timed out waiting for $what; state=${state.get()}")
    }
}

class CoreBridgeTest {
    @Test
    fun helpers() {
        assertEquals("482 913", formatCode("482913"))
        assertTrue(looksLikePairingLink(" devti://pair?v=1&x"))
        assertTrue(!looksLikePairingLink("https://example.com"))
        assertTrue(coreVersion().isNotEmpty())
    }

    @Test
    fun reducerIgnoresGarbageAndUnknownEvents() {
        val s = CoreState()
        assertEquals(s, Reducer.apply(s, "not json"))
        assertEquals(s, Reducer.apply(s, """{"type":"from_the_future","x":1}"""))
        assertEquals(s, Reducer.apply(s, """{"type":"peer_changed","peer":{"broken":true}}"""))
    }

    @Test
    fun phoneLikeNodePairsWithHubLikeNodeThroughTheBindings() {
        val hub = Device("Studio Laptop", "laptop", "linux")
        val phone = Device("Pixel 8", "phone", "android")
        assertEquals("test keystore", phone.state.get().local?.secretBackend)

        // The phone joins exactly as if it had scanned the Hub's QR code.
        val window = Reducer.json.parseToJsonElement(hub.core.openPairingWindowJson()).jsonObject
        val uri = window["uri"]!!.jsonPrimitive.content
        val id = phone.core.pairWithUri(uri)
        val verifying = phone.waitFor("verify") { it.pairing is PairingStage.Verify }
        val v = verifying.pairing as PairingStage.Verify
        assertEquals(id.toLong(), v.pairingId)
        assertEquals("Studio Laptop", v.peer.name)
        assertEquals(7, v.sas.length)

        val hubVerify = hub.waitFor("hub verify") { it.pairing is PairingStage.Verify }.pairing as PairingStage.Verify
        assertEquals(v.sas, hubVerify.sas)
        hub.core.confirmPairing(hubVerify.pairingId.toULong(), true)
        phone.core.confirmPairing(id, true)

        val paired = phone.waitFor("online hub") { s -> s.peers.any { it.online } }
        assertEquals("Studio Laptop", paired.hub?.info?.name)
        assertTrue(paired.pairing is PairingStage.Done)

        // Battery from the phone reaches the hub.
        phone.core.setBattery(57u, true)
        val hubSees = hub.waitFor("battery") { s -> s.peers.any { it.status.battery?.percent == 57 } }
        assertEquals(true, hubSees.peers.first().status.battery?.charging)

        // Share through the actual Kotlin bindings, then read and clear the history used by the phone UI.
        val hubId = hub.state.get().local!!.info.id
        val phoneId = phone.state.get().local!!.info.id
        phone.core.sendClipboardTo(hubId, "from the phone")
        hub.waitFor("clipboard from phone") { it.clipboardRevision > 0 }
        assertEquals("from the phone", Reducer.json.decodeFromString<List<ClipboardEntry>>(hub.core.clipboardHistoryJson()).first().text)
        hub.core.sendClipboardTo(phoneId, "from the laptop")
        phone.waitFor("clipboard from hub") { it.clipboardRevision > 0 }
        val received = Reducer.json.decodeFromString<List<ClipboardEntry>>(phone.core.clipboardHistoryJson()).first()
        assertEquals("from the laptop", received.text)
        assertEquals("Studio Laptop", received.deviceName)
        phone.core.setClipboardAccess(hubId, false)
        assertFailsWith<DevtiException.InvalidInput> { phone.core.sendClipboardTo(hubId, "disabled") }
        phone.core.clearClipboardHistory()
        assertTrue(Reducer.json.decodeFromString<List<ClipboardEntry>>(phone.core.clipboardHistoryJson()).isEmpty())

        // Errors arrive as typed exceptions with a readable reason.
        val bad = assertFailsWith<DevtiException.InvalidInput> { phone.core.pairWithUri("nope") }
        assertTrue(bad.reason.isNotEmpty())

        // The hub removes the phone; the phone learns about it.
        hub.core.revoke(phone.state.get().local!!.info.id)
        val removed = phone.waitFor("removed") { it.peers.isEmpty() }
        assertNotNull(removed.removedNotice)

        phone.core.shutdown()
        hub.core.shutdown()
    }
}
