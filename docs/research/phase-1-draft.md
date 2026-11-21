# Devti engineering decisions

**Phase 1 research draft · 22 November 2026**

The first build should prove one round trip. On a LAN with no internet, a Windows or Linux Hub displays an invitation. An Android 8 phone scans it. The owner confirms the same code on both screens. Both devices show an authenticated connection, reconnect after a restart, and stop communicating when the owner revokes the pairing.

For that build, use **Tauri 2 and React on the Hub, Kotlin and Jetpack Compose on Android, and a shared Rust core speaking TLS 1.3 over TCP**. Two findings shape the design. Android’s platform TLS 1.3 starts at API 29, three API levels above Devti’s minimum. Multicast DNS also exposes discovery traffic to the local link. The core therefore needs a TLS provider that runs on API 26, and onboarding needs a direct QR path that works with multicast disabled. These are research decisions. The acceptance tests at the end are pending. [Android TLS support][android-tls] · [Multicast DNS][mdns-rfc]

[Desktop and Android](#desktop-and-android) · [Discovery, transport and trust](#discovery-transport-and-trust) · [Later platform choices](#platform-choices-that-affect-later-features) · [License and cost](#license-and-operating-cost) · [Acceptance tests](#phase-1-acceptance-tests)

## Desktop and Android

### Desktop shell

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| Tauri 2 | A native Rust agent can share code with the Hub. The GUI uses the installed webview and can restrict which native operations each window may invoke. | WebView2 on Windows and WebKitGTK on Linux differ. Packaging, keyboard behavior and total memory need testing on both. | Select for Phase 1. | MIT or Apache-2.0. The Tauri project publishes v2 releases. [Project][tauri] · [Webviews][webviews] · [Capabilities][tauri-cap] |
| Electron | Bundled Chromium gives both desktops a predictable renderer and mature desktop integration. | Devti would ship Chromium and Node, update them with the app, and budget for their processes alongside the Rust agent. | Reconsider if webview differences block required interactions. | MIT for Electron, with separate bundled notices. Electron publishes a supported-release schedule. [Project][electron] · [Release schedule][electron-releases] |
| Flutter Desktop | One renderer supports a custom device map and animation system on both desktops. | Adds Dart and a native bridge to the Rust agent. The Android companion still needs platform integrations. | Reserve for a measured rendering problem. | BSD-3-Clause for Flutter. The SDK has a published stable channel. [Project][flutter] · [SDK archive][flutter-releases] |
| Qt 6 with Qt Quick | Mature native windows, graphics and desktop integration. | Adds a QML/C++ boundary and per-module distribution obligations. | Viable native alternative if the webview approach fails. | LGPLv3 or GPL options depend on the module. Qt maintains a 6.x line. [Qt licensing][qt-license] |

Run one **per-user background agent**. It owns peer connections, key access and the trust database. Closing the last window leaves the tray and agent available. Quitting Devti stops both. Expose a small, typed command interface over same-user local IPC. Load the GUI from packaged assets. Count the agent, GUI and webview processes together when measuring idle resource use.

### Desktop UI kit

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| React Aria Components | Unstyled controls include focus management, collections and keyboard and screen-reader drag and drop. These fit the device list and future display arrangement. | Devti owns the visual design, hit targets and integration tests. | Select with React, TypeScript and Vite. | Apache-2.0. Adobe publishes component releases. [Project][react-aria] · [Drag and drop][react-dnd] · [Releases][aria-releases] |
| Radix Primitives | Accessible dialogs, menus and popovers fit onboarding and tray settings. | The device map and accessible rearrangement require additional work. | Reserve for a specific missing primitive. | MIT. WorkOS maintains the project. [Radix][radix] |
| shadcn/ui | Components are copied into the app, making local styling and changes straightforward. | Devti must reconcile upstream accessibility fixes with its modified copies. The supplied styles also need reworking. | Prefer direct primitives for the first build. | MIT. The registry and CLI publish changes. [Project][shadcn] · [Changelog][shadcn-changes] |

The map and the accessible device list must use the same model. A keyboard user can select a device, inspect its permissions and revoke it from the list. A new Hub shows its own real device record and a pairing action. A missing battery reading displays “Unknown.”

### Animation

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| CSS transitions and Web Animations API | Connection, focus and layout transitions can run without an animation dependency. | Complex layout choreography needs explicit handling and testing in both webviews. | Select. Animate brief state changes and stop when idle. | Browser-provided APIs. W3C maintains the specification and webview vendors ship implementations. [Specification][waapi] · [Reduced motion][reduced-motion] |
| Motion for React | Layout animation and gestures simplify transitions between related device cards. | Adds runtime and bundle cost to interactions the first build can express in CSS. | Revisit for screen arrangement if it removes measurable complexity. | MIT for the core library. The project publishes releases. [Motion][motion] |
| Lottie-web | Exported vector sequences suit a fixed onboarding illustration. | Live device and battery states need application logic. Animation assets add download and rendering cost. | Reserve for an authored illustration with an accessible static state. | MIT. Airbnb hosts the 5.x release history. [Project][lottie] · [Releases][lottie-releases] |
| GSAP | Timelines and sequencing suit complex motion. | The standard license contains use restrictions that conflict with the open-source-only dependency requirement. | Exclude from the dependency set. | Webflow maintains GSAP under the GSAP Standard License. [License terms][gsap] |

### Android companion

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| Kotlin and Jetpack Compose | Direct access to Android permissions, network discovery, the camera and Keystore. | Requires a small native bridge to the shared core and separate desktop and mobile UI implementations. | Select. Set the minimum to API 26 and run the pairing flow there. | AndroidX Compose is Apache-2.0 and has a Google-maintained release train. [AndroidX][androidx] · [Compose releases][compose] |
| Flutter | Shares UI work if the desktop also uses Flutter. | Keystore signing, permission changes and future media sessions still need Android APIs across a plugin boundary. | Pass with the selected desktop shell. | BSD-3-Clause. The Flutter team maintains the Android target. [Flutter][flutter] |
| React Native | Shares React expertise and supports native modules. | Adds a JavaScript runtime and another bridge between the UI, OS services and Rust. | Pass for this native-service-heavy companion. | MIT. The project publishes its supported release lines. [Project][react-native] · [Versions][rn-versions] |

Keep camera frames on the phone. CameraX can feed a local ZXing-C++ decoder. The Hub can render QR codes with Nayuki’s MIT-licensed encoder. ZXing-C++ publishes Android bindings and releases under Apache-2.0. The original ZXing Java project describes itself as maintenance-only. [CameraX][camerax] · [ZXing-C++][zxingcpp] · [Java project status][zxingjava] · [QR encoder][qr-encoder]

## Discovery, transport and trust

### Finding a Hub

| Approach | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| mDNS and DNS-SD | Devices can discover `_devti._tcp` without an account or a central directory. Android provides a native service browser. | Guest networks, multicast filtering and access-point isolation can interrupt discovery. Neighboring hosts can observe advertisements. | Select for browsing. Advertise a temporary instance label and protocol major version. | DNS-SD is an IETF specification. Desktop `mdns-sd` is MIT or Apache-2.0 with a published 0.22 line. Android maintains `NsdManager`. [DNS-SD][dnssd] · [Rust library][mdns-lib] · [Android NSD][android-nsd] |
| Direct endpoint in a QR invitation | Operates on a routable LAN even when multicast is blocked. The invitation can carry a pinned Hub identity. | Requires a current address and an out-of-band handoff. Client isolation can also block direct traffic. | Select as a first-class path. Offer explicit import of the same full invitation for a peer without a camera. | MIT encoder and Apache-2.0 decoder. The decoder has published 3.x releases. [Nayuki][qr-encoder] · [ZXing-C++ release][zxing-release] |
| BLE advertisements for rendezvous | Can indicate that a nearby Hub is present before joining its network. | Adds radio permissions and hardware dependencies. A BLE sighting does not establish a routable LAN path. | Defer to the proximity phase. | Android exposes a maintained platform API. The cross-platform `btleplug` library uses BSD-3-Clause with additional upstream notices. [Android BLE][android-ble] · [btleplug][btleplug] |

Offer a QR-only discovery setting. Omit pairing secrets, pins, friendly names, battery and permissions from Devti’s advertisements. The advertised address and OS hostname can still identify a host on the local link. A discovery result is a candidate address. Show “Online” only after the peer authenticates and responds. If endpoints cannot reach each other, explain that both devices need a local network permitting peer traffic. On Android 17 with target SDK 37+, evaluate the system NSD picker for a single Hub and request local-network permission where direct network access requires it. Handle denial in onboarding. [Multicast DNS][mdns-rfc] · [Android local-network permissions][android-lan]

### Control and data transport

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| TLS 1.3 over TCP with rustls | One encrypted stream is easy to diagnose and sufficient for discovery follow-up, pairing and device presence. Rust can share the implementation with Android. | TCP orders all bytes on a connection. A future large file on the control stream would delay control messages. | Select. Give bulk transfer a separate authenticated connection when introduced. | rustls offers Apache-2.0, ISC or MIT and maintains a 0.23 line. [rustls][rustls] · [TLS 1.3][tls13] |
| QUIC with Quinn | Independent streams can keep control responsive during transfers. QUIC supports connection migration. | Adds UDP reachability and tuning to the first support matrix. | Revisit when loss and concurrent-transfer measurements justify it. | Quinn is MIT or Apache-2.0, with a published 0.11 line. [Quinn][quinn] · [QUIC][quic-rfc] |
| WebRTC data channels | Reliable or partially reliable channels can share a peer connection with media. | ICE, DTLS, SCTP and signaling add complexity before Devti can even list a paired phone. | Reserve for latency-sensitive interactions attached to a media session. | IETF data-channel specification. Native libwebrtc uses a BSD-style license plus third-party notices and has an active upstream tree. [Data channels][rtc-data] · [libwebrtc][webrtc-src] |

Require TLS 1.3, verify the peer on every connection, and disable 0-RTT application data. Retry a dropped connection with bounded exponential backoff and jitter. An explicit revoke or pause cancels active sessions and outstanding reconnects. Resume through a new authenticated session.

### Pairing and key custody

| Approach | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| QR-pinned TLS enrollment, followed by mutual TLS | The QR authenticates the Hub before the phone shares its invitation. Returning peers use the same transport and pinned device keys. | First enrollment and the matching-code display need a carefully reviewed state machine. | Select. Require confirmation on both devices. | Uses the selected rustls implementation and the published TLS exporter mechanism. [rustls][rustls] · [Channel binding][tls-binding] |
| Noise XX with out-of-band verification | A compact handshake gives each peer a static identity and an encrypted channel. | Devti would own more framing, session integration and mobile key-provider work alongside the TLS-based parts of the app. | Reserve if a required device cannot support the selected identity flow. | `snow` is MIT or Apache-2.0. Upstream publishes a 0.10 line. [Noise framework][noise] · [snow][snow] |
| Password-authenticated enrollment with SPAKE2+ | A human can type a short setup code on a device without a camera. Matter offers a deployed commissioning design to study. | Adds a separate enrollment protocol, retry policy, and a binding from that session to permanent device keys. | Revisit for short-code-only onboarding. | RFC 9383 is an informational protocol document. The CSA’s Matter implementation is maintained under Apache-2.0. [SPAKE2+][spake] · [Matter][matter] |

1. Opening **Pair a device** creates a single-use invitation with a short expiry, a reachable endpoint, the Hub public-key fingerprint and a cryptographically random secret of at least 128 bits. Bound the number and duration of pending enrollment attempts. The ordinary peer listener accepts only enrolled identities.
2. The phone verifies the Hub pin and presents the secret inside the encrypted enrollment session. Both endpoints prove possession of their device keys. Each displays a short code bound to this session and these identities. The owner compares the displays and approves on both. The code alone cannot enroll a device. [TLS 1.3][tls13] · [TLS exporter][tls-binding]
3. The agent atomically consumes the invitation and records the confirmed peer pin and initial grants. Enrollment retries reuse a transaction identifier. After an interrupted confirmation, the pair either finishes that same approved transaction or expires with access denied. A new peer starts with presence access and requests each sharing permission separately.
4. Keep pins across IP address, network and display-name changes. A changed key requires fresh pairing. Revocation closes live sessions, invalidates resumable state and rejects the old pin after restart. Redact invitation contents, codes, key material and session tokens from diagnostics.

| Platform | Phase 1 key storage and behavior |
| --- | --- |
| Windows | Use the signed-in user’s Credential Manager for the identity secret. A per-user agent can retrieve it after login. Evaluate a CNG key handle if Devti needs a non-exportable desktop signer. [Credential Manager][win-cred] · [CNG storage providers][win-cng] |
| Linux | Store the software identity secret through libsecret in the user’s Secret Service collection. A locked or absent collection presents an unlock or setup action and keeps pairing disabled. [libsecret][libsecret] |
| Android | Generate a P-256 signing key in Android Keystore and expose only the signing operation to rustls. The shared TLS engine avoids dependence on API 29’s platform TLS provider. Hardware backing varies by device. An API 26 handshake with the actual key is the deciding test. [Keystore][keystore] · [rustls signer][signer] · [Android TLS versions][android-tls] |
| iOS, later | Use Keychain in the source-built companion. Verify access groups and extension entitlements on a real device before promising background or broadcast behavior. [Keychain][keychain] · [Apple developer accounts][apple-account] |

### Wire format and device model

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| Protocol Buffers with prost | A language-neutral schema and numbered fields allow new clients to read older messages. | Requires generated types, a pinned build toolchain and discipline when retiring fields. | Select for peer messages. Commit compatibility fixtures with the schema. | prost is Apache-2.0 and publishes a 0.14 line. Google maintains the protobuf language guide. [prost][prost] · [Schema evolution][protobuf] |
| JSON with serde_json | Easy to inspect, edit and contribute. It suits human-authored accessory profiles. | Field evolution and bounds rely more heavily on Devti’s validation. Encoded control frames are typically larger. | Use for declarative profiles in their phase. | MIT or Apache-2.0. Serde maintains the 1.x implementation. [serde_json][serde] |
| Cap’n Proto | Typed schemas and direct access to encoded fields reduce some decoding work. | Adds a distinct tooling and memory model without measured benefit for short presence messages. | Reserve for evidence of a serialization bottleneck. | Rust implementation is MIT and has published releases. [Cap’n Proto for Rust][capnp] |

The shared envelope carries a protocol major, message type and request ID. Bound control-frame size before allocating, reserve retired field numbers and reject incompatible majors with a readable reason. Device records distinguish identity, advertised capability, local permission and session availability. Store the last authenticated contact separately from current presence. Battery is optional and carries an observation time. Renaming a Hub must preserve its peer relationships. [Proto3 field rules][protobuf]

### Local state

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| SQLite with rusqlite | Pairing, grants and local audit entries can commit together. Indexed history and schema migration fit later phases. | Requires migrations and explicit handling of a busy database. | Select. The agent owns writes. Store private identity material in the OS store. | SQLite is public domain with a published release history. rusqlite is MIT. [SQLite transactions][sqlite-tx] · [Releases][sqlite-releases] · [Binding][rusqlite] |
| Transactional Rust key-value store, redb | Embedded ACID storage with a Rust API. | More custom indexing and migration logic for device and activity queries. | Reserve if SQLite becomes a packaging constraint. | MIT or Apache-2.0. The project publishes versioned crates. [redb][redb] |
| Versioned JSON file with atomic replacement | Simple for a few preferences. | Cross-record transactions, concurrent writers and recovery become Devti’s responsibility. | Limit to non-sensitive preferences if needed. | serde_json is MIT or Apache-2.0. Devti would maintain the atomic-write logic. [serde_json][serde] |

## Platform choices that affect later features

### Screen streaming

| Option | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| GStreamer with webrtcbin | Composes capture, encoding and WebRTC. Hardware or software encoders can be selected at runtime. | Windows packaging and installed Linux plugins differ. Each shipped codec and binary needs its own review. | First desktop candidate for the screen phase. | GStreamer maintains a stable 1.x series under LGPL. Plugins and linked codecs have their own terms. [webrtcbin][gst-webrtc] · [Licensing][gst-license] · [Releases][gst-releases] |
| Native libwebrtc | Integrated congestion control, media processing and platform clients. | A large C++ build and release integration would sit alongside the Rust core. | Compare if GStreamer misses latency, interoperability or packaging targets. | BSD-style core license with separate bundled notices. Upstream maintains a rolling source tree. [License][webrtc-license] · [Source][webrtc-src] |
| FFmpeg with libdatachannel | Separates capture and codec choice from WebRTC media transport. | Devti must integrate timing, packetization and feedback with the encoders. | Keep as the modular alternative. | FFmpeg is generally LGPL-2.1-or-later, with configuration-dependent terms. libdatachannel is MPL-2.0. Both have maintained upstream projects. [FFmpeg licensing][ffmpeg] · [libdatachannel][libdatachannel] |

Start the interoperability test with VP8 software encoding. Measure a hardware encoder where available, and assess H.264 distribution terms separately for each build. Authenticate WebRTC fingerprints through the already paired control channel. Use local ICE candidates for the LAN test. Add a user-visible sharing indicator and stop action. Measure capture-to-display latency independently of network round-trip time. [WebRTC video codecs][rtc-video] · [WebRTC security][rtc-security]

| Display path | Use and limit | Evidence required before offering it |
| --- | --- | --- |
| Windows capture | Prefer the Windows Graphics Capture picker for one window or a display. Desktop Duplication is an alternative for whole-output capture. Either can feed a mirror. | Demonstrate window-only and monitor-only sharing, cancellation, resize and stop. [Windows capture][win-capture] · [Desktop Duplication][duplication] |
| Windows extended display | VirtualDrivers publishes an MIT IddCx project. Its 25.7.23 beta release explicitly lists a signed display driver. Microsoft requires an EV-associated account for Devti’s own attestation submission. | Pin an exact upstream package, verify its catalog signer and redistribution terms, then install, extend, disconnect and uninstall on clean Windows 11 with Secure Boot enabled. Offer mirror and window casting until this passes. [Upstream release][vdd-release] · [Source][vdd] · [Microsoft signing][driver-signing] |
| Wayland | Use a ScreenCast portal and PipeWire. A compositor may advertise the optional virtual-monitor source through `AvailableSourceTypes`. Window, monitor and virtual sources are distinct. | Probe the running backend and show only the modes it grants. Test cancellation, an absent virtual source and two real simultaneous outputs. [ScreenCast portal][portal-screen] |
| X11 and optional Linux virtual output | Use X11 capture, with Composite for a redirected window where supported. EVDI provides an optional kernel module and userspace interface. Module compatibility, Secure Boot and access control affect deployment. | Test capture separately from a real, separately addressable desktop output. Verify an exact kernel/distro combination before documenting extension. EVDI lists GPLv2 for the module and LGPL-2.1-or-later for the library. [X Composite][xcomp] · [EVDI][evdi] |

### Input control

| Approach | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| Wayland RemoteDesktop portal with libei | The compositor mediates the user’s grant and supplies an input session. | Backend support varies. A denied or ended session stops control. | Prefer on Wayland. Probe portal version and capabilities. | Portal and libei have upstream APIs. libei is MIT and is packaged by Fedora. [RemoteDesktop][portal-input] · [libei][libei] · [Package][libei-license] |
| Windows SendInput | Works in the signed-in user’s desktop session through a native API. | Windows integrity rules limit which windows can receive input. | Select for user-approved sessions on Windows. Surface an unavailable action when the OS blocks it. | System API. Rust bindings are MIT or Apache-2.0 and maintained by Microsoft. [SendInput][sendinput] · [Bindings][windows-rs] |
| X11 XTEST | Established path for pointer and key events in an X11 session. | The extension grants broad session access and depends on X11. | Select for X11 with Devti’s own visible session approval and stop control. | X.Org library with permissive X11, MIT and HPND-family notices. Distributions maintain packages. [XTEST][xtest] · [Package notices][xtest-license] |
| Linux uinput | Kernel virtual devices cover a broad range of input events. | Access is highly privileged for a desktop application and is a poor fit for portal consent. | Exclude from the default installation. | Kernel-maintained ABI. The userspace header specifies GPL-2.0-or-later with the Linux syscall exception. [uinput][uinput] · [Header license][uinput-license] |

On Android, keep screen sharing and user-enabled input control as independent grants. Android’s MediaProjection requires user consent for capture. On iOS, plan around a user-started broadcast extension and foreground companion features. The iOS capability panel should offer viewing, files and remote/display use within the sandbox and mark system-wide input control unavailable. [MediaProjection][projection] · [ReplayKit broadcast handler][replaykit] · [Apple sandbox][ios-sandbox]

### Bluetooth and accessory profiles

| Approach | Benefit for Devti | Cost or limitation | Verdict | License and maintenance |
| --- | --- | --- | --- | --- |
| Native adapters for BlueZ D-Bus and Windows Bluetooth APIs | Give the Hub access to the profiles and properties its host exposes, with separate paths for BLE GATT and Classic services. | Requires two backends. Some vendor channels may already be owned by the OS or inaccessible. | Select as the authoritative capability source per host. | BlueZ is a maintained system daemon with GPL-family and component-specific notices. Microsoft maintains WinRT APIs and permissively licensed Rust bindings. [BlueZ][bluez] · [Windows Bluetooth][win-bt] · [Bindings][windows-rs] |
| btleplug | A common Rust interface for BLE central operations and GATT characteristics. | BLE-only scope leaves Classic audio, headset routing and many vendor protocols uncovered. | Use selectively where it removes duplicated BLE code. | BSD-3-Clause with additional inherited notices. The project maintains multiple platform backends. [btleplug][btleplug] |
| Qt Bluetooth | Exposes Classic and BLE APIs with a documented per-platform backend matrix. | Adds Qt to the Tauri stack, and the available functions still depend on the backend. | Reserve if a required native backend is impractical. | Qt 6 maintains the module. Open-source options include LGPLv3 and GPLv2. [Module and licensing][qt-bt] |

Generate controls from observed services and a profile matched to the device’s model, firmware and transport. A vendor name alone is insufficient for a write. For an unknown headset, show battery only if the host reports it, keep unsupported controls absent, and offer a previewable local capability export that strips identifiers. Treat audio routing separately from GATT. PipeWire or Windows audio endpoints must actually expose the microphone or playback path. Test each direction and disclose measured relay latency. [BlueZ battery interface][bluez-battery] · [PipeWire media graph][pipewire] · [Windows audio endpoints][wasapi]

Use LibrePods, GalaxyBudsClient, OpenFreebuds and Gadgetbridge to identify model-specific test cases. SonyHeadphonesClient was archived in July 2025 and is historical evidence. Track Apple/Beats, Sony, Samsung, Huawei, Nothing and JBL support per model and firmware. Enable vendor controls after a hardware round trip confirms both the write and the reported state. [LibrePods][librepods] · [GalaxyBudsClient][buds] · [OpenFreebuds][freebuds] · [Gadgetbridge][gadgetbridge] · [Sony archive][sony]

## License and operating cost

Propose **GPL-3.0-or-later for Devti-authored code**. The Apache Software Foundation confirms that Apache-2.0 components may be included in a GPLv3 work. Preserve upstream notices, pin versions and review the full dependency tree and packaged binaries before distribution. Record codec plugins and drivers individually. Use vendor operating-system APIs as platform prerequisites and keep separately bundled application dependencies open source. The Windows build must identify its WebView2 prerequisite. [GPL license][gpl] · [Apache compatibility][apache-gpl] · [GStreamer licensing][gst-license] · [Tauri webviews][webviews]

The proposed desktop toolchain uses MIT-licensed React and Vite and Apache-2.0-licensed TypeScript. Keep their notices with the appropriate source or distributed assets. Record exact package versions and transitive licenses when the first dependency lockfiles exist. [React][react] · [Vite][vite] · [TypeScript][typescript]

The runtime needs no account, public DNS directory, STUN server, paid SDK or credit card for local pairing. Build the Android APK for sideloading and keep the later iOS release source-buildable in Xcode. When publishing resumes, use standard GitHub-hosted runners for the public repository, limit artifact retention and keep paid runners disabled. [GitHub Actions billing][actions-billing] · [Apple account options][apple-account]

KDE Connect provides a precedent for dividing paired-device features into plugins. LocalSend provides a reference for discovery and receiver-approved transfers. Study scrcpy for separation of Android capture and playback, Deskflow for input handoff, and Weylus for tablet interaction. spacedesk is a product reference with proprietary license terms. Implement Devti’s protocol and document any third-party code separately. [KDE Connect][kdeconnect] · [LocalSend protocol][localsend] · [scrcpy architecture][scrcpy] · [Deskflow][deskflow] · [Weylus][weylus] · [spacedesk terms][spacedesk]

## Phase 1 acceptance tests

Run these before marking Foundation complete or starting Continuity. Record actual hardware, OS, versions, pass/fail results and remaining limitations in the phase QA record.

| Test | Pass condition |
| --- | --- |
| Offline first pairing | With WAN disconnected, pair an API 26 handset with a Windows 11 Hub, Ubuntu on Wayland and Mint on X11. Display matching codes, confirm on both, and show real presence. Repeat with multicast blocked using direct QR. Repeat on current Android, including denied/regranted LAN and camera access. Record a Windows 10 compatibility run before listing that platform as verified. |
| Identity and protocol | Observe TLS 1.3 on API 26 with a Keystore-backed signing operation. Reject a wrong Hub pin, changed peer key, mismatched confirmation, expired or reused invitation, and two simultaneous claims on one invitation. A losing or interrupted enrollment receives no device data. |
| Reconnect and revocation | Restart both apps, change the phone’s IP and suspend the Hub. A trusted peer reconnects once. Revoke during a live connection, restart again, and confirm that the old identity requires fresh pairing. With the OS keyring locked, pairing stays disabled and the GUI explains how to recover. |
| Usability | Complete onboarding on Windows and Linux using a keyboard. Test focus, a screen reader, reduced motion, the tray, relaunch and the genuine empty-device state. The optional accessory step can be skipped. Network denial and client isolation produce distinct, actionable messages where distinguishable. |
| Automated checks | Unit-test the device model and permissions, fuzz malformed, oversized and truncated control frames, exercise enrollment and restart using two real protocol endpoints, and run onboarding and dashboard UI tests. Target at least 80% core line coverage. Build Windows, Linux and an Android APK with recorded dependency versions. |
| Idle and motion budgets | After a 10-minute warm-up with one idle peer, sample CPU for 5 minutes and sum the Hub’s process tree. Target mean CPU below 2% of one logical core and memory below 250 MB. Report private working set on Windows and proportional set size on Linux separately. Check missed frames during a 60 Hz connection transition. |

Screen latency belongs to the screen phase. On a documented LAN, report capture-to-display p50 and p95 at a stated resolution, frame rate and codec. Compare against the brief’s 50 ms target. A working cast, low ping, successful compilation or published upstream feature does not establish that result.

[android-tls]: https://developer.android.com/reference/javax/net/ssl/SSLSocket
[mdns-rfc]: https://www.rfc-editor.org/rfc/rfc6762
[tauri]: https://github.com/tauri-apps/tauri
[webviews]: https://v2.tauri.app/reference/webview-versions/
[tauri-cap]: https://v2.tauri.app/security/capabilities/
[electron]: https://github.com/electron/electron
[electron-releases]: https://www.electronjs.org/docs/latest/tutorial/electron-timelines
[flutter]: https://github.com/flutter/flutter
[flutter-releases]: https://docs.flutter.dev/install/archive
[qt-license]: https://doc.qt.io/qt-6/licensing.html
[react-aria]: https://github.com/adobe/react-spectrum
[react-dnd]: https://react-aria.adobe.com/dnd
[aria-releases]: https://react-aria.adobe.com/releases/
[radix]: https://github.com/radix-ui/primitives
[shadcn]: https://github.com/shadcn-ui/ui
[shadcn-changes]: https://ui.shadcn.com/docs/changelog
[waapi]: https://www.w3.org/TR/web-animations-1/
[reduced-motion]: https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/prefers-reduced-motion
[motion]: https://github.com/motiondivision/motion
[lottie]: https://github.com/airbnb/lottie-web
[lottie-releases]: https://github.com/airbnb/lottie-web/releases
[gsap]: https://gsap.com/community/standard-license/
[androidx]: https://github.com/androidx/androidx
[compose]: https://developer.android.com/jetpack/androidx/releases/compose-ui
[react-native]: https://github.com/react/react-native
[rn-versions]: https://reactnative.dev/versions
[camerax]: https://developer.android.com/media/camera/camerax
[zxingcpp]: https://github.com/zxing-cpp/zxing-cpp
[zxingjava]: https://github.com/zxing/zxing
[qr-encoder]: https://www.nayuki.io/page/qr-code-generator-library
[dnssd]: https://www.rfc-editor.org/rfc/rfc6763
[mdns-lib]: https://github.com/keepsimple1/mdns-sd
[android-nsd]: https://developer.android.com/develop/connectivity/wifi/use-nsd
[zxing-release]: https://github.com/zxing-cpp/zxing-cpp/releases/tag/v3.1.1
[android-ble]: https://developer.android.com/develop/connectivity/bluetooth/ble/ble-overview
[btleplug]: https://github.com/deviceplug/btleplug
[android-lan]: https://developer.android.com/privacy-and-security/local-network-permission
[rustls]: https://github.com/rustls/rustls
[tls13]: https://www.rfc-editor.org/rfc/rfc8446
[quinn]: https://github.com/quinn-rs/quinn
[quic-rfc]: https://www.rfc-editor.org/rfc/rfc9000
[rtc-data]: https://www.rfc-editor.org/rfc/rfc8831
[webrtc-src]: https://webrtc.googlesource.com/src/+/refs/heads/main/
[tls-binding]: https://www.rfc-editor.org/rfc/rfc9266
[noise]: https://noiseprotocol.org/noise.html
[snow]: https://github.com/mcginty/snow
[spake]: https://www.rfc-editor.org/rfc/rfc9383
[matter]: https://github.com/project-chip/connectedhomeip
[win-cred]: https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew
[win-cng]: https://learn.microsoft.com/en-us/windows/win32/seccertenroll/cng-key-storage-providers
[libsecret]: https://gnome.pages.gitlab.gnome.org/libsecret/
[keystore]: https://developer.android.com/privacy-and-security/keystore
[signer]: https://docs.rs/rustls/latest/rustls/sign/trait.SigningKey.html
[keychain]: https://developer.apple.com/documentation/security/keychain-services
[apple-account]: https://developer.apple.com/help/account/basics/about-your-developer-account
[prost]: https://github.com/tokio-rs/prost
[protobuf]: https://protobuf.dev/programming-guides/proto3/
[serde]: https://github.com/serde-rs/json
[capnp]: https://github.com/capnproto/capnproto-rust
[sqlite-tx]: https://www.sqlite.org/transactional.html
[sqlite-releases]: https://sqlite.org/chronology.html
[rusqlite]: https://github.com/rusqlite/rusqlite
[redb]: https://github.com/cberner/redb
[gst-webrtc]: https://gstreamer.freedesktop.org/documentation/webrtc/
[gst-license]: https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html
[gst-releases]: https://gstreamer.freedesktop.org/releases/
[webrtc-license]: https://webrtc.googlesource.com/src/+/refs/heads/main/LICENSE
[ffmpeg]: https://ffmpeg.org/legal.html
[libdatachannel]: https://github.com/paullouisageneau/libdatachannel
[rtc-video]: https://www.rfc-editor.org/rfc/rfc7742
[rtc-security]: https://www.rfc-editor.org/rfc/rfc8827
[win-capture]: https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture
[duplication]: https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api
[vdd-release]: https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/tag/25.7.23
[vdd]: https://github.com/VirtualDrivers/Virtual-Display-Driver
[driver-signing]: https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-reqs
[portal-screen]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
[xcomp]: https://www.x.org/releases/current/doc/compositeproto/compositeproto.txt
[evdi]: https://github.com/DisplayLink/evdi
[portal-input]: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html
[libei]: https://libinput.pages.freedesktop.org/libei/
[libei-license]: https://packages.fedoraproject.org/pkgs/libei/libei/
[sendinput]: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
[windows-rs]: https://github.com/microsoft/windows-rs
[xtest]: https://www.x.org/releases/current/doc/libXtst/xtestlib.html
[xtest-license]: https://packages.fedoraproject.org/pkgs/libXtst/libXtst/
[uinput]: https://docs.kernel.org/input/uinput.html
[uinput-license]: https://github.com/torvalds/linux/blob/master/include/uapi/linux/uinput.h
[projection]: https://developer.android.com/media/grow/media-projection
[replaykit]: https://developer.apple.com/documentation/replaykit/rpbroadcastsamplehandler
[ios-sandbox]: https://support.apple.com/guide/security/security-of-runtime-process-sec15bfe098e/web
[bluez]: https://github.com/bluez/bluez
[win-bt]: https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/bluetooth
[qt-bt]: https://doc.qt.io/qt-6/qtbluetooth-index.html
[bluez-battery]: https://manpages.debian.org/unstable/bluez/org.bluez.Battery.5.en.html
[pipewire]: https://docs.pipewire.org/page_objects_design.html
[wasapi]: https://learn.microsoft.com/en-us/windows/win32/coreaudio/wasapi
[librepods]: https://github.com/librepods-org/librepods
[buds]: https://github.com/timschneeb/GalaxyBudsClient
[freebuds]: https://github.com/melianmiko/OpenFreebuds
[gadgetbridge]: https://gadgetbridge.org/
[sony]: https://github.com/Plutoberth/SonyHeadphonesClient
[gpl]: https://spdx.org/licenses/GPL-3.0-or-later.html
[apache-gpl]: https://www.apache.org/licenses/GPL-compatibility
[react]: https://github.com/react/react
[vite]: https://github.com/vitejs/vite
[typescript]: https://github.com/microsoft/TypeScript
[actions-billing]: https://docs.github.com/en/billing/concepts/product-billing/github-actions
[kdeconnect]: https://github.com/KDE/kdeconnect-kde
[localsend]: https://github.com/localsend/protocol
[scrcpy]: https://github.com/Genymobile/scrcpy/blob/master/doc/develop.md
[deskflow]: https://github.com/deskflow/deskflow
[weylus]: https://github.com/H-M-H/Weylus
[spacedesk]: https://www.spacedesk.net/licensing/
