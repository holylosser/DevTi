package app.devti.android.core.state

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/**
 * Pure state machine: (state, event JSON) → new state. No Android types, so it is
 * unit-tested on the JVM against the real Rust core (android/jvm-check).
 */
object Reducer {
    val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        coerceInputValues = true
    }

    /** Builds the initial state from the core's snapshot calls. */
    fun snapshot(localJson: String, peersJson: String, nearbyJson: String): CoreState =
        CoreState(
            ready = true,
            local = json.decodeFromString(LocalSnapshot.serializer(), localJson),
            peers = sortPeers(json.decodeFromString(kotlinx.serialization.builtins.ListSerializer(PeerSnapshot.serializer()), peersJson)),
            nearby = json.decodeFromString(kotlinx.serialization.builtins.ListSerializer(DiscoveredDevice.serializer()), nearbyJson),
        )

    fun apply(state: CoreState, eventJson: String): CoreState {
        val obj = runCatching { json.parseToJsonElement(eventJson).jsonObject }.getOrNull() ?: return state
        return when (obj.string("type")) {
            "local_changed" -> state.copy(local = obj.decode("local", LocalSnapshot.serializer()) ?: state.local)
            "peer_changed" -> {
                val peer = obj.decode("peer", PeerSnapshot.serializer()) ?: return state
                state.copy(
                    peers = sortPeers(state.peers.filterNot { it.info.id == peer.info.id } + peer),
                    nearby = state.nearby.filterNot { it.id == peer.info.id },
                )
            }
            "peer_removed" -> {
                val id = obj.string("id") ?: return state
                val gone = state.peers.firstOrNull { it.info.id == id }
                state.copy(
                    peers = state.peers.filterNot { it.info.id == id },
                    removedNotice = if (obj.string("reason") == "revoked_by_peer" && gone != null) {
                        "${gone.info.name} removed this phone from its devices."
                    } else {
                        state.removedNotice
                    },
                )
            }
            "device_discovered" -> {
                val dev = obj.decode("device", DiscoveredDevice.serializer()) ?: return state
                if (state.peers.any { it.info.id == dev.id }) state
                else state.copy(nearby = (state.nearby.filterNot { it.id == dev.id } + dev).sortedBy { it.name })
            }
            "clipboard_received" -> state.copy(clipboardRevision = state.clipboardRevision + 1)
            "device_lost" -> state.copy(nearby = state.nearby.filterNot { it.id == obj.string("id") })
            "pairing_started" -> obj.long("pairing_id")?.let { state.copy(pairing = PairingStage.Connecting(it)) } ?: state
            "pairing_verify" -> {
                val id = obj.long("pairing_id") ?: return state
                val peer = obj.decode("peer", DeviceInfo.serializer()) ?: return state
                state.copy(pairing = PairingStage.Verify(id, peer, obj.string("sas") ?: ""))
            }
            "pairing_completed" -> {
                val id = obj.long("pairing_id") ?: return state
                val peer = obj.decode("peer", PeerSnapshot.serializer()) ?: return state
                state.copy(
                    pairing = PairingStage.Done(id, peer),
                    peers = sortPeers(state.peers.filterNot { it.info.id == peer.info.id } + peer),
                )
            }
            "pairing_failed" -> {
                val id = obj.long("pairing_id") ?: return state
                state.copy(pairing = PairingStage.Failed(id, obj.string("reason") ?: "other", obj.string("message") ?: ""))
            }
            "paused" -> {
                val paused = obj["paused"]?.jsonPrimitive?.booleanOrNull ?: return state
                state.copy(local = state.local?.copy(paused = paused))
            }
            else -> state
        }
    }

    fun sortPeers(peers: List<PeerSnapshot>): List<PeerSnapshot> =
        peers.sortedWith(compareByDescending<PeerSnapshot> { it.online }.thenBy { it.pairedAt })

    private fun JsonObject.string(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull

    private fun JsonObject.long(key: String): Long? = this[key]?.jsonPrimitive?.longOrNull

    private fun <T> JsonObject.decode(key: String, s: kotlinx.serialization.KSerializer<T>): T? =
        this[key]?.let { runCatching { json.decodeFromJsonElement(s, it) }.getOrNull() }
}

/** Quick check before handing scanned text to the core, which validates it fully. */
fun looksLikePairingLink(text: String): Boolean = text.trim().startsWith("devti://pair?")

/** "482913" or "482 913" → "482 913". */
fun formatCode(code: String): String {
    val d = code.filter { it.isDigit() }
    return if (d.length == 6) "${d.substring(0, 3)} ${d.substring(3)}" else code
}
