# Educoder++

Rust 编写的头歌代码工作台。采用 Cargo workspace：`educoder-core` 提供平台抽象和异步数据访问，`educoder-ui` 使用 Slint。

当前实现爬虫核心库、独立诊断示例和课堂作业桌面界面。UI 参考 Codex 的侧栏布局与灰阶配色，支持深色/浅色模式；编辑器尚未实现。

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

代码保存、提交与评测状态接口尚未核实，当前没有实现；账号密码登录、代码编辑器和评论系统也尚未实现。接口协议参考 [educoder-helper](https://github.com/alumkal/educoder-helper)，不是官方稳定 API。未使用真实账号进行在线验证。

## 开发

需要 Rust stable，当前开发环境为 Rust 1.96，依赖版本由 `Cargo.lock` 锁定。

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
4. 左下角切换深色/浅色主题或退出登录。当前版本只浏览作业列表，不提供作业打开、代码编辑或提交按钮。

界面显示真实接口返回的数据，不内置演示课堂。网络请求失败时可重试，登录过期时提示重新导入。课堂列表接口的分页协议仍待核实，若平台明确返回的总数大于已获取数量，界面会提示可能未显示全部课堂。

开发时可生成使用示例数据的真实 Slint 渲染图，检查两种主题、紧凑窗口、登录和加载状态（不访问平台、不打开桌面窗口）：

```powershell
cargo run -p educoder-ui --example preview
```

预览保存在 `target/ui-previews/`，不会提交到仓库。

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
crates/core/examples/crawl.rs  诊断 CLI
crates/ui/src/controller.rs  后台任务与界面状态协调
crates/ui/ui/app.slint       深浅主题、课堂与作业界面
crates/ui/examples/preview.rs  离屏渲染预览
```

参考源码的许可声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
