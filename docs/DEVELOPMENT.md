# 开发与技术说明

Rust 编写的头歌代码工作台。采用 Cargo workspace：`educoder-core` 提供平台抽象和异步数据访问，`educoder-ui` 使用 Slint。

当前提供课堂作业浏览与代码工作区。Slint 负责无边框标题栏、侧栏和设置；本地 WebView 内嵌离线 Monaco 与 Markdown，采用 Codex 风格灰阶界面，支持深浅主题与强调色。

## 已实现

- Windows 内嵌头歌官方微信扫码页，仅提供微信扫码登录；扫码确认后异步读取并验证会话，登录页使用 InPrivate 模式，验证后的会话保存在系统凭据库中。
- 导入 Netscape 格式 Cookie 文件或 Cookie 请求头；过滤无关域、过期 Cookie，支持 `#HttpOnly_`，按域和路径选择 Cookie。
- 请求签名、服务器时间校准、连接与请求超时、共享请求限速、响应大小限制。
- 当前用户、课程列表、分页作业、实训详情、关卡、个人实训实例、任务描述和可编辑文件列表查询。
- 读取代码文件并解码 Base64，保留原始字节；一次获取任务描述和全部可编辑文件的 `TaskSnapshot`。
- 查询作业报告；提供单独的 `enter_shixun` 接口，只有调用此方法才可能创建实训实例，读取过程不会自动进入实训。
- 在 Windows 凭据管理器或 macOS Keychain 保存、恢复和删除会话；存储操作在后台线程执行。Linux 暂只支持从文件导入，不提供持久化凭据后端。
- 登录失效、平台错误、HTTP 错误、响应格式变化等错误分类；不记录原始响应或凭据。
- 桌面界面微信扫码登录、恢复保存的登录会话、退出登录、刷新课堂、切换实训/图文作业、作业翻页和失败重试。
- 网络请求在 Tokio 后台执行，通过 Slint 事件循环更新界面；快速切换课堂或作业类型时丢弃旧请求结果。

- 会话隔离的内存/磁盘缓存；已有列表在刷新时保持显示，过期缓存先展示再后台更新，离线仍可查看已缓存内容。
- 作业列表隐藏后台缓存进度和作业/实训编号，只展示标题与“已通过关卡数 / 总关卡数”。比例复用只读预取的服务端评测状态，移除左侧状态图标，全部关卡确认通过后右侧通过比例标签使用绿色文字和浅绿色背景，整行保持普通配色；状态未获取齐全时用 `—` 占位，空实例不标记为完成。手动刷新重新核实当前页的关卡状态，工作区评测完成后同步更新列表。
- Markdown 题目、多文件 Monaco 编辑、语法高亮、查找、折叠、快捷键和自动保存的本地草稿。切换课堂/关卡、返回列表及关闭窗口前等待草稿落盘；远端变化时以本地代码为准，保留草稿并自动同步覆盖远端代码。
- 本机 C/C++、Rust、Python、JavaScript、Java 样例运行，配置工具链路径、检查环境、查看编译诊断和 stdout/stderr、运行超时及输出比较。
- 保存到头歌、提交评测、轮询结果及手动刷新，展示平台公开测试集；隐藏测试只展示平台允许返回的结果，不展示或执行隐藏输入。
- 自绘标题栏支持拖动、双击最大化、最小化和关闭，右上角按钮无边框，关闭按钮悬停红色；窗口采用圆角，Windows 最大化时取消圆角，还原后恢复。Windows 最大化/还原改由 DWM 合成器播放系统动画，只切换一次真实窗口尺寸，不逐帧缩放 Slint/WebView 内容；保留原窗口位置与大小，遵循系统动画设置。最大化或全屏时隐藏外边框并禁用边缘缩放。连接状态绿色/红色，主题与编译环境集中在设置页。
  - 动画前后临时切换 `WS_CAPTION` 时，必须通过 `SetWindowPos(SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER)` 刷新非客户区缓存，避免边框变化延迟到下一次尺寸切换。首次最大化闪烁排查时，应从新启动的进程验证首次和后续最大化；`native_workspace` 的稳定帧截图不能证明 DWM 动画中间帧没有闪烁。
- 左下角显示头歌真实头像和姓名，点击展开姓名、账号、设置、连接/切换账号和退出登录菜单；菜单以 180ms 淡入并轻微上移，每次打开重新播放，点击菜单外或按 Escape 收起。窗口和菜单动画遵循系统的减少动态效果设置。头像异步加载并按会话缓存 7 天，加载失败使用默认头像。
- “我的课堂”右上角按钮可将左侧栏收起为 64px 竖栏，保留展开按钮和账号头像入口。展开、收起采用 220ms 宽度与淡入淡出动画，代码工作区同步调整位置和宽度；遵循系统的减少动态效果设置，切换时保留课堂选择。
- 课堂、作业、题目、评测结果和答案重新查找统一使用刷新图标按钮，不显示边框和悬停文字，悬停时仅高亮图标；保留无障碍用途说明、键盘操作与加载旋转反馈；请求期间禁用重复点击，结束或失败后恢复，减少动态效果模式下使用静态加载反馈。

协议参考 [educoder-helper](https://github.com/alumkal/educoder-helper)、[educoder-cli](https://github.com/hecongnan/educoder-cli) 与平台前端请求，不是官方稳定 API。真实会话只读联调已覆盖题目、代码、测试集和提交上下文；保存/提交通过模拟 HTTP 服务验证，未对真实账号执行提交。Windows 登录已接入官方 `/wxlogin.html` 微信扫码页面，允许微信二维码授权页与头歌回调；已验证公开页面显示二维码，未使用真实账号完成扫码登录。评论系统暂未实现。

## 开发

需要 Rust stable（本次验证使用 1.96.0）和 Node.js/npm；依赖版本由 Cargo/npm lockfile 锁定。Windows 桌面需要 WebView2 Runtime；本地样例需要安装对应编译器/解释器。桌面集成当前在 Windows 验证；Linux 构建 Wry 还需要系统 WebKitGTK 开发库。

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

1. Windows 下点击「连接头歌账号」→「微信扫码登录」，使用微信扫描应用内的官方二维码并在手机上确认，再点击「已完成登录」。应用通过 WebView2 异步读取包括 HttpOnly 在内的会话 Cookie，交给核心库校验作用域并验证账号；验证成功后存入系统凭据库。如果保存失败，界面会提示，本次仍可使用。取消登录会销毁私密登录视图，失败的账号切换不会替换当前会话。桌面不再提供 Cookie 文件导入或账号密码登录；其他平台暂不支持扫码登录。
2. 下次启动自动恢复系统凭据库中已保存的会话。`EDUCODER_COOKIES` 仅供核心库诊断工具使用，桌面不读取该变量。
3. 侧栏展示课堂并支持刷新。点击课堂，在主区域通过标签栏切换实训/图文作业，选中下划线以 220ms 动画滑动，减少动态效果时立即切换；作业项悬停高亮，底部使用无边框左右箭头按每页 20 条翻页，不显示页码和总项数；总数未知时可翻至空页，再返回上一页。
4. 点击实训作业打开工作区，选择关卡和文件。若尚无个人实例，点击「进入实训」创建；普通读取不会自动创建实例。图文作业当前仅列出。
5. 左侧阅读 Markdown，右侧用 Monaco 编辑代码。编辑后自动后台同步，Ctrl+S 可立即同步；「提交评测」保存代码并提交一次，评测结束显示结果。网络结果不确定时先刷新远端结果再处理，不自动重复提交。右键可打开 Monaco 的剪切、复制和粘贴菜单。
6. 在「本地测试」编辑输入与预期输出，点击「编译并运行」。缺少环境时到设置中填写 PATH 程序名或绝对路径，检查后保存设置。
7. 点击左下角头歌头像展开账号菜单，在其中连接/切换账号、退出登录或打开设置。主题默认跟随系统，也可选择深色或浅色；支持灰/蓝/紫强调色与编译环境，点击「保存设置」持久化。旧版未包含主题模式的配置迁移为跟随系统，保留编译环境；连接指示始终显示绿/红状态。窗口化时外沿使用细灰边框区分边界，最大化和全屏时隐藏。

Markdown 渲染会兼容 `#标题`、`##标题` 等缺少空格的标题，只在显示时修正，保留代码围栏、缩进代码、HTML 代码块与 C 预处理指令。按钮、页面、弹窗和测试卡片带短过渡动画。新提交确认通过后，整窗从顶部落下彩带，中央显示 AC，约 5 秒后结束；打开旧的通过结果不会重播，动画不拦截编辑或鼠标操作。开启系统减少动态效果时，停止彩带和位移动画，保留简短 AC 提示。

界面显示真实接口返回的数据，不内置演示课堂。网络失败时保留已显示的缓存内容，登录过期时提示重新扫码登录。课堂列表接口的分页协议仍待核实，若平台明确返回的总数大于已获取数量，界面会提示可能未显示全部课堂。

开发时可生成使用示例数据的真实 Slint 渲染图，检查两种主题、紧凑窗口、登录和加载状态（不访问平台、不打开桌面窗口）：

```powershell
cargo run -p educoder-ui --example preview
```

预览保存在 `target/ui-previews/`，不会提交到仓库。

## 缓存、草稿与执行规则

编辑代码后约 500ms 持久化本地草稿，停止输入约 1.8 秒后自动后台同步到头歌；界面显示等待、同步中、已同步、离线或失败状态，不再提供“保存到头歌”按钮。同步串行执行，发送期间的新修改保留并在下一批同步。远端代码与本地不一致时，始终以当前本地代码为准同步和提交，无需手动处理冲突；刷新题目保留本地草稿，不显示缓存恢复提示。离线、评测中或结果未知时暂停写入；失败后保留草稿，需刷新题目确认远端状态，不自动重试不确定的写入。Ctrl+S 可立即触发同步，提交前等待正在进行的同步并保存当前代码。

提交使用 `game_build.json` 返回的本次任务参数和保存回执中的临时凭据，轮询 `game_status.json` 获取评测结果；负数表示进行中，`2` 表示通过，`0` 等完成状态显示未通过。临时凭据仅保留在内存，退出登录时清除。前台等待最多 60 秒，超时释放操作并每 3 秒后台查询，最多 40 次；可以手动刷新，不会重复发起提交。重启后若无法证明结果属于新一轮评测，提示结果未知，不将旧通过结果视为本轮完成。

进行中的评测先读取状态，不按完整测试集解析进度响应；兼容数字字符串状态及空测试集。实际平台在执行阶段会仅返回 `running_code_message` 与 `running_code_status`，没有 `status`，这类响应视为进度，继续轮询直至收到最终结果。提交被平台确认后，即使结果解析或读取失败，也保留已保存代码的基线和待查询状态，明确提示“代码已提交，读取评测结果失败”，继续查询而不重复提交。

平台编译输出、测试输入/输出中的 HTML 以经过清理的内容呈现，支持换行、表格和文本颜色；去除脚本、事件、嵌入页面及危险样式。普通代码文本仍按原文显示，隐藏测试输入和预期输出继续由核心库剔除。

数据位于系统用户数据目录（Windows 通常为 `%LOCALAPPDATA%\Educoder\EducoderPlusPlus\data`），不写入源码目录。课程资料、作业分页、关卡和题目使用 10 分钟缓存；手动刷新强制请求。缓存按会话的不可逆摘要隔离，登录凭据仍在系统凭据库；磁盘缓存包含已读取的题目、代码、草稿和结果。设置独立保存。退出登录后不展示原会话内容，原 Cookie 文件不删除。

显示课堂实训作业列表后，先恢复当前页本地保存的关卡通过状态，使已知全部通过的作业立即显示绿色通过比例标签；随后优先查询当前页所有作业的评测信息、更新完成比例，最后才缓存代码和后续分页。评测信息独立保存，即使代码下载失败也不影响比例显示；首次访问尚无缓存时需等待平台返回状态，未知状态不会标绿。后台缓存进度不显示。前台打开与后台预取共享请求；保存/评测期间暂停新预取，旧请求不能覆盖写入后的缓存，不改动本地草稿，也不自动创建个人实训实例。切换课堂、作业类型、重新加载列表或退出登录会取消旧队列。

只读缓存预热诊断：设置 `EDUCODER_COOKIES` 后运行 `cargo run -p educoder-core --example prefetch -- 18`，预取首个课堂前 18 项作业（最多 20 项）。只输出题目/文件数量和缓存命中数量，不保存或评测远端代码。

本地运行将当前文件复制到临时目录，编译输出放在独立目录。编译限时 30 秒，每个样例限时 5 秒，stdout/stderr 各限 1 MiB；超时结束整个子进程组。程序以当前用户权限执行，不提供虚拟机隔离。输出比较统一 CRLF 和末尾换行，其余内容精确比较。

C/C++ 编译当前任务的同语言源文件，Rust 使用 rustc（edition 2024），Java 支持无 package 的单文件入口。暂不自动安装工具链，不自动还原 Cargo/Maven/npm 依赖，也不提供 SQL、容器或平台专用服务环境。平台评测环境与本机可能不同。

验证本机工具链与真实桌面集成（只使用合成数据）：

```powershell
cargo run -p educoder-core --example local_smoke
cargo run -p educoder-ui --example native_workspace
cargo run -p educoder-ui --example navigation
npm test --prefix crates/ui/web
```

浏览器测试默认使用本机 Edge。真实账号 smoke 工具始终只读。

`native_workspace` 还会触发 Windows 非客户区重绘，检查真实窗口的标题栏像素和原生样式，覆盖最大化、还原及深浅主题，防止系统标题栏重新盖住自定义界面；截图保存在 `target/ui-previews/titlebar-native-*.png`。

`navigation` 使用合成数据连续三次进入编辑器并通过返回按钮回到作业列表，比较原生窗口像素并验证编辑内容保留。页面切换和 WebView 显隐时会完整重绘 Slint 表面，避免子窗口遮挡造成的旧页面残留；该适配使用 Slint 的内部绘制区域接口，UI 的运行库和构建器固定为 1.18.1，升级时需一起复核。

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

## 题目答案

答案原始文件从 Gitee 下载，并仅允许重定向到同仓库的 Gitee HTTPS 地址或 `raw.giteeusercontent.com` 内容 CDN。迁移后使用独立缓存命名空间，避免旧 GitHub 索引继续生效。

题目工作区的“题解”从 [educoder-answer](https://gitee.com/flyingpigcpu-233/educoder-answer) 读取公开 `index.json`，按仓库 README 规定的 `(course_id, homework_id, position)` 精确匹配，并读取对应 `answer_files`。未收录、留空或空代码显示“未找到对应答案”；网络或索引错误单独显示失败信息。答案与索引缓存 10 分钟，可手动“重新查找”；离线时已有答案以旧缓存标记显示。请求不携带头歌登录信息。弹窗采用自适应代码区与底部导入操作栏，窄窗口自动换行，来源说明默认折叠。测试结果使用无边框 Tab，选中下划线在切换时以 220ms 平滑移动并适应布局变化，减少动态效果设置下直接切换。

答案窗口支持多文件查看和复制，可将完整代码覆盖到选定的同类型可编辑文件，或选取片段插入目标文件的光标/选区（切换目标时插入文件开头）。导入前后持久化本地草稿，支持 Ctrl+Z 撤销；编辑内容会后台自动同步到头歌，不会自动提交评测。仓库的独立程序与平台头文件/填空可能不同，完整 C 程序不能直接覆盖头文件；请选取需要的表达式或函数片段。仓库验证状态不代表隐藏测试通过。

公开仓库只读诊断：`cargo run -p educoder-core --example answers -- <课程ID> <作业ID> <关卡序号>`，仅输出是否找到、文件数量和缓存状态。

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
  answers.rs      公开答案索引、精确匹配和代码缓存
  workspace.rs    前后台共享的题目预取与缓存
  runner.rs       本地工具链与限时样例运行
crates/core/examples/crawl.rs  诊断 CLI
crates/ui/src/controller.rs  后台任务与界面状态协调
crates/ui/src/workbench.rs   题目、草稿、代码操作与运行协调
crates/ui/src/webview.rs     离线 WebView 与消息桥接
crates/ui/src/preferences.rs 设置持久化与编译环境
crates/ui/src/avatar.rs      后台头像缓存、解码和圆形裁切
crates/ui/src/chrome.rs      无边框窗口控制、Windows 圆角与 DWM 原生动画
crates/ui/src/login.rs       官方网页登录视图、异步会话读取与取消清理
crates/ui/ui/app.slint       标题栏、设置、课堂与作业界面
crates/ui/web/src/           Monaco、Markdown、测试集和结果界面
crates/ui/examples/preview.rs  离屏渲染预览
```

参考源码的许可声明见 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)。
