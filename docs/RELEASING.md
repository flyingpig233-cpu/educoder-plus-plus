# Windows 单文件发布

1.0 发布目标为 `x86_64-pc-windows-msvc`。构建机需要 Rust、Node.js/npm 和 MSVC/Windows SDK；使用者无需安装这些开发工具。

## 构建

```powershell
npm ci --prefix crates/ui/web
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
npm test --prefix crates/ui/web
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

脚本明确使用 `--target`，使 `+crt-static` 只作用于目标程序，静态链接 MSVC C 运行库。WebView2 Loader 默认静态链接；Monaco、Markdown、字体、Worker、Slint 资源和图标均嵌入 EXE。发布文件和 SHA-256 清单位于 `target/release-assets/`。

EXE 不包含 WebView2 浏览器运行时，也不包含本地语言工具链。Windows 10/11 x64 需要已安装 Microsoft Edge WebView2 Runtime；本地测试按需安装编译器/解释器。配置、缓存和凭据属于当前系统用户，移动 EXE 不会迁移账号或草稿。

## 执行可移植性核验

先构建使用同一静态运行库的合成数据测试程序，再执行验证脚本：

```powershell
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --locked --release --target x86_64-pc-windows-msvc -p educoder-ui --example native_workspace --example navigation
powershell -ExecutionPolicy Bypass -File scripts/verify-windows.ps1
```

验证脚本从 Visual Studio 安装中定位 `dumpbin`，核对导入 DLL、GUI 子系统和 SHA-256，然后在新的中文空格目录执行主程序及两个示例。子进程的 PATH 仅含 Windows 系统目录；主程序会按正常启动流程恢复本机已保存会话，两个示例仅用合成数据。测试输出留在 `target/portable-check/`。

## 核验内容

- 使用 Windows SDK 的 `dumpbin /dependents` 检查 EXE，不应依赖 `VCRUNTIME*.dll`、`MSVCP*.dll` 或 `WebView2Loader.dll` 等需随包分发的 DLL。
- 将发布 EXE 单独复制到新的含中文和空格的目录，从该目录启动；检查窗口创建、进程持续运行及正常关闭，目录不放任何源码、网页资源或 DLL。
- 使用相同目标和静态运行库构建 `native_workspace`、`navigation` 示例，分别复制到空目录执行。它们只使用合成数据，验证离线 Monaco/Markdown、答案复制导入、原生 IPC、窗口与页面切换，不访问头歌、不提交作业。
- 核对 `SHA256SUMS.txt` 与实际发布资产一致。

这些检查证明本机搬移运行及嵌入资源可用，不等同于在干净 Windows 虚拟机或每个 Windows 版本验证。发布记录应写明实测环境和边界。

## GitHub Releases

完成核验后检查 diff，提交源码并推送，创建对应提交的 `v1.0.0` 标签。创建 Release 草稿，上传 EXE、SHA-256 清单和许可文件，确认资产完整后发布。已有版本和标签不得覆盖。

发布说明包含功能摘要、系统要求、可移植性检查结果和已知限制。无需将构建产物、缓存或账号数据提交到 Git。

1.0.0 的实际验证环境、结果及已知限制见 [发布记录](releases/1.0.0.md)。
