# kotlin-app — Sufrix POS (Android + Desktop)

Thin Compose Multiplatform host. **No business logic** — it calls into
`rust-core` over the UniFFI Kotlin bindings (JNA). The same `commonMain` UI runs
on Android and JVM desktop; only the native artifact and entry point differ.

## Layout

```
composeApp/src/
├── commonMain/  App.kt + AppModel.kt + LoginScreen.kt   # shared Compose UI + state
│   └── app/sufrix/core/             # generated sufrix_core.kt drops in here
├── desktopMain/ main.kt            # JVM window entry + FileVault (home dir)
└── androidMain/ MainActivity.kt + FileVault (filesDir) + AndroidManifest.xml + jniLibs/
```

## Login UI (online + offline)

`LoginScreen` has Teller (name + PIN — works online AND offline via the cached
org bundle) and Manager (email + password) modes; `AppModel` owns the core
handle + token vault and forwards to `core.signIn`, which makes the
online↔offline decision (the host never branches on connectivity). `FileVault`
persists the session blob + device branch in app-private storage.

> **Not compiled in this checkout.** The login UI references only verified
> binding symbols, but it has not been built here — `gradle` isn't installed and
> the project's `androidTarget()` requires the Android SDK even to configure a
> desktop-only build. Build it in Android Studio (or with the toolchain below).
> TODO: upgrade the Android `FileVault` to Keystore / EncryptedSharedPreferences.

## Wire in the core

```bash
# Desktop (no Android SDK needed): builds the host dylib + Kotlin bindings.
cd ../rust-core && ./tool/build-bindings.sh
cp -r bindings/kotlin/app/sufrix/core \
      ../kotlin-app/composeApp/src/commonMain/kotlin/app/sufrix/

# Android (needs ANDROID_NDK_HOME + cargo-ndk): per-ABI .so into jniLibs.
cd ../rust-core && ./tool/build-android.sh
```

## Build / run

```bash
# First time (gradle not installed system-wide): generate the wrapper.
gradle wrapper --gradle-version 8.10        # or open in Android Studio, which does this

# Desktop — runnable with just JDK 17 + gradle:
./gradlew :composeApp:run -Djna.library.path=../rust-core/target/debug

# Android — requires the Android SDK:
./gradlew :composeApp:assembleDebug
```

> **Status:** this machine has no Android SDK and no system gradle, so this
> module is scaffolded but not yet built here (see PLAN.md §Risks). The desktop
> target builds once `gradle` is available; Android needs the SDK + NDK.
> Phase 6 builds out the real phone/tablet/desktop layouts per PLAN.md §6.
