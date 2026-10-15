# Educoder++

Rust 编写的头歌代码工作台。采用 Cargo workspace：`educoder-core` 提供平台抽象和异步数据访问，`educoder-ui` 使用 Slint。

当前提供课堂作业浏览与代码工作区。Slint 负责无边框标题栏、侧栏和设置；本地 WebView 内嵌离线 Monaco 与 Markdown，采用 Codex 风格灰阶界面，支持深浅主题与强调色。

## 已实现

- 导入 Netscape 格式 Cookie 文件或 Cookie 请求头；过滤无关域、过期 Cookie，支持 `#HttpOnly_`，按域和路径选择 Cookie。
- 请求签名、服务器时间校准、连接与请求超时、共享请求限速、响应大小限制。
- 当前用户、课程列表、分页作业、实训详情、关卡、个人实训实例、任务描述和可编辑文件列表查询。
- 读取代码文件并解码 Base64，保留原始字节；一次获取任务描述和全部可编辑文件的 `TaskSnapshot`。
- 查询作业报告；提供单独的 `enter_shixun` 接口，只有调用此方法才可能创建实训实例，读取过程不会自动进入实训。
- 在 Windows 凭据管理器或 macOS Keychain 保存、恢复和删除会话；存储操作在后台线程执行。Linux 暂只支持从文件导入，不提供持久化凭据后端。
- 登录失效、平台错误、HTTP 错误、响应格式变化等错误分类；不记录原始响应或凭据。
- 桌面界面导入 Cookie 文件、恢复保存的登录会话、退出登录、搜索与刷新课堂、切换实训/图文作业、作业翻页和失败重试。
- 网络请求在 Tokio 后台执行，通过 Slint 事件循环更新界面；快速切换课堂或作业类型时丢弃旧请求结果。

- 会话隔离的内存/磁盘缓存；已有列表在刷新时保持显示，过期缓存先展示再后台更新，离线仍可查看已缓存内容。
- Markdown 题目、多文件 Monaco 编辑、语法高亮、查找、折叠、快捷键和自动保存的本地草稿。切换课堂/关卡、返回列表及关闭窗口前等待草稿落盘；远端变化时保留草稿并提示冲突。
- 本机 C/C++、Rust、Python、JavaScript、Java 样例运行，配置工具链路径、检查环境、查看编译诊断和 stdout/stderr、运行超时及输出比较。
- 保存到头歌、提交评测、轮询结果及手动刷新，展示平台公开测试集；隐藏测试只展示平台允许返回的结果，不展示或执行隐藏输入。
- 自绘标题栏支持拖动、双击最大化、最小化和关闭，右上角按钮无边框，关闭按钮悬停红色；窗口采用圆角，Windows 最大化时取消圆角，还原后恢复。连接状态绿色/红色，主题与编译环境集中在设置页。
- 左下角显示头歌真实头像，点击展开姓名、账号、设置、连接/切换账号和退出登录菜单；点击菜单外或按 Escape 收起。头像异步加载并按会话缓存 7 天，加载失败使用默认头像。

协议参考 [educoder-helper](https://github.com/alumkal/educoder-helper)、[educoder-cli](https://github.com/hecongnan/educoder-cli) 与平台前端请求，不是官方稳定 API。真实会话只读联调已覆盖题目、代码、测试集和提交上下文；保存/提交通过模拟 HTTP 服务验证，未对真实账号执行提交。账号密码登录和评论系统暂未实现。

## 开发

需要 Rust stable（当前为 1.96）和 Node.js/npm，版本由 Cargo/npm lockfile 锁定。Windows 桌面需要 WebView2 Runtime；本地样例需要安装对应编译器/解释器。桌面集成当前在 Windows 验证；Linux 构建 Wry 还需要系统 WebKitGTK 开发库。

首次构建先安装编辑器依赖：

```powershell
npm ci --prefix crates/ui/web
```

Cargo 构建 UI 时自动运行前端构建，JS/CSS/字体/Worker 嵌入可执行文件，启动无需 npm 服务或 CDN。

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo run -p educoder-ui
```

默认 `cargo build` 构建核心库；`--workspace` 包含 Slint UI。测试使用本机 HTTP 模拟服务，不要求账号，不会访问或提交真实作业。

## 使用桌面界面

```powershell
cargo run -p educoder-ui
```

1. 点击「连接头歌账号」，输入从浏览器导出的 Netscape 格式 `cookies.txt` 文件路径，然后点击「导入并连接」。会话验证成功后存入系统凭据库；如果保存失败，界面会提示，本次仍可使用。
2. 下次启动自动恢复已保存的会话，也支持通过 `EDUCODER_COOKIES` 指定文件。
3. 侧栏展示课堂并支持搜索和刷新。点击课堂，在主区域查看实训/图文作业，按每页 20 条翻页；总数未知时可翻至空页，再返回上一页。
4. 点击实训作业打开工作区，选择关卡和文件。若尚无个人实例，点击「进入实训」创建；普通读取不会自动创建实例。图文作业当前仅列出。
5. 左侧阅读 Markdown，右侧用 Monaco 编辑代码。Ctrl+S 保存本地草稿；「保存到头歌」同步远端；「提交评测」保存代码并提交一次，评测结束显示结果。网络结果不确定时先刷新远端结果再处理，不自动重复提交。
6. 在「本地测试」编辑输入与预期输出，点击「编译并运行」。缺少环境时到设置中填写 PATH 程序名或绝对路径，检查后保存设置。
7. 点击左下角头歌头像展开账号菜单，在其中连接/切换账号、退出登录或打开设置。设置支持深浅主题、灰/蓝/紫强调色与编译环境，点击「保存设置」持久化；连接指示始终显示绿/红状态。

界面显示真实接口返回的数据，不内置演示课堂。网络失败时保留已显示的缓存内容，登录过期时提示重新导入。课堂列表接口的分页协议仍待核实，若平台明确返回的总数大于已获取数量，界面会提示可能未显示全部课堂。

开发时可生成使用示例数据的真实 Slint 渲染图，检查两种主题、紧凑窗口、登录和加载状态（不访问平台、不打开桌面窗口）：

```powershell
cargo run -p educoder-ui --example preview
```

预览保存在 `target/ui-previews/`，不会提交到仓库。

## 缓存、草稿与执行规则

数据位于系统用户数据目录（Windows 通常为 `%LOCALAPPDATA%\Educoder\EducoderPlusPlus\data`），不写入源码目录。课程资料、作业分页、关卡和题目使用 10 分钟缓存；手动刷新强制请求。缓存按会话的不可逆摘要隔离，登录凭据仍在系统凭据库；磁盘缓存包含已读取的题目、代码、草稿和结果。设置独立保存。退出登录后不展示原会话内容，原 Cookie 文件不删除。

本地运行将当前文件复制到临时目录，编译输出放在独立目录。编译限时 30 秒，每个样例限时 5 秒，stdout/stderr 各限 1 MiB；超时结束整个子进程组。程序以当前用户权限执行，不提供虚拟机隔离。输出比较统一 CRLF 和末尾换行，其余内容精确比较。

C/C++ 编译当前任务的同语言源文件，Rust 使用 rustc（edition 2024），Java 支持无 package 的单文件入口。暂不自动安装工具链，不自动还原 Cargo/Maven/npm 依赖，也不提供 SQL、容器或平台专用服务环境。平台评测环境与本机可能不同。

验证本机工具链与真实桌面集成（只使用合成数据）：

```powershell
cargo run -p educoder-core --example local_smoke
cargo run -p educoder-ui --example native_workspace
npm test --prefix crates/ui/web
```

浏览器测试默认使用本机 Edge。真实账号 smoke 工具始终只读。

## 使用爬虫

先在浏览器登录自己的头歌账号，导出该站点的 Netscape 格式 `cookies.txt`。将文件保存到源码目录外的私人目录；勿将 Cookie 粘贴到命令行或提交 Git。

导入并验证会话后，保存到系统凭据库（Windows/macOS）：

```powershell
cargo run -p educoder-core --example crawl -- import "C:\private\cookies.txt"
cargo run -p educoder-core --example crawl -- me
cargo run -p educoder-core --example crawl -- courses
cargo run -p educoder-core --example crawl -- homeworks 12345 1
cargo run -p educoder-core --example crawl -- challenges shixun_identifier
cargo run -p educoder-core --example crawl -- games myshixun_identifier
cargo run -p educoder-core --example crawl -- task game_identifier
cargo run -p educoder-core --example crawl -- code game_identifier src/main.rs
cargo run -p educoder-core --example crawl -- report 12345
cargo run -p educoder-core --example crawl -- logout
```

也可仅指定 Cookie 文件，不写系统凭据库：

```powershell
$env:EDUCODER_COOKIES = "C:\private\cookies.txt"
cargo run -p educoder-core --example crawl -- courses
```

`logout` 删除系统凭据库条目，不删除用户原始 Cookie 文件、不撤销浏览器登录；如果仍设置 `EDUCODER_COOKIES`，后续独立进程仍会从该文件读取会话。会话过期后重新导入。

`homeworks` 逐页读取，页码从 1 开始，默认每页 50 条；核心库允许显式指定 1–100 条。课程列表分页协议未核实，当前仅返回该接口的一次响应，不承诺包含全部课程。

## 真实接口只读联调

```powershell
$env:EDUCODER_COOKIES = "C:\private\cookies.txt"
cargo run -p educoder-core --example smoke
```

`smoke` 只输出成功状态、数量和网络错误类别，不打印账号、代码、Cookie 或原始响应，也不保存会话。它抽样检查第一个课堂的实训/图文作业前两页，以及其中一个已有实训的关卡、任务文件和报告；没有相应数据时显示 `SKIP`，不会创建实例或提交作业。这是手动联调工具，常规 `cargo test` 不使用真实账号。

网络较慢时可设置 `$env:EDUCODER_TIMEOUT_SECONDS = "60"` 延长本次联调超时；应用默认仍为 30 秒。

2026-10-14 联调确认 API 位于 `https://data.educoder.net/`，网站来源仍为 `https://www.educoder.net/`。客户端直接访问 API，不依赖跨域重定向。作业模型适配 `homework_id` 和筛选后的 `query_total_count`；报告支持数字或数字字符串分数，未评分保留为缺省值，不转换为零分。

## 核心库示例

```rust,no_run
use educoder_core::{EducoderClient, Session, models::{HomeworkKind, PageRequest}};

# async fn example() -> educoder_core::Result<()> {
let session = Session::from_file("/private/cookies.txt").await?;
let client = EducoderClient::new(session)?;
let user = client.validate_session().await?;
let courses = client.get_courses().await?;
let assignments = client.get_homeworks(
    12345,
    HomeworkKind::Shixun,
    PageRequest { page: 1, limit: 50 },
).await?;
let snapshot = client.get_task_snapshot("game_identifier").await?;
// snapshot.task 提供 Markdown 描述和文件列表；snapshot.files 提供路径和原始字节。
# Ok(())
# }
```

`EducoderClient` 的 clone 共享登录状态、时间校准和请求限速。默认最小请求间隔 300ms、超时 30s、单响应及任务文件快照总字节上限 16MiB，可通过 `ClientOptions` 调整。平台返回时间错误时报告 `Error::Clock`，可创建新客户端重新校准；不自动重试可能产生副作用的调用。

核心库只向固定的头歌 HTTPS 域发送凭据，不开放任意 URL 请求，不跟随重定向。UI 应按错误类别提示重新登录或请求失败，不能将读取失败的错误文本当作代码保存。

## 目录

```text
crates/core/src/
  client.rs       平台异步接口、限速和网络处理
  dto.rs          私有远端响应模型与转换
  models.rs       GUI 可使用的领域模型
  session.rs      Cookie 导入、作用域和过期处理
  signing.rs      平台签名协议
  vault.rs        系统凭据存储
  error.rs        错误分类
  cache.rs        异步磁盘缓存与会话隔离
  runner.rs       本地工具链与限时样例运行
crates/core/examples/crawl.rs  诊断 CLI
crates/ui/src/controller.rs  后台任务与界面状态协调
crates/ui/src/workbench.rs   题目、草稿、代码操作与运行协调
crates/ui/src/webview.rs     离线 WebView 与消息桥接
crates/ui/src/preferences.rs 设置持久化与编译环境
crates/ui/src/avatar.rs      后台头像缓存、解码和圆形裁切
crates/ui/src/chrome.rs      无边框窗口控制与 Windows 圆角
crates/ui/ui/app.slint       标题栏、设置、课堂与作业界面
crates/ui/web/src/           Monaco、Markdown、测试集和结果界面
crates/ui/examples/preview.rs  离屏渲染预览
```

参考源码的许可声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
