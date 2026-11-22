<p align="center">
  <img src="docs/media/devti-icon.png" width="112" alt="Devti logo: a glass prism lit in three colors">
</p>

<h1 align="center">Devti</h1>

<p align="center"><strong>All your devices, one living system.</strong><br>
Free, open-source, local-first. Your laptops, phones, tablets and Bluetooth accessories, connected straight to each other — encrypted, private, no cloud.</p>

<p align="center">
  <img src="docs/media/ecosystem.png" width="860" alt="The Devti ecosystem map: a prism in the middle for this computer, four devices around it joined by colored threads of light">
</p>

---

## Where Devti is today

Phase 1 delivers the secure core, Hub and Android pairing. Its automated suite passes on the
recorded baseline. Physical-device acceptance is still open. Phase 2 is in development across
the Hub and Android. See the [QA log](docs/QA.md) for test results and remaining checks.

| Phase | Scope | Status |
|---|---|---|
| 1. Foundation | Core protocol, encryption, discovery, pairing, desktop Hub, onboarding, dashboard, Android pairing | Hardware verification pending |
| 2. Continuity | Clipboard, file transfer, drag and drop, notifications, find my device, battery dashboard | In progress |
| 3. Control | Phone → laptop remote, laptop → Android mirror and control, shortcuts deck, local API + CLI | Planned |
| 4. Screens | Mirror, single-window cast, extended displays, multi-screen layout, one keyboard and mouse | Planned |
| 5. Bluetooth | Accessory manager, automatic feature detection, device profiles, audio relay | Planned |
| 6. Communication + automation | SMS, calls, rules engine, proximity, handoff, webcam and mic sharing | Planned |
| 7. Reach + extensibility | Remote access, plugin SDK, shared storage browser | Planned |
| 8. iOS + release | iPhone and iPad app, final polish, v1.0.0 | Planned |

## What works now

- **Pair in seconds.** The Hub shows a QR code; scan it with the Devti app on your phone. Both
  screens then show the same six digits — tap "match" on each and you're paired. Computers pair
  with each other using a 6-digit code.
- **Encrypted, always.** Every connection is TLS 1.3 over QUIC with both sides proving who they
  are. There is no unencrypted fallback anywhere in the code.
- **Reconnects by itself.** Paired devices find each other on your network and reconnect after
  Wi-Fi drops, sleep or a restart.
- **Live ecosystem map.** Every device is a point of light in its own color; a thread of light
  means it's connected. Battery and connection quality update live.
- **You stay in control.** Choose exactly what each device may do (screen, control, files,
  clipboard, notifications, commands, Bluetooth), read the activity log, or pause the whole
  ecosystem in one click.
- **Accessories detected.** The setup wizard shows nearby Bluetooth accessories live (names,
  makers, signal). Controlling them arrives in Phase 5.

<p align="center">
  <img src="docs/media/pair-qr.png" width="49%" alt="Pairing sheet showing a QR code and three steps">
  <img src="docs/media/pair-verify.png" width="49%" alt="Verification step showing six large digits and a 'The codes match' button">
</p>

## What works where

| | Windows 10/11 | Linux X11 | Linux Wayland | Android 8+ | iPhone / iPad |
|---|:-:|:-:|:-:|:-:|:-:|
| Pair with QR code or 6-digit code | ✅ | ✅ | ✅ | ✅ | Phase 8 |
| Encrypted automatic reconnection | ✅ | ✅ | ✅ | ✅ | Phase 8 |
| Live battery level | ✅ (laptops) | ✅ (laptops) | ✅ (laptops) | ✅ | Phase 8 |
| Per-device permissions, pause, remove | ✅ | ✅ | ✅ | ✅ | Phase 8 |
| Activity log | ✅ | ✅ | ✅ | via Hub | Phase 8 |
| Bluetooth accessory detection | ✅ | ✅ (BlueZ) | ✅ (BlueZ) | — | — |

**iPhone and iPad honesty box:** Apple doesn't allow any app to remote-control an iPhone, read SMS
or calls, or mirror other apps' notifications. When the iOS app arrives it will do everything Apple
allows: act as a screen, remote and shortcuts deck, share files and the clipboard while open, and
receive notifications from your Hub. Building it needs a Mac with Xcode; App Store distribution
needs a paid Apple account, so it will be a build-it-yourself app.

---

## Install

> Pre-built installers are produced by every CI run (see the latest **Actions** run → *Artifacts*).
> The first tagged release comes with Phase 8; until then use the CI artifacts or build from source.

### Windows 10 / 11
1. Download `devti-desktop-Windows` and unzip it.
2. Run `Devti_x.y.z_x64-setup.exe` (or the `.msi`).
3. **"Windows protected your PC"?** Devti isn't signed with a paid certificate yet. Click
   **More info → Run anyway**.
4. When Windows Firewall asks, allow Devti on **private networks**.

### Linux (Ubuntu, Mint, Fedora, Arch…)
Download `devti-desktop-Linux` and pick your format:

- **Debian, Ubuntu, Mint:** `sudo apt install ./Devti_*_amd64.deb`
- **Fedora, openSUSE:** `sudo dnf install ./Devti-*.x86_64.rpm`
- **Any distribution (AppImage):** `chmod +x Devti_*.AppImage` then run it.
- **Arch:** use the AppImage, or build from source (below).

Notes: the tray icon needs `libayatana-appindicator` (installed by the .deb/.rpm). If your firewall
is on, allow UDP port 47810 on your home network (`sudo ufw allow 47810/udp`).

### Android 8+
1. Download `devti-android-apk` and copy `app-release.apk` to your phone.
2. Open it. Android asks to allow installs from this source — allow it once.
3. Open Devti and follow the setup.
4. Recommended: **Settings → Apps → Devti → Battery → Unrestricted**, so Android doesn't cut the
   connection when the screen is off (some brands add their own battery savers; see
   [dontkillmyapp.com](https://dontkillmyapp.com)).

> **Updating a CI build:** until the first tagged release (which uses a permanent signing key),
> each CI build is signed with a temporary key. Android refuses to update across different keys,
> so uninstall the old build first (you'll need to pair again).

### iPhone / iPad
Arrives in Phase 8 as build-it-yourself instructions.

---

## First-time setup

On the computer that will be your **Hub**, open Devti. The setup wizard walks you through:

1. **Welcome** — what Devti is.
2. **Permissions** — what Devti will ask for and why (network, Bluetooth, screen capture, input,
   notifications). Nothing is requested until a feature needs it.
3. **This computer** — name it, and choose "Make it the Hub" (or "Join another Hub").
4. **First device** — a QR code appears. On your phone, open Devti → *Scan the QR code*. Compare
   the six digits on both screens and confirm on each.
5. **Accessories** — Devti listens for nearby Bluetooth accessories.
6. **Tour** — five short stops around the app (re-run any time from Help).
7. **Ready** — three suggestions to start with.

## Using Devti

### The ecosystem map
The prism in the middle is this computer. Each paired device sits around it in its own color: a
solid, glowing thread means connected; a dashed one means offline (it shows when it was last
seen). The ring around each device is its battery. Select a device to see its details on the right.

### Pairing more devices
Click **Pair** in the left rail. Phones and tablets scan the QR code; for another computer choose
*Another computer* and type the 6-digit code it shows (it also appears under *On your network*).
Codes expire after five minutes, and three wrong codes close the window.

### Sharing clipboard text with Android
On the phone, open **Clipboard**, copy some text, return to Devti and tap **Send text to** the
computer you want. On the Hub, open **Files → Clipboard** to see received text. With desktop
clipboard sharing enabled, copying text on the Hub also sends it to connected devices that
have clipboard sharing enabled. On the phone, tap **Copy** beside a received item to put it
on the Android clipboard. Each device keeps its own recent history. On the phone, use the
switch beside a device to turn sharing with it on or off.

### Permissions and roles
Open a device → *What it may do here*. **Trusted** allows everything; **Restricted** allows only
files, clipboard and notifications. Each switch can be changed on its own, and the other device
can never change them.

### Pause everything (kill switch)
**Security → Pause ecosystem**, or the tray menu. Every device disconnects at once and new
connections are refused until you resume.

### Activity log
**Security → Activity** lists pairings, connections, removals and permission changes. Filter by
device and export as CSV.

### Tray
Closing the window keeps Devti running in the tray (it frees the window's memory). Use the tray
to open Devti, pair, pause or quit. Turn this off in **Settings → Startup and tray**.

---

## Troubleshooting

| Problem | What to do |
|---|---|
| Phone can't find the computer | Same Wi-Fi network (not a guest network)? Some routers block discovery — the QR code still works because it contains the computer's address. |
| Pairing times out on Windows | Allow Devti in Windows Security → Firewall → *Allow an app* (private networks). |
| Codes on both screens differ | Tap *They're different*. Different codes mean the connection didn't go directly between your devices. Try again on a network you trust. |
| SmartScreen warning | *More info → Run anyway* (unsigned build). |
| Wayland asks for permission | Expected: screen sharing and remote input on Wayland always go through your desktop's dialog. |
| Android disconnects with the screen off | Set Devti's battery usage to *Unrestricted*. |
| Bluetooth accessories don't appear | Bluetooth on? Accessory in pairing mode? On Linux: `systemctl status bluetooth`. |
| Need logs for a bug report | **Settings → Diagnostics → Export** saves a redacted report to Downloads. |

## Privacy and security model

- **No servers, no accounts, no telemetry.** Devices talk directly over your local network.
- **Identity:** each device has an Ed25519 key. The private key lives in the OS keychain (Windows
  Credential Manager, GNOME Keyring/KWallet, Android Keystore). If no keychain exists, Devti says
  so in Security and uses a file only your user can read.
- **Pairing:** SPAKE2 password-authenticated key exchange over the QR secret or 6-digit code,
  bound to the TLS session and both device fingerprints, then a 6-digit verification code you
  compare by eye. A man in the middle can't make the codes match.
- **Every connection:** mutual TLS 1.3 (QUIC) with pinned fingerprints. Strangers can't even
  finish a handshake unless you've opened a pairing window.
- **Permissions are enforced by the device that owns them**, never by the requesting device.
- Full details: [docs/PROTOCOL.md](docs/PROTOCOL.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## Build from source

Requirements: Rust (stable), Node 22 + pnpm 10, and for Linux desktop builds
`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libdbus-1-dev`.

```bash
git clone https://github.com/hasi-desings/devti && cd devti
```

```bash
cargo test -p devti-core
```

```bash
cd desktop && pnpm install && pnpm tauri dev
```

Installers: `pnpm tauri build`. Android (needs the SDK, NDK, `cargo install cargo-ndk` and the
three Android Rust targets): `cd android && ./gradlew assembleRelease`.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full developer guide, including the device
simulator that lets you test the Hub without a phone.

## Project layout

```
core/          Rust core: identity, crypto, pairing, discovery, QUIC transport, storage
core/ffi/      UniFFI bindings used by the Android (and later iOS) apps
core/fuzz/     libFuzzer targets for everything parsed from the network or camera
desktop/       Devti Hub: React UI (src/) + Tauri backend (src-tauri/)
android/       Kotlin + Jetpack Compose companion app; jvm-check/ tests the bindings
docs/          DECISIONS, DESIGN, ARCHITECTURE, PROTOCOL, QA
```

## Contributing

Bug reports, translations, device profiles and code are all welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

Devti is free software under the **GNU General Public License v3.0 or later** ([LICENSE](LICENSE)).

## Credits

Devti learned from these projects (ideas and protocol knowledge only; no code was copied):
[KDE Connect](https://invent.kde.org/network/kdeconnect-kde),
[scrcpy](https://github.com/Genymobile/scrcpy),
[Deskflow](https://github.com/deskflow/deskflow),
[LocalSend](https://github.com/localsend/localsend),
[Weylus](https://github.com/H-M-H/Weylus),
[LibrePods](https://github.com/kavishdevar/librepods),
[Gadgetbridge](https://codeberg.org/Freeyourgadget/Gadgetbridge),
[Sony Headphones Client](https://github.com/Plutoberth/SonyHeadphonesClient),
[OpenFreebuds](https://github.com/melianmiko/OpenFreebuds),
[GalaxyBudsClient](https://github.com/timschneeb/GalaxyBudsClient).
Built on Tauri, quinn, rustls, ring, RustCrypto, mdns-sd, SQLite, React, React Aria, Motion,
Jetpack Compose, CameraX and ZXing. Typefaces: Bricolage Grotesque and Instrument Sans (OFL).
