import com.safeincloud.core.crypto.Crypto
import com.safeincloud.core.crypto.VaultContainer
import com.safeincloud.core.json.AppJson
import com.safeincloud.core.model.AttachmentRef
import com.safeincloud.core.model.Field
import com.safeincloud.core.model.FieldType
import com.safeincloud.core.model.ItemStatus
import com.safeincloud.core.model.Label
import com.safeincloud.core.model.Tombstone
import com.safeincloud.core.model.Vault
import com.safeincloud.core.model.VaultItem
import java.io.File
import kotlinx.serialization.encodeToString

/**
 * 与 Rust 侧互操作的夹具工具（桌面仓库专用，SafePassword 仓库只读）。
 *
 * gen <dir>                 生成 known1.sicv（中文+emoji 主密码）与已知明文 JSON
 * verify <file> <pw> <out>  用 Kotlin 容器打开 .sicv，把明文 vault JSON 写到 out
 */

val TEST_PASSWORD = "测试密码123!🚀"

/** 固定 contentKey（base64 of 0x00..0x1F），保证明文 JSON 确定性。 */
val FIXTURE_CONTENT_KEY: String = Crypto.toBase64(ByteArray(32) { it.toByte() })

fun fixtureVault(): Vault {
    val lblBank = Label(
        id = "lbl-0000-0000-0000-000000000001",
        name = "银行",
        color = 0xFF0F766E.toInt(),
        sortOrder = 0
    )
    val lblWork = Label(
        id = "lbl-0000-0000-0000-000000000002",
        name = "工作",
        color = 0xFFE91E63.toInt(),
        sortOrder = 1
    )

    val github = VaultItem(
        id = "item-0000-0000-0000-000000000001",
        templateId = "login",
        title = "GitHub",
        fields = mutableListOf(
            Field(id = "fld-0000-0000-0000-000000000001", label = "账号", value = "user@example.com"),
            Field(id = "fld-0000-0000-0000-000000000002", label = "密码", value = "P@ssw0rd!", type = FieldType.PASSWORD),
            Field(id = "fld-0000-0000-0000-000000000003", label = "网址", value = "https://example.com", type = FieldType.URL)
        ),
        url = "https://example.com",
        labelIds = mutableListOf("lbl-0000-0000-0000-000000000002"),
        favorite = true,
        createdAt = 1700000000000,
        updatedAt = 1700000100000,
        iconKey = "key"
    )

    val card = VaultItem(
        id = "item-0000-0000-0000-000000000002",
        templateId = "bank_card",
        title = "招商银行",
        fields = mutableListOf(
            Field(id = "fld-0000-0000-0000-000000000004", label = "卡号", value = "6222021234567890", sensitive = true),
            Field(id = "fld-0000-0000-0000-000000000005", label = "持卡人", value = "张三"),
            Field(id = "fld-0000-0000-0000-000000000006", label = "有效期", value = "12/30"),
            Field(id = "fld-0000-0000-0000-000000000007", label = "CVV", value = "123", type = FieldType.PASSWORD),
            Field(id = "fld-0000-0000-0000-000000000008", label = "银行", value = "招商银行")
        ),
        notes = "额度5万\n临时冻结电话 95555",
        notesLabel = "卡片备注",
        notesType = FieldType.MULTILINE,
        notesPosition = 2,
        createdAt = 1700000200000,
        updatedAt = 1700000300000
    )

    val wifi = VaultItem(
        id = "item-0000-0000-0000-000000000003",
        templateId = "wifi",
        title = "MyWiFi-5G",
        fields = mutableListOf(
            Field(id = "fld-0000-0000-0000-000000000009", label = "网络名称", value = "MyWiFi-5G"),
            Field(id = "fld-0000-0000-0000-000000000010", label = "密码", value = "中文密码测试", type = FieldType.PASSWORD),
            Field(id = "fld-0000-0000-0000-000000000011", label = "加密方式", value = "WPA2")
        ),
        status = ItemStatus.ARCHIVED,
        createdAt = 1700000400000,
        updatedAt = 1700000500000
    )

    val note = VaultItem(
        id = "item-0000-0000-0000-000000000004",
        templateId = "note",
        title = "安全笔记",
        notes = "许可证：ABCD-1234-EFGH",
        status = ItemStatus.TRASHED,
        createdAt = 1700000600000,
        updatedAt = 1700000700000,
        deletedAt = 1700000700000
    )

    val email = VaultItem(
        id = "item-0000-0000-0000-000000000005",
        templateId = "email",
        title = "我的邮箱",
        fields = mutableListOf(
            Field(id = "fld-0000-0000-0000-000000000012", label = "邮箱", value = "my@mail.com", type = FieldType.EMAIL),
            Field(id = "fld-0000-0000-0000-000000000013", label = "密码", value = "émoji🔐密码", type = FieldType.PASSWORD),
            Field(id = "fld-0000-0000-0000-000000000014", label = "服务器", value = "imap.mail.com")
        ),
        attachments = mutableListOf(
            AttachmentRef(
                id = "att-0000-0000-0000-000000000001",
                name = "秘密.txt",
                mime = "text/plain",
                size = 12,
                addedAt = 1700000800000
            )
        ),
        createdAt = 1700000800000,
        updatedAt = 1700000900000,
        iconKey = "letter"
    )

    return Vault(
        schema = 1,
        id = "vault-0000-0000-0000-000000000001",
        name = "测试密码库🚀",
        items = mutableListOf(github, card, wifi, note, email),
        labels = mutableListOf(lblBank, lblWork),
        createdAt = 1690000000000,
        updatedAt = 1700001000000,
        revision = 42,
        lastSyncedAt = 1700000950000,
        deviceId = "device-0000-0000-0000-000000000001",
        contentKey = FIXTURE_CONTENT_KEY,
        tombstones = mutableListOf(
            Tombstone(id = "item-0000-0000-0000-000000000009", deletedAt = 1700000990000)
        )
    )
}

fun gen(dir: File) {
    dir.mkdirs()

    // known1：内容丰富 + 中文+emoji 主密码（验证 PBKDF2 口令 UTF-8 编码）
    val container = VaultContainer.create(fixtureVault(), TEST_PASSWORD.toCharArray())
    val json = AppJson.instance.encodeToString(Vault.serializer(), container.vault)
    File(dir, "known1.sicv").writeBytes(container.toBytes())
    File(dir, "known1.password.txt").writeText(TEST_PASSWORD)
    File(dir, "known1.vault.json").writeText(json)

    // known2：空库 + 纯 ASCII 口令
    val emptyContainer = VaultContainer.create(Vault(), "pw2".toCharArray())
    File(dir, "known2.sicv").writeBytes(emptyContainer.toBytes())
    File(dir, "known2.password.txt").writeText("pw2")
    File(dir, "known2.vault.json")
        .writeText(AppJson.instance.encodeToString(Vault.serializer(), emptyContainer.vault))

    println("OK gen -> ${dir.absolutePath}")
}

fun verify(file: File, password: String, out: File) {
    val container = VaultContainer.open(file.readBytes(), password.toCharArray())
    val json = AppJson.instance.encodeToString(Vault.serializer(), container.vault)
    out.parentFile?.mkdirs()
    out.writeText(json)
    println("OK verify items=${container.vault.items.size} name=${container.vault.name}")
}

fun main(args: Array<String>) {
    when (args.firstOrNull()) {
        "gen" -> gen(File(args.getOrElse(1) { "fixtures/android" }))
        "verify" -> {
            // --b64 模式：password 以 Base64 传入，规避 shell 传参编码问题
            val b64 = args.getOrNull(1) == "--b64"
            require(if (b64) args.size >= 5 else args.size >= 4) {
                "usage: verify [--b64] <file> <password-b64?> <out.json>"
            }
            val file = File(if (b64) args[2] else args[1])
            val password = if (b64) {
                String(java.util.Base64.getDecoder().decode(args[3]))
            } else {
                args[2]
            }
            val out = File(if (b64) args[4] else args[3])
            verify(file, password, out)
        }
        else -> {
            println("usage: gen <dir> | verify <file> <password> <out.json>")
            kotlin.system.exitProcess(2)
        }
    }
}
