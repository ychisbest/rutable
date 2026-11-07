# Rutable

Windows 多维表格桌面程序，Rust + NSH 原生界面。客户端只在内存中保留当前文档和撤销记录，
数据、表结构、视图、分组、筛选、排序和附件均通过自建 Rust API 保存到服务器 SQLite。
客户端不读取或生成 `base.json`，不创建本地数据库、自动备份或附件缓存，也不再调用飞书 API。

## 使用

服务器地址：`https://rutable.ych.show`，默认文档 ID：`default`。
打开程序后可在“服务器”窗口填写连接信息，也可通过环境变量提供：

```powershell
$env:RUTABLE_API_URL = "https://rutable.ych.show"
$env:RUTABLE_DOCUMENT_ID = "default"
$env:RUTABLE_API_TOKEN = "<服务器访问密钥>"
.\target\release\rutable.exe
```

配置从进程环境或启动参数读取，密钥不会写入项目文件或文档。
`--api <根地址>`、`--document <文档ID>` 可覆盖环境变量，`--maximized` 最大化窗口，
`--automation [端口]` 启用本地测试端口。原有 `--data`、`--fresh`、`--feishu` 不再用作存储入口。

编辑完成后自动保存，也可 Ctrl+S 或点击“服务器 → 立即保存”。界面状态只有收到服务器确认后
才显示已保存。保存过程中仍可编辑，新修改会保留在内存并继续提交。多个窗口修改不同内容时
进行三方合并，同一内容冲突时显示错误并保留当前内存中的修改，不静默覆盖服务器。

关闭窗口时等待保存确认；服务器不可用时保留窗口，允许重试或显式放弃修改后退出。
强制结束进程会丢失尚未提交的内存修改。字段编辑框按 Enter 或移开焦点后提交。

附件直接上传到服务器 SQLite BLOB。点击附件打开五分钟有效的在线预览，不自动下载到客户端。
主动点击预览页的“下载文件”或使用 CSV 导出时，由用户选择保存文件；这些是明确的导出操作。

## 构建

桌面端需要 Windows Rust MSVC 工具链和 Visual Studio C++ Build Tools：

```powershell
cargo build --release --bin rutable
cargo test --bin rutable
```

后端可在 Linux 或 Windows 上独立运行，无需飞书凭据：

```sh
cargo build --release -p rutable-server
export RUTABLE_API_TOKEN='<至少32字符的随机访问密钥>'
export RUTABLE_DB=/var/lib/rutable/rutable.sqlite3
export RUTABLE_BIND=127.0.0.1:47810
./target/release/rutable-server
```

首次迁移，在服务器执行一次；已存在的文档不会被导入覆盖：

```sh
./rutable-server --db /var/lib/rutable/rutable.sqlite3 \
  --document default --import /path/to/original-base.json
```

历史飞书连接信息在迁移时丢弃，表格内容、字段、视图和分组保留。
已上传附件必须使用服务器返回的 `rutable://attachment/<UUID>` 引用；客户端文件路径不能保存到服务器。

## 部署

当前部署在 `ych.show`，Cloudflare A 记录 `rutable.ych.show` 指向服务器，Caddy 负责自动 HTTPS。
API 仅监听服务器 loopback，由独立 systemd 服务运行，不修改其他站点。

- 程序：`/opt/rutable/rutable-server`
- SQLite：`/var/lib/rutable/rutable.sqlite3`，WAL + `synchronous=FULL`
- 服务配置：`/etc/rutable/server.env`（root 私有，包含访问密钥）
- 服务：`rutable.service`
- 备份：`rutable-backup.timer` 每日在线备份，保留 14 天，位于 `/var/backups/rutable/`
- 原客户端迁移归档：`/var/backups/rutable/legacy-client/original-client-data.tar`

`deploy/` 提供 service、备份 timer、脚本和 Caddy 配置片段。

```sh
systemctl status rutable.service
journalctl -u rutable.service
systemctl start rutable-backup.service
```

## API

除健康检查和短期附件预览链接外，接口均需 `Authorization: Bearer <RUTABLE_API_TOKEN>`。
所有响应使用 `Cache-Control: no-store`。

| 方法 | 路径 | 用途 |
|---|---|---|
| GET | `/healthz` | 健康检查 |
| GET | `/api/v1/documents/{id}` | 返回 `{revision, document}`，支持 ETag / 304 |
| PUT | `/api/v1/documents/{id}` | 提交 `{expected_revision, document}`，版本冲突返回 409；版本 0 创建文档 |
| POST | `/api/v1/documents/{id}/attachments?name=<文件名>` | 原始文件字节上传，最大 64 MiB |
| GET | `/api/v1/documents/{id}/attachments/{UUID}` | 认证下载附件 |
| POST | `/api/v1/documents/{id}/attachments/{UUID}/link` | 生成五分钟在线预览链接 |

文档、字段、记录、视图使用原有稳定 ID。SQLite 单事务进行版本检查和持久化，历史表保留每个文档
最近 100 个提交版本。后台不调用外部平台 API。访问密钥是此服务的完整读写凭据，没有高级权限系统。

## 功能与代码

支持字段类型、表格、看板、画册、甘特、日历、表单、公式、关联、查找引用、仪表盘、自动化、
记录详情、评论、撤销重做、CSV 导入导出、搜索、筛选、排序和分组。此前的 URL 点击、
重复展开按钮修复保留；数字分组标题可正常显示。

- `src/ui/`：Windows 桌面界面及内存状态
- `src/api.rs`：桌面 API 客户端、连接池、版本保存和合并
- `src/model/`、`model/`：共享 Rust 数据模型、公式、查询和三方合并
- `server/`：Axum API、SQLite 存储、附件与预览
- `driver/`：真实鼠标、键盘和状态自动化工具

```sh
cargo test -p rutable-model -p rutable-server
```

测试覆盖版本冲突拒绝覆盖、持久化重启、认证、附件 BLOB、ETag、三方合并、公式和类型转换。
