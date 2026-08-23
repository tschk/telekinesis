package hk.tsc.telekinesis.mobile

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import org.json.JSONObject

data class ViewIrNode(
    val kind: String,
    val content: String?,
    val children: List<ViewIrNode>,
)

fun parseViewIr(raw: String): List<ViewIrNode> {
    val document = JSONObject(raw)
    val root = document.getJSONArray("root")
    return (0 until root.length()).map { index -> readNode(root.getJSONObject(index)) }
}

private fun readNode(node: JSONObject): ViewIrNode {
    val childrenJson = node.optJSONArray("children")
    val children =
        if (childrenJson == null) {
            emptyList()
        } else {
            (0 until childrenJson.length()).map { readNode(childrenJson.getJSONObject(it)) }
        }
    return ViewIrNode(
        kind = node.getString("kind"),
        content = node.optString("content").ifEmpty { node.optString("label").ifEmpty { null } },
        children = children,
    )
}

@Composable
fun ViewIrRootView(raw: String, modifier: Modifier = Modifier) {
    Column(
        modifier
            .fillMaxSize()
            .background(Color(0xFF09090B))
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        parseViewIr(raw).forEach { ViewIrNodeView(it) }
    }
}

@Composable
fun ViewIrNodeView(node: ViewIrNode) {
    if (node.kind == "text" || node.kind == "button") {
        Text(text = node.content.orEmpty(), color = Color(0xFFA1A1AA))
    } else {
        Column {
            node.children.forEach { ViewIrNodeView(it) }
        }
    }
}

@Composable
fun PairRootView(fixture: String) {
    ViewIrRootView(raw = fixture)
}
