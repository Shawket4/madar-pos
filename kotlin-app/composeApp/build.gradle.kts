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
            implementation(compose.ui)
            // Compose resources — the real brand assets in composeResources/.
            implementation(compose.components.resources)
            implementation(libs.kotlinx.coroutines.core)
            // The generated UniFFI binding (app/sufrix/core/sufrix_core.kt) lives
            // in commonMain and needs JNA at runtime.
            implementation(libs.jna)
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
            implementation(libs.coil.network.okhttp)
        }
    }
}

android {
    namespace = "app.sufrix"
    compileSdk = libs.versions.android.compileSdk.get().toInt()
    defaultConfig {
        applicationId = "app.sufrix"
        minSdk = libs.versions.android.minSdk.get().toInt()
        targetSdk = libs.versions.android.compileSdk.get().toInt()
        versionCode = 1
        versionName = "0.1.0"
    }
    // Per-ABI .so produced by ../../rust-core/tool/build-android.sh land in
    // src/androidMain/jniLibs (the default jniLibs dir).
}

// Deterministic package for the generated `Res` class so imports are stable.
compose.resources {
    publicResClass = true
    packageOfResClass = "app.sufrix.resources"
}

compose.desktop {
    application {
        mainClass = "app.sufrix.MainKt"
        nativeDistributions {
            targetFormats(TargetFormat.Dmg, TargetFormat.Msi, TargetFormat.Deb)
            packageName = "Sufrix POS"
            packageVersion = "1.0.0"
        }
    }
}
