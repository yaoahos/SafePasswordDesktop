package com.safeincloud.desktop

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Archive
import androidx.compose.material.icons.outlined.Backup
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Label
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.Password
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Shield
import androidx.compose.material.icons.outlined.Star
import androidx.compose.material.icons.outlined.Upload
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Divider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import kotlinx.coroutines.launch

// ---------------------------------------------------------------------------
// 演示数据（不读写真实密码库）
// ---------------------------------------------------------------------------

data class DemoLabel(val name: String, val color: Color)

data class DemoField(val label: String, val value: String, val masked: Boolean = false)

data class DemoItem(
    val id: Int,
    val title: String,
    val iconText: String,
    val fields: List<DemoField>,
    val labels: List<DemoLabel>,
    val favorite: Boolean = false,
    val trashed: Boolean = false,
)

val demoLabels = listOf(
    DemoLabel("工作", Color(0xFF2563EB)),
    DemoLabel("社交", Color(0xFFDB2777)),
    DemoLabel("银行", Color(0xFF059669)),
    DemoLabel("开发", Color(0xFF7C3AED)),
)

val demoItems = listOf(
    DemoItem(
        1, "坚果云", "坚",
        listOf(
            DemoField("账号", "demo@example.com"),
            DemoField("密码", "Nt3#kLp9vXq2", masked = true),
            DemoField("网址", "https://www.jianguoyun.com"),
            DemoField("备注", "WebDAV 同步使用"),
        ),
        listOf(demoLabels[0]), favorite = true,
    ),
    DemoItem(
        2, "GitHub", "GH",
        listOf(
            DemoField("账号", "demo-dev"),
            DemoField("密码", "Gh7\$wRt4mZn8", masked = true),
            DemoField("网址", "https://github.com"),
        ),
        listOf(demoLabels[3]),
    ),
    DemoItem(
        3, "招商银行", "招",
        listOf(
            DemoField("账号", "6225 **** **** 1234"),
            DemoField("密码", "Bk5@xQw3jHd6", masked = true),
            DemoField("备注", "查询密码与支付密码不同"),
        ),
        listOf(demoLabels[2]), favorite = true,
    ),
    DemoItem(
        4, "微博", "微",
        listOf(
            DemoField("账号", "demo@example.com"),
            DemoField("密码", "Wb2!zVc8nPm4", masked = true),
            DemoField("网址", "https://weibo.com"),
        ),
        listOf(demoLabels[1]),
    ),
    DemoItem(
        5, "公司 VPN", "VPN",
        listOf(
            DemoField("账号", "zhangsan"),
            DemoField("密码", "Vp9&kLs1cXe5", masked = true),
            DemoField("服务器", "vpn.example.com"),
        ),
        listOf(demoLabels[0]),
    ),
    DemoItem(
        6, "Google", "G",
        listOf(
            DemoField("账号", "demo@gmail.com"),
            DemoField("密码", "Go4%hJm7qRt2", masked = true),
            DemoField("网址", "https://accounts.google.com"),
        ),
        listOf(demoLabels[1], demoLabels[3]),
    ),
    DemoItem(
        7, "旧论坛（已删除）", "旧",
        listOf(DemoField("账号", "olduser"), DemoField("密码", "Xx0*zzYy9", masked = true)),
        listOf(), trashed = true,
    ),
)

// ---------------------------------------------------------------------------
// 顶栏动作
// ---------------------------------------------------------------------------

enum class TopAction(val label: String) {
    GENERATOR("密码生成器"),
    SECURITY("安全检查"),
    WEBDAV("WebDAV 同步"),
    THEME("主题"),
    IMPORT("导入密码"),
    SETTINGS("设置"),
    ABOUT("关于"),
    SYNC("同步"),
    LOCK("锁定"),
}

fun main() = application {
    Window(
        onCloseRequest = ::exitApplication,
        title = "SafePassword",
        state = rememberWindowState(width = 1280.dp, height = 820.dp),
        icon = painterResource("app-icon.png"),
    ) {
        MaterialTheme(colorScheme = lightColorScheme(primary = Color(0xFF1677FF))) {
            DemoApp()
        }
    }
}

// ---------------------------------------------------------------------------
// 主界面：顶栏 + 三栏
// ---------------------------------------------------------------------------

enum class Section(val title: String) {
    ALL("全部"),
    FAVORITES("收藏"),
    TRASH("回收站"),
}

@Composable
fun DemoApp() {
    var section by remember { mutableStateOf(Section.ALL) }
    var query by remember { mutableStateOf("") }
    var selectedId by remember { mutableStateOf(demoItems.first().id) }
    var dialog by remember { mutableStateOf<TopAction?>(null) }
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    fun notify(text: String) {
        scope.launch {
            snackbarHostState.showSnackbar(text)
        }
    }

    val visibleItems = demoItems.filter { item ->
        when (section) {
            Section.ALL -> !item.trashed
            Section.FAVORITES -> !item.trashed && item.favorite
            Section.TRASH -> item.trashed
        }
    }.filter { item ->
        query.isBlank() || item.title.contains(query, ignoreCase = true) ||
            item.fields.any { it.value.contains(query, ignoreCase = true) }
    }

    val selected = visibleItems.find { it.id == selectedId } ?: visibleItems.firstOrNull()

    Scaffold(
        snackbarHost = { SnackbarHost(snackbarHostState) },
        topBar = { TopBar(onAction = { dialog = it }) },
        containerColor = Color(0xFFF6F7F9),
    ) { padding ->
        BoxWithConstraints(Modifier.padding(padding).fillMaxSize()) {
            // 三栏自适应：侧栏固定，中间列表随窗口宽度在 300–440dp 伸缩，预览占余下空间
            val listWidth = (maxWidth * 0.34f).coerceIn(300.dp, 440.dp)
            Row(Modifier.fillMaxSize()) {
                Sidebar(
                    section = section,
                    onSelect = { section = it },
                    itemCount = demoItems.count { !it.trashed },
                    onLabelManager = { notify("演示：标签管理") },
                    onLock = { dialog = TopAction.LOCK },
                )
                Divider(Modifier.fillMaxHeight().width(1.dp), color = Color(0xFFE3E6EB))
                ItemList(
                    items = visibleItems,
                    selected = selected,
                    query = query,
                    onQueryChange = { query = it },
                    onSelect = { selectedId = it.id },
                    modifier = Modifier.width(listWidth).fillMaxHeight(),
                )
                Divider(Modifier.fillMaxHeight().width(1.dp), color = Color(0xFFE3E6EB))
                ItemPreview(selected, modifier = Modifier.weight(1f).fillMaxHeight())
            }
        }
    }

    dialog?.let { action ->
        ActionDialog(
            action = action,
            onDismiss = { dialog = null },
            notify = ::notify,
        )
    }
}

// ---------------------------------------------------------------------------
// 顶栏
// ---------------------------------------------------------------------------

// 与移动端工具页一致的彩色图标底色
private val toolIconTints = mapOf(
    TopAction.GENERATOR to (Color(0xFF1677FF) to Color(0xFFDBEAFE)),
    TopAction.SECURITY to (Color(0xFFDC2626) to Color(0xFFFEE2E2)),
    TopAction.WEBDAV to (Color(0xFF0891B2) to Color(0xFFCFFAFE)),
    TopAction.THEME to (Color(0xFF9333EA) to Color(0xFFF3E8FF)),
    TopAction.IMPORT to (Color(0xFF16A34A) to Color(0xFFDCFCE7)),
    TopAction.SETTINGS to (Color(0xFF4B5563) to Color(0xFFF3F4F6)),
    TopAction.ABOUT to (Color(0xFF0891B2) to Color(0xFFCFFAFE)),
    TopAction.SYNC to (Color(0xFF1677FF) to Color(0xFFDBEAFE)),
    TopAction.LOCK to (Color(0xFFDC2626) to Color(0xFFFEE2E2)),
)

@Composable
fun TopBar(onAction: (TopAction) -> Unit) {
    Surface(color = Color.White, shadowElevation = 2.dp) {
        Row(
            Modifier.fillMaxWidth().height(58.dp).padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TopAction.entries.forEach { action ->
                TopBarButton(action, onClick = { onAction(action) })
            }
        }
    }
}

@Composable
fun TopBarButton(action: TopAction, onClick: () -> Unit) {
    val icon = when (action) {
        TopAction.GENERATOR -> Icons.Outlined.Password
        TopAction.SECURITY -> Icons.Outlined.Shield
        TopAction.WEBDAV -> Icons.Outlined.Cloud
        TopAction.THEME -> Icons.Outlined.Palette
        TopAction.IMPORT -> Icons.Outlined.Upload
        TopAction.SETTINGS -> Icons.Outlined.Settings
        TopAction.ABOUT -> Icons.Outlined.Info
        TopAction.SYNC -> Icons.Outlined.Refresh
        TopAction.LOCK -> Icons.Outlined.Lock
    }
    val (tint, bg) = toolIconTints.getValue(action)
    TextButton(onClick = onClick, contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp)) {
        Box(
            Modifier.size(24.dp).background(bg, RoundedCornerShape(7.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = action.label, tint = tint, modifier = Modifier.size(15.dp))
        }
        Spacer(Modifier.width(6.dp))
        Text(action.label, fontSize = 12.5.sp, color = Color(0xFF30343B))
    }
}

// ---------------------------------------------------------------------------
// 左侧边栏
// ---------------------------------------------------------------------------

@Composable
fun Sidebar(
    section: Section,
    onSelect: (Section) -> Unit,
    itemCount: Int,
    onLabelManager: () -> Unit,
    onLock: () -> Unit,
) {
    Column(
        Modifier.width(230.dp).fillMaxHeight().background(Color.White).padding(vertical = 12.dp),
    ) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
            Text("我的密码库", fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
            Text("$itemCount 个条目 · 已加密", color = Color(0xFF9AA0A6), fontSize = 12.sp)
        }
        Spacer(Modifier.height(6.dp))
        Section.entries.forEach { s ->
            SidebarRow(
                selected = section == s,
                onClick = { onSelect(s) },
                icon = {
                    Icon(
                        when (s) {
                            Section.ALL -> Icons.Outlined.Shield
                            Section.FAVORITES -> Icons.Outlined.Star
                            Section.TRASH -> Icons.Outlined.Delete
                        },
                        contentDescription = null,
                        modifier = Modifier.size(19.dp),
                    )
                },
                label = s.title,
            )
        }
        SidebarRow(
            selected = false,
            onClick = onLabelManager,
            icon = {
                Icon(Icons.Outlined.Label, contentDescription = null, modifier = Modifier.size(19.dp))
            },
            label = "标签管理",
        )
        Spacer(Modifier.height(10.dp))
        Divider(color = Color(0xFFEFF1F4), modifier = Modifier.padding(horizontal = 16.dp))
        Spacer(Modifier.height(10.dp))
        Text(
            "标签",
            color = Color(0xFF9AA0A6),
            fontSize = 12.sp,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
        demoLabels.forEach { label ->
            Row(
                Modifier.fillMaxWidth().clickable { }.padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.size(10.dp).background(label.color, CircleShape))
                Spacer(Modifier.width(10.dp))
                Text(label.name, fontSize = 14.sp)
            }
        }
        Spacer(Modifier.weight(1f))
        SidebarRow(
            selected = false,
            onClick = onLock,
            icon = {
                Icon(Icons.Outlined.Lock, contentDescription = null, modifier = Modifier.size(19.dp))
            },
            label = "锁定",
        )
        Text(
            "SafePassword 0.1.0-demo",
            color = Color(0xFFB4B9BF),
            fontSize = 11.sp,
            modifier = Modifier.align(Alignment.CenterHorizontally).padding(bottom = 6.dp),
        )
    }
}

@Composable
fun SidebarRow(selected: Boolean, onClick: () -> Unit, icon: @Composable () -> Unit, label: String) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 10.dp, vertical = 2.dp)
            .background(if (selected) Color(0xFFE8F1FF) else Color.Transparent, RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon()
        Spacer(Modifier.width(11.dp))
        Text(
            label,
            fontSize = 14.sp,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            color = if (selected) MaterialTheme.colorScheme.primary else Color(0xFF30343B),
        )
    }
}

// ---------------------------------------------------------------------------
// 中间：密码库列表
// ---------------------------------------------------------------------------

@Composable
fun ItemList(
    items: List<DemoItem>,
    selected: DemoItem?,
    query: String,
    onQueryChange: (String) -> Unit,
    onSelect: (DemoItem) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.background(Color(0xFFF6F7F9))) {
        OutlinedTextField(
            value = query,
            onValueChange = onQueryChange,
            placeholder = { Text("搜索标题、账号、网址") },
            leadingIcon = { Icon(Icons.Outlined.Search, contentDescription = null) },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth().padding(12.dp),
        )
        LazyColumn(
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(items, key = { it.id }) { item ->
                ItemCard(item, selected = selected?.id == item.id, onClick = { onSelect(item) })
            }
        }
    }
}

@Composable
fun ItemCard(item: DemoItem, selected: Boolean, onClick: () -> Unit) {
    Card(
        onClick = onClick,
        colors = CardDefaults.cardColors(
            containerColor = if (selected) Color(0xFFE8F1FF) else Color.White,
        ),
        shape = RoundedCornerShape(12.dp),
        elevation = CardDefaults.cardElevation(defaultElevation = if (selected) 0.dp else 1.dp),
    ) {
        Row(Modifier.fillMaxWidth().padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(38.dp).background(Color(0xFFEAF2FF), RoundedCornerShape(10.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Text(item.iconText, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, fontSize = 13.sp)
            }
            Spacer(Modifier.width(11.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(item.title, fontWeight = FontWeight.SemiBold, fontSize = 14.5.sp, maxLines = 1)
                    if (item.favorite) {
                        Spacer(Modifier.width(4.dp))
                        Icon(
                            Icons.Outlined.Star,
                            contentDescription = "收藏",
                            tint = Color(0xFFF59E0B),
                            modifier = Modifier.size(14.dp),
                        )
                    }
                }
                Spacer(Modifier.height(2.dp))
                Text(
                    item.fields.firstOrNull()?.value ?: "",
                    color = Color(0xFF8A9099),
                    fontSize = 12.sp,
                    maxLines = 1,
                )
                if (item.labels.isNotEmpty()) {
                    Spacer(Modifier.height(5.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        item.labels.forEach { label ->
                            Text(
                                label.name,
                                fontSize = 11.sp,
                                color = label.color,
                                modifier = Modifier
                                    .background(label.color.copy(alpha = 0.10f), RoundedCornerShape(6.dp))
                                    .padding(horizontal = 7.dp, vertical = 2.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 右侧：条目预览
// ---------------------------------------------------------------------------

@Composable
fun ItemPreview(item: DemoItem?, modifier: Modifier = Modifier) {
    if (item == null) {
        Box(modifier.background(Color(0xFFF6F7F9)), contentAlignment = Alignment.Center) {
            Text("选择左侧条目查看预览", color = Color(0xFFB4B9BF))
        }
        return
    }
    Column(modifier.background(Color(0xFFF6F7F9)).padding(24.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(46.dp).background(Color(0xFFEAF2FF), RoundedCornerShape(12.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Text(item.iconText, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
            Spacer(Modifier.width(13.dp))
            Column {
                Text(item.title, fontWeight = FontWeight.Bold, fontSize = 20.sp)
                Text("登录信息 · 本地已加密", color = Color(0xFF9AA0A6), fontSize = 12.sp)
            }
            Spacer(Modifier.weight(1f))
            if (item.favorite) {
                Icon(Icons.Outlined.Star, contentDescription = "已收藏", tint = Color(0xFFF59E0B))
                Spacer(Modifier.width(8.dp))
            }
        }
        Spacer(Modifier.height(18.dp))
        Card(colors = CardDefaults.cardColors(containerColor = Color.White), shape = RoundedCornerShape(14.dp)) {
            Column(Modifier.padding(16.dp)) {
                item.fields.forEach { field ->
                    PreviewField(field)
                }
            }
        }
        Spacer(Modifier.height(14.dp))
        if (item.labels.isNotEmpty()) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                item.labels.forEach { label ->
                    Text(
                        "标签：${label.name}",
                        fontSize = 12.sp,
                        color = label.color,
                        modifier = Modifier
                            .background(label.color.copy(alpha = 0.10f), RoundedCornerShape(8.dp))
                            .padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }
            Spacer(Modifier.height(10.dp))
        }
        Text(
            "演示界面：正式版在此提供编辑、复制、历史与附件等操作。",
            color = Color(0xFFB4B9BF),
            fontSize = 12.sp,
        )
        Spacer(Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Button(onClick = { }, shape = RoundedCornerShape(10.dp)) { Text("编辑条目") }
            OutlinedButton(onClick = { }, shape = RoundedCornerShape(10.dp)) { Text("复制密码") }
        }
    }
}

@Composable
fun PreviewField(field: DemoField, initialShow: Boolean = false) {
    var showPassword by remember(field.label) { mutableStateOf(initialShow) }
    Row(
        Modifier.fillMaxWidth().padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(field.label, color = Color(0xFF8A9099), fontSize = 13.sp, modifier = Modifier.width(76.dp))
        Text(
            text = if (field.masked && !showPassword) "••••••••••••" else field.value,
            fontSize = 14.sp,
            modifier = Modifier.weight(1f),
        )
        if (field.masked) {
            IconButton(onClick = { showPassword = !showPassword }, modifier = Modifier.size(30.dp)) {
                Icon(
                    if (showPassword) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility,
                    contentDescription = "显示密码",
                    tint = Color(0xFF8A9099),
                    modifier = Modifier.size(17.dp),
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 操作弹窗：只能用自身关闭按钮关闭
// ---------------------------------------------------------------------------

@Composable
fun ActionDialog(action: TopAction, onDismiss: () -> Unit, notify: (String) -> Unit) {
    Dialog(
        // 关键：不响应外部点击与返回键，弹窗只能自己关闭
        onDismissRequest = onDismiss,
        properties = DialogProperties(dismissOnClickOutside = false, dismissOnBackPress = false),
    ) {
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = Color.White,
            modifier = Modifier.width(480.dp),
        ) {
            Column(Modifier.padding(22.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(action.label, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                    Spacer(Modifier.weight(1f))
                    IconButton(onClick = onDismiss) {
                        Icon(
                            Icons.Outlined.Close,
                            contentDescription = "关闭",
                            tint = Color(0xFF9AA0A6),
                            modifier = Modifier.size(20.dp),
                        )
                    }
                }
                Spacer(Modifier.height(10.dp))
                when (action) {
                    TopAction.GENERATOR -> GeneratorContent(notify)
                    TopAction.SECURITY -> SecurityContent()
                    TopAction.WEBDAV -> WebDavContent(notify)
                    TopAction.THEME -> ThemeContent()
                    TopAction.IMPORT -> ImportContent(notify)
                    TopAction.SETTINGS -> SettingsContent(notify)
                    TopAction.ABOUT -> AboutContent()
                    TopAction.SYNC -> SyncContent(notify)
                    TopAction.LOCK -> LockContent(notify, onDismiss)
                }
                if (action != TopAction.SETTINGS) {
                    Spacer(Modifier.height(16.dp))
                    Button(
                        onClick = onDismiss,
                        shape = RoundedCornerShape(10.dp),
                        modifier = Modifier.fillMaxWidth().height(42.dp),
                    ) { Text("关闭") }
                }
            }
        }
    }
}

@Composable
fun GeneratorContent(notify: (String) -> Unit) {
    var password by remember { mutableStateOf(randomPassword()) }
    OutlinedTextField(
        value = password,
        onValueChange = { password = it },
        label = { Text("生成的密码") },
        trailingIcon = {
            IconButton(onClick = { password = randomPassword() }) {
                Icon(Icons.Outlined.Refresh, contentDescription = "换一个")
            }
        },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(10.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        OutlinedButton(onClick = { notify("已复制到剪贴板（演示）") }) { Text("复制") }
        OutlinedButton(onClick = { password = randomPassword() }) { Text("换一个") }
    }
    Spacer(Modifier.height(8.dp))
    Text("演示：正式版提供长度、字符类型与口令短语选项。", color = Color(0xFF9AA0A6), fontSize = 12.sp)
}

fun randomPassword(): String {
    val pool = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^&*"
    return (1..16).map { pool.random() }.joinToString("")
}

@Composable
fun SecurityContent() {
    listOf(
        "弱密码" to Color(0xFFF59E0B),
        "重复使用" to Color(0xFFEF4444),
        "已泄露" to Color(0xFFDC2626),
        "长期未更新" to Color(0xFF8B5CF6),
    ).forEach { (name, color) ->
        Row(
            Modifier.fillMaxWidth().padding(vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(9.dp).background(color, CircleShape))
            Spacer(Modifier.width(10.dp))
            Text(name, fontSize = 14.sp)
            Spacer(Modifier.weight(1f))
            Text("0", fontWeight = FontWeight.Bold, fontSize = 15.sp)
        }
    }
    Spacer(Modifier.height(6.dp))
    Text("演示：正式版在此列出具体风险条目与处理建议。", color = Color(0xFF9AA0A6), fontSize = 12.sp)
}

@Composable
fun WebDavContent(notify: (String) -> Unit) {
    OutlinedTextField(
        value = "https://dav.example.com/dav/",
        onValueChange = {},
        label = { Text("服务器地址") },
        enabled = false,
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(10.dp))
    OutlinedTextField(
        value = "demo@example.com",
        onValueChange = {},
        label = { Text("用户名") },
        enabled = false,
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(12.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        OutlinedButton(onClick = { notify("测试连接…（演示）") }) { Text("测试连接") }
        Button(onClick = { notify("已保存（演示）") }) { Text("保存") }
    }
}

@Composable
fun ThemeContent() {
    val colors = listOf(
        Color(0xFF1677FF), Color(0xFF0EA5E9), Color(0xFF10B981),
        Color(0xFFF59E0B), Color(0xFFEF4444), Color(0xFF8B5CF6),
    )
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        colors.forEachIndexed { index, color ->
            Box(
                Modifier.size(40.dp).background(color, RoundedCornerShape(10.dp))
                    .border(
                        width = if (index == 0) 3.dp else 0.dp,
                        color = if (index == 0) color.copy(alpha = 0.4f) else Color.Transparent,
                        shape = RoundedCornerShape(10.dp),
                    ),
            )
        }
    }
    Spacer(Modifier.height(10.dp))
    Text("演示：正式版支持预置品牌色与自定义颜色。", color = Color(0xFF9AA0A6), fontSize = 12.sp)
}

@Composable
fun ImportContent(notify: (String) -> Unit) {
    listOf("SafeInCloud XML", "CSV", "JSON").forEach { format ->
        OutlinedButton(
            onClick = { notify("选择 $format 文件…（演示）") },
            shape = RoundedCornerShape(10.dp),
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        ) { Text("从 $format 导入") }
    }
}

private val settingTabs = listOf("安全", "同步与备份", "密码库", "显示与搜索", "导入与导出")

/** 演示用设置行：与移动端 SettingRow 观感一致（标题 + 副标题 + 右侧图标或开关）。 */
@Composable
fun DemoSettingRow(
    title: String,
    subtitle: String,
    icon: ImageVector? = null,
    trailing: (@Composable () -> Unit)? = null,
    onClick: (() -> Unit)? = null,
) {
    Row(
        Modifier.fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 14.sp)
            Text(subtitle, color = Color(0xFF9AA0A6), fontSize = 11.5.sp)
        }
        when {
            trailing != null -> trailing()
            icon != null -> Icon(icon, contentDescription = null, tint = Color(0xFF6B7280))
        }
    }
}

@Composable
fun SettingsContent(notify: (String) -> Unit) {
    var tab by remember { mutableStateOf(0) }
    var searchPasswords by remember { mutableStateOf(false) }

    ScrollableTabRow(
        selectedTabIndex = tab,
        containerColor = Color.White,
        contentColor = Color(0xFF1677FF),
        edgePadding = 0.dp,
        divider = { Divider(color = Color(0xFFEFF1F4)) },
    ) {
        settingTabs.forEachIndexed { index, title ->
            Tab(
                selected = tab == index,
                onClick = { tab = index },
                text = { Text(title, fontSize = 12.5.sp) },
            )
        }
    }
    Spacer(Modifier.height(6.dp))

    when (tab) {
        0 -> { // 安全
            DemoSettingRow("自动锁定", "1 分钟", onClick = { notify("演示：选择自动锁定时长") })
            DemoSettingRow(
                "修改主密码",
                "只重新包裹数据密钥，不重新加密附件",
                onClick = { notify("演示：修改主密码") },
            )
        }

        1 -> { // 同步与备份
            DemoSettingRow("WebDAV 同步", "未配置 · 支持坚果云、Nextcloud、NAS 等",
                icon = Icons.Outlined.Cloud, onClick = { notify("演示：配置 WebDAV") })
            DemoSettingRow("本地备份", "上次备份：—", icon = Icons.Outlined.Backup,
                onClick = { notify("演示：查看本地备份") })
            DemoSettingRow("自动备份间隔", "每 1 天", onClick = { notify("演示：选择自动备份间隔") })
            DemoSettingRow("立即同步", "上次同步：—", onClick = { notify("同步完成（演示）") })
        }

        2 -> { // 密码库
            DemoSettingRow("管理密码库", "新建、切换、重命名或删除",
                icon = Icons.Outlined.Folder, onClick = { notify("演示：管理密码库") })
            DemoSettingRow("重命名当前密码库", "我的密码库",
                onClick = { notify("演示：重命名密码库") })
        }

        3 -> { // 显示与搜索
            DemoSettingRow("默认展示", "全部条目", onClick = { notify("演示：选择默认展示的标签") })
            DemoSettingRow(
                "搜索密码",
                "允许搜索框匹配密码字段内容",
                trailing = { Switch(checked = searchPasswords, onCheckedChange = { searchPasswords = it }) },
            )
        }

        4 -> { // 导入与导出
            DemoSettingRow("导入 JSON / CSV / XML", "支持 .json / .csv / .xml / .txt；XML 兼容原版 SafeInCloud 导出",
                icon = Icons.Outlined.Upload, onClick = { notify("演示：选择导入文件") })
            DemoSettingRow("导入加密保险库", "需要原数据库主密码",
                icon = Icons.Outlined.Upload, onClick = { notify("演示：选择加密保险库") })
            DemoSettingRow("导出 JSON", "明文，包含全部字段，注意保管",
                icon = Icons.Outlined.Download, onClick = { notify("演示：导出 JSON") })
            DemoSettingRow("导出 CSV", "适合迁移到其他密码管理器",
                icon = Icons.Outlined.Download, onClick = { notify("演示：导出 CSV") })
            DemoSettingRow("导出 XML", "可还原到原版 SafeInCloud",
                icon = Icons.Outlined.Download, onClick = { notify("演示：导出 XML") })
            DemoSettingRow("导出加密备份", "仅能用主密码打开",
                icon = Icons.Outlined.Download, onClick = { notify("演示：导出加密备份") })
        }
    }
}

@Composable
fun AboutContent() {
    Text("SafePassword 0.1.0-demo", fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
    Spacer(Modifier.height(6.dp))
    Text(
        "Windows 桌面版布局演示，界面参考 SafePassword 平板端。",
        color = Color(0xFF9AA0A6), fontSize = 12.5.sp,
    )
    Spacer(Modifier.height(10.dp))
    DemoSettingRow("版本", "0.1.0-demo", icon = Icons.Outlined.Info)
    DemoSettingRow("技术栈", "Kotlin + Compose Desktop", icon = Icons.Outlined.Info)
    DemoSettingRow("功能范围", "三栏布局、顶栏工具与设置弹窗", icon = Icons.Outlined.Info)
    Spacer(Modifier.height(8.dp))
    Text(
        "演示：正式版接入 core 模块，提供真实的加密、同步与备份能力。",
        color = Color(0xFF9AA0A6), fontSize = 12.sp,
    )
}

@Composable
fun SyncContent(notify: (String) -> Unit) {
    Text("上次同步：—", fontSize = 14.sp)
    Spacer(Modifier.height(4.dp))
    Text("云端：WebDAV 未配置（演示）", color = Color(0xFF9AA0A6), fontSize = 12.5.sp)
    Spacer(Modifier.height(12.dp))
    Button(onClick = { notify("同步完成（演示）") }, shape = RoundedCornerShape(10.dp)) { Text("立即同步") }
}

@Composable
fun LockContent(notify: (String) -> Unit, onDismiss: () -> Unit) {
    Text("锁定后需要重新输入主密码才能查看密码库。", fontSize = 14.sp)
    Spacer(Modifier.height(12.dp))
    Button(
        onClick = {
            notify("已锁定（演示）")
            onDismiss()
        },
        colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFDC2626)),
        shape = RoundedCornerShape(10.dp),
        modifier = Modifier.fillMaxWidth().height(42.dp),
    ) { Text("立即锁定") }
}
