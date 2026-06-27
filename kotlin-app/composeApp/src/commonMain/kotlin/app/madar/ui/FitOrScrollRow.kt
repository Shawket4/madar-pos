package app.madar.ui

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.unit.Constraints

/**
 * A single-row bar that **fills the available width** — so `Modifier.weight()` /
 * spacers right-align as normal — when its content fits, but becomes **horizontally
 * scrollable** when the content is wider than the viewport (very narrow phones or
 * split-screen layouts). The Compose answer to SwiftUI's `ViewThatFits`: a plain
 * `horizontalScroll` can't be used directly because it gives the row an unbounded
 * width, which collapses `weight()` to zero. So we measure the natural content
 * width first, then either fill-and-pin or scroll.
 */
@Composable
fun FitOrScrollRow(
    modifier: Modifier = Modifier,
    horizontalArrangement: Arrangement.Horizontal = Arrangement.spacedBy(Space.sm),
    verticalAlignment: Alignment.Vertical = Alignment.CenterVertically,
    content: @Composable RowScope.() -> Unit,
) {
    val scroll = rememberScrollState()
    SubcomposeLayout(modifier) { constraints ->
        val viewport = constraints.maxWidth
        // 1) Measure the content at its natural (unbounded) width.
        val natural = subcompose("measure") {
            Row(horizontalArrangement = horizontalArrangement, verticalAlignment = verticalAlignment, content = content)
        }.first().measure(Constraints(maxHeight = constraints.maxHeight))
        val fits = natural.width <= viewport
        // 2) Lay it out: fill the viewport when it fits (weight/spacers work), else scroll.
        val placeable = if (fits) {
            subcompose("fit") {
                Row(Modifier.fillMaxWidth(), horizontalArrangement, verticalAlignment, content)
            }.first().measure(constraints.copy(minWidth = viewport))
        } else {
            subcompose("scroll") {
                Row(Modifier.horizontalScroll(scroll), horizontalArrangement, verticalAlignment, content)
            }.first().measure(constraints)
        }
        layout(viewport, placeable.height) { placeable.place(0, 0) }
    }
}
