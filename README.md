# Rutable

Windows 多维表格桌面程序，Rust + NSH 原生界面。客户端只在内存中保留当前文档和撤销记录，
数据、表结构、视图、分组、筛选、排序和附件均通过自建 Rust API 保存到服务器 SQLite。
客户端不读取或生成 `base.json`，不创建本地数据库、自动备份或附件缓存，也不再调用飞书 API。
本机文件只有两种：服务器已确认版本的只读副本（见下文“本机副本”）和 DPAPI 加密的登录凭证；
未保存的修改只在内存中。

## 使用

1. 打开 `https://rutable.ych.show` 下载 `rutable.exe`（免安装）。
2. 登录页填写服务器地址和密码，地址和登录都会记住，只需填一次。首次使用需设置访问密码：在客户端首次启动时，或在网页上设置。设置时需要服务器密钥
   （部署时的 `RUTABLE_API_TOKEN`），防止他人抢先占用一台新服务器；若环境变量里已有该密钥，
   客户端自动使用，只需输入密码。之后每台电脑用密码登录一次，登录凭证（不是密码）用 Windows
   DPAPI 加密保存在 `%LOCALAPPDATA%\Rutable\login-*.bin`，180 天有效；修改密码会让其他设备重新登录。
   连续 5 次密码错误后 30 秒内拒绝登录。
3. 启动后进入“我的多维表格”列表：新建、点图标选 emoji、双击/菜单重命名、拖动排序、删除
   （进入“最近删除”，可恢复）。点击进入多维表格，左上角 🏠 返回列表（先保存当前修改）。
4. 「复制给 AI」生成一个独立的 AI 密钥并复制 MCP 配置（Claude Code 命令和通用 JSON），粘贴给
   支持 MCP 的 AI 工具即可读写表格；密钥在头像菜单「管理 AI 访问」中随时撤销。

启动参数：`--api <根地址>`（覆盖登录页填写并保存在 `%LOCALAPPDATA%\Rutable\server.txt` 的地址；都没有时用 `RUTABLE_API_URL`，再默认 `https://rutable.ych.show`）、`--maximized`、`--automation [端口]`、
`--no-cache`。`--document <ID>` 直接打开该多维表格；与环境变量 `RUTABLE_API_TOKEN`（服务器密钥）
同时提供时跳过登录，供脚本和测试使用。环境变量 `RUTABLE_DOCUMENT_ID` 不再使用。
崩溃原因写入 `%LOCALAPPDATA%\Rutable\crash.log`。

## AI（MCP）

`POST https://rutable.ych.show/mcp`，Streamable HTTP，JSON 响应，`Authorization: Bearer <AI 密钥>`。
工具：`create_base`（建库时可带第一张表的表名和字段）、`list_bases`、`describe_table`、`list_records`（按字段文字包含筛选、分页）、`create_records`、
`update_records`、`delete_records`（每次最多 500 条）、`create_field`、`create_table`。字段一律按名称
传值：单选/多选传选项名（不存在时自动新增），日期 `YYYY-MM-DD HH:MM`（服务器时区），`null` 清空。
写入与客户端保存走同一条合并和版本流程，记录的修改历史标注“（AI）”；打开的客户端通过
`/documents/{id}/wait` 长轮询在约 1 秒内显示变化（本机实测 50 ms）。

编辑完成后自动保存，也可 Ctrl+S 或点击“服务器 → 立即保存”。界面状态只有收到服务器确认后
才显示已保存。保存过程中仍可编辑，新修改会保留在内存并继续提交。多个窗口修改不同内容时
进行三方合并，同一内容冲突时显示错误并保留当前内存中的修改，不静默覆盖服务器。

同步是增量的：首次启动下载一次完整文档（brotli/gzip 压缩），之后保存只上传改动的记录、
文档设置（字段、视图等，不含记录）和被调整过的行顺序，服务器逐条三方合并后只返回
此版本之后变化的内容；轮询用 `changes?since=<版本>`，无变化时返回 304。
记录在内存中以 `Arc` 写时复制共享，撤销快照和同步基线不复制整份数据。

本机副本：每次同步后（最多每 30 秒一次）把服务器确认的版本写入
`%LOCALAPPDATA%\Rutable\cache\<文档ID>-<服务器与文档的哈希>.json`。之后启动先显示该副本，
再只下载此版本之后的变化。副本不含访问密钥，也从不包含未保存的修改；副本版本比服务器新
（例如服务器从备份恢复）时丢弃并重新下载整份文档，服务器上文档被删除时同时删除副本。

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
export RUTABLE_DIST=/opt/rutable/dist   # 放 rutable.exe，供下载页提供下载
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

除健康检查、下载页、登录相关公开接口和短期附件预览链接外，接口均需 `Authorization: Bearer <凭证>`：
服务器密钥 `RUTABLE_API_TOKEN`、密码登录得到的登录凭证（`rts_`）或 AI 密钥（`rtk_`）。服务器只保存
凭证的 SHA-256 和密码的 argon2 哈希。忘记密码时在服务器执行
`sqlite3 /var/lib/rutable/rutable.sqlite3 "DELETE FROM settings WHERE key='password'"`，再用服务器密钥重新设置。
所有响应使用 `Cache-Control: no-store`。

| 方法 | 路径 | 用途 |
|---|---|---|
| GET | `/healthz` | 健康检查 |
| GET | `/api/v1/documents/{id}` | 返回 `{revision, document}`，支持 ETag / 304 |
| PUT | `/api/v1/documents/{id}` | 整份提交 `{expected_revision, document}`（导入、旧客户端），版本冲突返回 409；版本 0 创建文档 |
| GET | `/api/v1/documents` | 多维表格列表（名称、emoji、表数、记录数、更新时间，按排序）；POST 新建 `{name, icon}` |
| PATCH / DELETE | `/api/v1/documents/{id}` | 改名或换 emoji `{name?, icon?}`（作为普通文档修改）；删除进入回收站 |
| GET / POST | `/api/v1/trash`、`/api/v1/documents/{id}/restore` | 最近删除列表；恢复 |
| PUT | `/api/v1/workspace/order` | 排序 `{ids}` |
| GET | `/api/v1/documents/{id}/wait?since=<版本>&timeout=25` | 长轮询：版本变化或超时后返回 `{revision, changed}` |
| GET / POST | `/api/v1/auth/status`、`/auth/setup`、`/auth/login` | 公开：是否已设密码；首次设置 `{setup_key, password}`；登录 `{password}` → 登录凭证 |
| POST | `/api/v1/auth/password`、`/auth/logout` | 修改密码 `{current, new}`（结束其他设备登录）；退出当前登录 |
| GET / POST / DELETE | `/api/v1/auth/tokens[/{id}]` | AI 密钥：列出、生成（明文只返回一次）、撤销 |
| GET | `/`、`/download/rutable.exe` | 公开：下载页（含网页设置密码）与客户端下载（`RUTABLE_DIST` 目录） |
| GET | `/api/v1/documents/{id}/changes?since=<版本>` | 返回该版本之后变化的设置、记录（删除为 null）和行顺序；`If-None-Match` 相同时 304 |
| POST | `/api/v1/documents/{id}/changes` | 提交 `{base_revision, patch}`，逐条与之后的修改三方合并；同一内容冲突返回 409；返回 `base_revision` 之后的全部变化 |
| POST | `/api/v1/documents/{id}/attachments?name=<文件名>` | 原始文件字节上传，最大 64 MiB |
| GET | `/api/v1/documents/{id}/attachments/{UUID}` | 认证下载附件 |
| POST | `/api/v1/documents/{id}/attachments/{UUID}/link` | 生成五分钟在线预览链接 |

文档、字段、记录、视图使用原有稳定 ID。SQLite 中每条记录一行（`doc_records`，带最后修改版本，
删除保留墓碑），文档设置一行（`doc_meta`），保存只写改动的行；`doc_log` 保留每个文档最近
100 次提交的补丁（含修改前后的值）。服务器内存中缓存当前文档。旧版整份 JSON 的 `documents`
表在首次启动时迁移（`user_version=2`），原表保留不再写入。后台不调用外部平台 API。服务器密钥、登录凭证和 AI 密钥都有完整读写权限（单一所有者，无多用户权限）。

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
