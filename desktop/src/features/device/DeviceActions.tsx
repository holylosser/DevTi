import { useState } from "react";
import type { PeerSnapshot } from "../../bridge";
import { useBridge } from "../../lib/bridge-context";
import { useStore } from "../../state/store";
import { Button } from "../../ui";

/** Find my device + send files, for one paired device. */
export function DeviceActions({ peer }: { peer: PeerSnapshot }) {
  const bridge = useBridge();
  const toast = useStore((st) => st.toast);
  const [ringing, setRinging] = useState(false);
  const offline = !peer.online;

  const ring = async () => {
    try {
      await bridge.ring(peer.info.id, !ringing);
      setRinging(!ringing);
      if (!ringing) toast(`Ring request sent to ${peer.info.name}.`, "ok");
    } catch (e) {
      toast(String(e instanceof Error ? e.message : e), "danger");
    }
  };

  const send = async () => {
    const paths = await bridge.pickFiles();
    if (!paths.length) return;
    try {
      await bridge.sendFiles(peer.info.id, paths);
    } catch (e) {
      toast(String(e instanceof Error ? e.message : e), "danger");
    }
  };

  return (
    <>
      <Button icon="signal" onPress={() => void ring()} isDisabled={offline}>
        {ringing ? "Stop ringing" : "Ring it"}
      </Button>
      <Button icon="folder" onPress={() => void send()}>
        Send files
      </Button>
    </>
  );
}
