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
            implementation(libs.kotlinx.coroutines.core)
            // The generated UniFFI binding (app/sufrix/core/sufrix_core.kt) lives
            // in commonMain and needs JNA at runtime.
            implementation(libs.jna)
        }
        androidMain.dependencies {
            implementation(libs.androidx.activity.compose)
            // Android needs the JNA aar (bundles the per-ABI .so loader).
            implementation("${libs.jna.get().module}:${libs.versions.jna.get()}@aar")
        }
        desktopMain.dependencies {
            implementation(compose.desktop.currentOs)
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
