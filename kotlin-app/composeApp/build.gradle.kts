import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.composeMultiplatform)
    alias(libs.plugins.composeCompiler)
    alias(libs.plugins.androidApplication)
}

kotlin {
    // Desktop (JVM) — builds with just gradle + JDK, no Android SDK required.
    jvm("desktop")

    // Android phone/tablet — requires the Android SDK installed.
    androidTarget()

    sourceSets {
        val desktopMain by getting

        commonMain.dependencies {
            implementation(compose.runtime)
            implementation(compose.foundation)
            implementation(compose.material3)
            // Material Icons Extended — the full Material vector set, mapped from
            // SwiftUI's SF Symbol names in ui/Icons.kt so the shared components
            // (buttons, fields, chips, banners, toasts) render real icons at parity
            // with the SwiftUI app instead of Unicode glyphs.
            implementation(compose.materialIconsExtended)
            implementation(compose.ui)
            // Compose resources — the real brand assets in composeResources/.
            implementation(compose.components.resources)
            implementation(libs.kotlinx.coroutines.core)
            // The generated UniFFI binding (app/madar/core/madar_core.kt) lives in
            // commonMain and needs JNA to COMPILE. `compileOnly` keeps the plain jna
            // .jar off the runtime classpath so it can't collide with the Android
            // @aar below (same com.sun.jna classes → checkDuplicateClasses fails).
            // Each platform supplies JNA at runtime: android via the @aar, desktop
            // via the .jar added in desktopMain.
            compileOnly(libs.jna)
            // Coil 3 — async network images (menu-item photos). The network engine
            // (okhttp) is added per JVM target; the fetcher auto-registers via the
            // ServiceLoader, so the default ImageLoader handles http(s) URLs.
            implementation(libs.coil.compose)
        }
        androidMain.dependencies {
            implementation(libs.androidx.activity.compose)
            // Android needs the JNA aar (bundles the per-ABI .so loader).
            implementation("${libs.jna.get().module}:${libs.versions.jna.get()}@aar")
            implementation(libs.coil.network.okhttp)
        }
        desktopMain.dependencies {
            implementation(compose.desktop.currentOs)
            // JNA runtime for the desktop JVM (commonMain carries it compileOnly).
            implementation(libs.jna)
            implementation(libs.coil.network.okhttp)
        }
    }
}

// Mark the UniFFI-generated core records as stable (see compose_stability.conf)
// so Compose can skip recomposing list items whose data hasn't changed.
composeCompiler {
    stabilityConfigurationFiles.add(
        rootProject.layout.projectDirectory.file("compose_stability.conf")
    )
}

android {
    namespace = "app.madar"
    compileSdk = libs.versions.android.compileSdk.get().toInt()
    defaultConfig {
        applicationId = "com.madar.pos"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.compileSdk.get().toInt()
        versionCode = 1
        versionName = "0.1.0"
    }
    // Local-testing only: sign `release` with the debug keystore so
    // `installRelease` can push to a device. NOT for distribution — the
    // Play Store rejects APKs signed with the Android debug key.
    signingConfigs {
        create("release") {
            storeFile = file("${System.getProperty("user.home")}/.android/debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }
    // Match Kotlin's JVM target (17) so AGP's Java compile isn't left at its 1.8
    // default — otherwise compileDebugKotlinAndroid (17) vs Java (1.8) fails the
    // JVM-target consistency check.
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("release")
        }
    }
    // Per-ABI .so produced by ../../rust-core/tool/build-android.sh land in
    // src/androidMain/jniLibs (the default jniLibs dir).
}

// Deterministic package for the generated `Res` class so imports are stable.
compose.resources {
    publicResClass = true
    packageOfResClass = "app.madar.resources"
}

// Offscreen screenshot generator — renders the component gallery to PNG with no
// window (ImageComposeScene). `./gradlew :composeApp:screenshots`.
tasks.register<JavaExec>("screenshots") {
    group = "madar"
    description = "Render the refreshed component gallery to PNGs (headless)."
    val desktopMain = kotlin.targets.getByName("desktop").compilations.getByName("main")
    dependsOn(desktopMain.compileTaskProvider)
    classpath = files(desktopMain.output.allOutputs, desktopMain.runtimeDependencyFiles)
    mainClass.set("app.madar.ScreenshotMainKt")
    systemProperty("madar.fontDir", file("src/commonMain/composeResources/font").absolutePath)
    systemProperty("madar.outDir", layout.buildDirectory.dir("screenshots").get().asFile.absolutePath)
    // Headless AWT — no Dock icon / window, just Skia rendering to a bitmap.
    systemProperty("java.awt.headless", "true")
}

compose.desktop {
    application {
        mainClass = "app.madar.MainKt"
        // Let the UniFFI Kotlin binding find libmadar_core.dylib (built by
        // `cargo build` in ../../rust-core) when running `:composeApp:run` on the
        // desktop JVM — JNA reads jna.library.path. Without this the app compiles
        // but crashes at launch with UnsatisfiedLinkError.
        jvmArgs += "-Djna.library.path=${rootProject.projectDir}/../rust-core/target/debug"
        // Disable ProGuard for the release distribution — it chokes on JNA/UniFFI
        // reflection and isn't needed for a desktop JVM bundle.
        buildTypes.release.proguard {
            isEnabled.set(false)
        }
        nativeDistributions {
            targetFormats(TargetFormat.Dmg, TargetFormat.Msi, TargetFormat.Deb)
            packageName = "Madar"
            packageVersion = "1.0.0"
        }
    }
}
