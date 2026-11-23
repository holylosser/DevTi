package app.devti.android

import app.devti.android.core.state.CoreState
import app.devti.android.core.state.PairingStage
import app.devti.android.core.state.Reducer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ReducerTest {
    private val peer = """{"info":{"id":"AAAAAAAAAAAAAAAA","name":"Studio Laptop","kind":"laptop","platform":"linux",
        "os_version":"Linux","model":null,"app_version":"0.1.0","capabilities":["battery"]},
        "fingerprint":"ab","role":"trusted","permissions":{"screen_view":true,"control":true,"files":true,
        "clipboard":true,"notifications":true,"commands":true,"bluetooth":true},"paired_at":1,"last_seen":null,
        "online":true,"connection":null,"status":{"battery":{"percent":70,"charging":false}}}"""

    @Test
    fun pairingStagesFlow() {
        var s = CoreState(ready = true)
        s = Reducer.apply(s, """{"type":"pairing_started","pairing_id":5,"role":"joiner","peer":null}""")
        assertTrue(s.pairing is PairingStage.Connecting)
        s = Reducer.apply(
            s,
            """{"type":"pairing_verify","pairing_id":5,"role":"joiner","peer":{"id":"AAAAAAAAAAAAAAAA","name":"Studio Laptop"},"sas":"123 456"}""",
        )
        assertEquals("123 456", (s.pairing as PairingStage.Verify).sas)
        s = Reducer.apply(s, """{"type":"pairing_completed","pairing_id":5,"peer":$peer}""")
        assertTrue(s.pairing is PairingStage.Done)
        assertEquals("Studio Laptop", s.hub?.info?.name)
        assertEquals(70, s.hub?.status?.battery?.percent)
    }

    @Test
    fun removalByHubLeavesANotice() {
        var s = Reducer.apply(CoreState(ready = true), """{"type":"peer_changed","peer":$peer}""")
        assertNull(s.removedNotice)
        s = Reducer.apply(s, """{"type":"peer_removed","id":"AAAAAAAAAAAAAAAA","reason":"revoked_by_peer"}""")
        assertTrue(s.peers.isEmpty())
        assertNotNull(s.removedNotice)
    }

    @Test
    fun activeRingIsScopedToAnAllowedConnectedPeer() {
        val paired = Reducer.apply(CoreState(), """{"type":"peer_changed","peer":$peer}""")
        val start = """{"type":"ring_requested","from":"AAAAAAAAAAAAAAAA","on":true}"""
        val active = Reducer.apply(paired, start)
        assertEquals("AAAAAAAAAAAAAAAA", active.ringingPeerId)
        assertEquals(active, Reducer.apply(active, """{"type":"ring_requested","from":"OTHER","on":false}"""))
        assertEquals(active, Reducer.apply(active, start))
        assertEquals(paired, Reducer.apply(paired, """{"type":"ring_requested","from":"OTHER","on":true}"""))
        assertNull(Reducer.apply(active, """{"type":"paused","paused":true}""").ringingPeerId)
        assertNull(Reducer.apply(active, """{"type":"peer_removed","id":"AAAAAAAAAAAAAAAA"}""").ringingPeerId)
        for (changedPeer in listOf(peer.replace("\"online\":true", "\"online\":false"),
                                   peer.replace("\"commands\":true", "\"commands\":false"))) {
            val stopped = Reducer.apply(active, """{"type":"peer_changed","peer":$changedPeer}""")
            assertNull(stopped.ringingPeerId)
            assertNull(Reducer.apply(stopped, start).ringingPeerId)
        }
    }

    @Test
    fun unknownAndBrokenEventsAreIgnored() {
        val s = CoreState(ready = true)
        assertEquals(s, Reducer.apply(s, "{"))
        assertEquals(s, Reducer.apply(s, """{"type":"hologram"}"""))
    }
}
