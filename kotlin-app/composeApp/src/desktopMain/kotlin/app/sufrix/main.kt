package app.sufrix

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application

// Desktop (JVM) entry point. The UniFFI Kotlin binding loads libsufrix_core via
// JNA from java.library.path; build it with ../../rust-core (cdylib) and point
// -Djna.library.path at target/<profile>, or bundle it in resources.
fun main() = application {
    Window(onCloseRequest = ::exitApplication, title = "Sufrix POS") {
        App()
    }
}
