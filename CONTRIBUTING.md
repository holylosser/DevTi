# Contributing to Devti

Thanks for helping. Devti is GPL-3.0-or-later; by contributing you agree your work is released
under the same license.

## Ground rules
- **Never fake a feature.** If something can't work on a platform, show that in the UI and the
  README, and explain why.
- **$0 and open source only.** No paid services, no proprietary SDKs, no accounts.
- **Research first** for new subsystems: add an entry to `docs/DECISIONS.md` comparing at least
  three options before writing code.
- **Every component matters.** No placeholder screens or stubbed handlers in shipped code.

## Public and private files

Use sample values in shared configuration such as `.env.example`. Keep device identity backups, local Devti databases and signing keys outside Git. Store CI signing credentials in [GitHub Actions secrets](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets).

Keep confidential documents in access-controlled storage. This repository also ignores a top-level `private/` directory to prevent accidental staging of local working copies. Ignore rules apply to untracked files.

## Set up

| Need | Install |
|---|---|
| Rust | stable via rustup; nightly too if you fuzz |
| Desktop | Node 22 + pnpm 10; on Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev libdbus-1-dev` |
| Android | Android SDK + NDK, JDK 17, `cargo install cargo-ndk`, `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android` |

## Everyday commands

Core tests (unit + real-QUIC integration):
```bash
cargo test -p devti-core -p devti-ffi
```

Coverage (CI fails under 80 % lines):
```bash
cargo llvm-cov -p devti-core --summary-only
```

Fuzz a parser for a minute:
```bash
cd core && cargo +nightly fuzz run pairing_uri -- -max_total_time=60
```

Desktop app with hot reload:
```bash
cd desktop && pnpm install && pnpm tauri dev
```

Desktop checks:
```bash
cd desktop && pnpm lint && pnpm typecheck && pnpm test && pnpm test:e2e
```

Design work without Tauri (preview mode with simulated devices, clearly labelled in the UI):
```bash
cd desktop && pnpm dev
```

Android bindings tested on the JVM against the real core (no SDK needed):
```bash
cd android && ./gradlew -p jvm-check test
```

Android APK:
```bash
cd android && ./gradlew assembleRelease
```

## Test without a phone: the device simulator
`sim_device` is a real Devti node that pairs, auto-confirms the code and reports a battery.
Open *Pair a device* in the Hub, choose *Another computer*, and use the code and address it shows
(addresses are listed in **Settings → This computer**):

```bash
cargo run -p devti-core --example sim_device -- --code 192.168.1.20:47810 482913 --name "Test Phone"
```

The real-app smoke test (`desktop/tests/real/smoke.mjs`) uses the same simulator through
WebDriver to pair with the actual desktop binary.

## Style
- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, no `unsafe` in core.
- TypeScript: Biome (`pnpm lint`), strict TypeScript.
- Kotlin: official style; keep anything that doesn't need Android in `core/state` so it stays
  testable on the JVM.
- UI copy: sentence case, plain verbs, say what happens. No all-caps labels.
- Design: follow `docs/DESIGN.md`. No component-library default styles, no stock icon packs.

## Pull requests
1. One topic per PR, with tests.
2. Explain *why* in the description; link the DECISIONS entry if you add a dependency.
3. CI must be green (lint, types, tests, coverage, fuzz smoke, real-app smoke, installers, APK).

## Bluetooth device profiles
Device profiles (Phase 5) are declarative YAML files in `profiles/`. Use the *Device profile*
issue template to share the capability report Devti generates for an unsupported accessory.
Never include serial numbers or MAC addresses.
