package app.madar

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.madar.core.FloorTableView
import app.madar.core.ReservationView
import kotlinx.coroutines.launch

private fun tableColor(status: String): Color = when (status) {
    "free" -> Color(0xFF22C55E)
    "held" -> Color(0xFFF59E0B)
    "seated" -> Color(0xFF3B82F6)
    "dirty" -> Color(0xFF9CA3AF)
    else -> Color(0xFF9CA3AF)
}

private val STATUSES = listOf("free", "held", "seated", "dirty")

/**
 * Floor plan + host board (render-and-operate). Geometry is authored in the
 * dashboard; this renders the branch floor to scale from the core's geometry,
 * shows live status, and drives host ops. All logic is in the core.
 */
@Composable
fun FloorPlanScreen(app: AppModel) {
    val scope = rememberCoroutineScope()
    var activeSection by remember { mutableStateOf<String?>(null) }
    var statusTarget by remember { mutableStateOf<FloorTableView?>(null) }
    var seatTarget by remember { mutableStateOf<ReservationView?>(null) }

    LaunchedEffect(Unit) { app.loadFloor() }

    val sectionId = activeSection ?: app.floorSections.firstOrNull()?.id
    val section = app.floorSections.firstOrNull { it.id == sectionId }
    val tables = app.floorTables.filter { it.sectionId == sectionId }
    val canvasW = (section?.canvasW ?: 1000).toFloat()
    val canvasH = (section?.canvasH ?: 700).toFloat()

    LazyColumn(modifier = Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (app.floorSections.size > 1) {
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    app.floorSections.forEach { s ->
                        OutlinedButton(onClick = { activeSection = s.id }) { Text(s.name) }
                    }
                }
            }
        }

        // Floor canvas — tables positioned + sized to scale.
        item {
            BoxWithConstraintsFloor(canvasW, canvasH, tables, app) { statusTarget = it }
        }

        item { Text(app.t("reservations.title"), fontWeight = FontWeight.Bold, fontSize = 18.sp) }
        items(app.reservations, key = { it.id }) { b ->
            Row(
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp))
                    .background(Color(0x11000000)).padding(10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(b.customerName, fontWeight = FontWeight.SemiBold)
                    Text("${b.partySize} · ${b.status}", fontSize = 12.sp, color = Color.Gray)
                }
                OutlinedButton(onClick = { seatTarget = b }) { Text(app.t("reservations.seat")) }
                Spacer(Modifier.width(6.dp))
                TextButton(onClick = { scope.launch { app.notifyReservation(b.id) } }) { Text("🔔") }
            }
        }
    }

    // Set-status dialog.
    statusTarget?.let { tb ->
        AlertDialog(
            onDismissRequest = { statusTarget = null },
            title = { Text("${tb.label} · ${app.t("reservations.setStatus")}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    STATUSES.forEach { s ->
                        Button(
                            onClick = { scope.launch { app.setTableStatus(tb.id, s) }; statusTarget = null },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text(app.t("reservations.status_$s")) }
                    }
                }
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = { statusTarget = null }) { Text(app.t("common.cancel")) } },
        )
    }

    // Seat dialog — pick one or more tables (multiple ⇒ merged).
    seatTarget?.let { booking ->
        val picked = remember { mutableStateOf(setOf<String>()) }
        AlertDialog(
            onDismissRequest = { seatTarget = null },
            title = { Text(booking.customerName) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    tables.forEach { tb ->
                        val on = picked.value.contains(tb.id)
                        OutlinedButton(
                            onClick = {
                                picked.value = if (on) picked.value - tb.id else picked.value + tb.id
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text("${if (on) "✓ " else ""}${tb.label} · ${tb.seats} · ${tb.status}") }
                    }
                }
            },
            confirmButton = {
                Button(
                    enabled = picked.value.isNotEmpty(),
                    onClick = {
                        scope.launch {
                            if (app.seatReservation(booking.id, picked.value.toList())) seatTarget = null
                        }
                    },
                ) { Text(app.t("reservations.seat")) }
            },
            dismissButton = { TextButton(onClick = { seatTarget = null }) { Text(app.t("common.cancel")) } },
        )
    }
}

@Composable
private fun BoxWithConstraintsFloor(
    canvasW: Float,
    canvasH: Float,
    tables: List<FloorTableView>,
    app: AppModel,
    onTapTable: (FloorTableView) -> Unit,
) {
    androidx.compose.foundation.layout.BoxWithConstraints(
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Color(0x0A000000)),
    ) {
        val scale = maxWidth.value / canvasW
        Box(modifier = Modifier.fillMaxWidth().height((canvasH * scale).dp)) {
            tables.forEach { tb ->
                val color = tableColor(tb.status)
                val shape = if (tb.shape == "circle") CircleShape else RoundedCornerShape(8.dp)
                Box(
                    modifier = Modifier
                        .offset((tb.posX.toFloat() * scale).dp, (tb.posY.toFloat() * scale).dp)
                        .size((tb.width.toFloat() * scale).dp, (tb.height.toFloat() * scale).dp)
                        .clip(shape)
                        .background(color.copy(alpha = 0.22f))
                        .border(2.dp, color, shape)
                        .clickable { onTapTable(tb) },
                    contentAlignment = Alignment.Center,
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(tb.label, fontWeight = FontWeight.Bold, fontSize = 12.sp)
                        Text("${tb.seats}", fontSize = 10.sp, color = Color.Gray)
                    }
                }
            }
        }
    }
}
