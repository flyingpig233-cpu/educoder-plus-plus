//! Bounded local compilation and sample execution. Runs with the user's permissions.
use crate::{Error, Result, models::CodeFile};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const OUTPUT_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolchains {
    pub c: String,
    pub cpp: String,
    pub rust: String,
    pub python: String,
    pub javascript: String,
    pub java: String,
    pub javac: String,
}
impl Default for Toolchains {
    fn default() -> Self {
        Self {
            c: "gcc".into(),
            cpp: "g++".into(),
            rust: "rustc".into(),
            python: "python".into(),
            javascript: "node".into(),
            java: "java".into(),
            javac: "javac".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub name: String,
    pub input: String,
    pub expected: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub truncated: bool,
    pub elapsed_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleResult {
    pub name: String,
    pub output: ProcessOutput,
    pub passed: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalResult {
    pub compilation: Option<ProcessOutput>,
    pub samples: Vec<SampleResult>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainStatus {
    pub language: String,
    pub command: String,
    pub available: bool,
    pub version: String,
}

pub async fn inspect_toolchains(settings: &Toolchains) -> Vec<ToolchainStatus> {
    let mut results = Vec::new();
    for (name, executable, argument) in [
        ("C", &settings.c, "--version"),
        ("C++", &settings.cpp, "--version"),
        ("Rust", &settings.rust, "--version"),
        ("Python", &settings.python, "--version"),
        ("JavaScript", &settings.javascript, "--version"),
        ("Java", &settings.javac, "-version"),
    ] {
        let mut command = Command::new(executable);
        command.arg(argument);
        let output = execute(command, "", Duration::from_secs(5)).await;
        let (available, version) = match output {
            Ok(o) => (
                o.exit_code == Some(0) && !o.timed_out,
                format!("{}{}", o.stdout, o.stderr)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            Err(_) => (false, "未找到或不可执行".into()),
        };
        results.push(ToolchainStatus {
            language: name.into(),
            command: executable.clone(),
            available,
            version,
        });
    }
    results
}

pub async fn run_samples(
    files: &[CodeFile],
    entry: &str,
    samples: &[Sample],
    tools: &Toolchains,
    timeout: Duration,
) -> Result<LocalResult> {
    if samples.is_empty()
        || samples.len() > 100
        || timeout.is_zero()
        || timeout > Duration::from_secs(60)
    {
        return Err(Error::InvalidInput("测试数量或运行时限无效"));
    }
    if !files.iter().any(|f| f.path == entry) {
        return Err(Error::InvalidInput("入口文件不在当前题目文件中"));
    }
    let workspace = tempfile::tempdir().map_err(Error::Io)?;
    let build = tempfile::tempdir().map_err(Error::Io)?;
    let mut bytes = 0usize;
    for file in files {
        super::client::valid_repo_path(&file.path)?;
        bytes = bytes
            .checked_add(file.content.len())
            .ok_or(Error::ResponseTooLarge)?;
        if bytes > 16 * 1024 * 1024 {
            return Err(Error::ResponseTooLarge);
        }
        let destination = workspace.path().join(&file.path);
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(Error::Io)?;
        }
        tokio::fs::write(destination, &file.content)
            .await
            .map_err(Error::Io)?;
    }
    let source = workspace.path().join(entry);
    let binary = build.path().join(if cfg!(windows) {
        "program.exe"
    } else {
        "program"
    });
    let extension = Path::new(entry)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    let mut compiler = None;
    let mut program;
    match extension {
        "c" | "cpp" | "cc" | "cxx" => {
            let cpp = extension != "c";
            let mut command = Command::new(if cpp { &tools.cpp } else { &tools.c });
            command
                .arg(if cpp { "-std=c++17" } else { "-std=c11" })
                .arg("-O0")
                .arg("-I")
                .arg(workspace.path());
            for file in files.iter().filter(|f| {
                Path::new(&f.path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| {
                        if cpp {
                            ["cpp", "cc", "cxx"].contains(&e)
                        } else {
                            e == "c"
                        }
                    })
            }) {
                command.arg(workspace.path().join(&file.path));
            }
            command.arg("-o").arg(&binary);
            compiler = Some(command);
            program = Command::new(&binary);
        }
        "rs" => {
            let mut command = Command::new(&tools.rust);
            command
                .arg("--edition=2024")
                .arg(&source)
                .arg("-o")
                .arg(&binary);
            compiler = Some(command);
            program = Command::new(&binary);
        }
        "py" => {
            program = Command::new(&tools.python);
            program.arg(&source);
        }
        "js" | "mjs" => {
            program = Command::new(&tools.javascript);
            program.arg(&source);
        }
        "java" => {
            let class = Path::new(entry)
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or(Error::InvalidInput("Java 入口文件无效"))?;
            let mut command = Command::new(&tools.javac);
            command
                .arg("-encoding")
                .arg("UTF-8")
                .arg("-d")
                .arg(build.path())
                .arg(&source);
            compiler = Some(command);
            program = Command::new(&tools.java);
            program.arg("-cp").arg(build.path()).arg(class);
        }
        _ => {
            return Err(Error::InvalidInput(
                "本地运行支持 C、C++、Rust、Python、JavaScript 和无 package 的 Java 单文件入口",
            ));
        }
    }
    let compilation = if let Some(mut compiler) = compiler {
        compiler.current_dir(workspace.path());
        let output = execute(compiler, "", Duration::from_secs(30)).await?;
        let failed = output.exit_code != Some(0) || output.timed_out;
        if failed {
            return Ok(LocalResult {
                compilation: Some(output),
                samples: Vec::new(),
            });
        }
        Some(output)
    } else {
        None
    };
    program.current_dir(workspace.path());
    let executable = program.as_std().get_program().to_owned();
    let arguments = program
        .as_std()
        .get_args()
        .map(|a| a.to_owned())
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    for sample in samples {
        if sample.input.len() > OUTPUT_LIMIT as usize {
            return Err(Error::InvalidInput("测试输入超过 1 MiB"));
        }
        let mut command = Command::new(&executable);
        command.args(&arguments).current_dir(workspace.path());
        let output = execute(command, &sample.input, timeout).await?;
        let passed = sample.expected.as_ref().map(|expected| {
            !output.timed_out
                && !output.truncated
                && output.exit_code == Some(0)
                && normalize(&output.stdout) == normalize(expected)
        });
        results.push(SampleResult {
            name: sample.name.clone(),
            output,
            passed,
        });
    }
    Ok(LocalResult {
        compilation,
        samples: results,
    })
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end_matches('\n').to_owned()
}

async fn read_output(reader: impl AsyncRead + Unpin) -> std::io::Result<(String, bool)> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    let truncated = bytes.len() as u64 > OUTPUT_LIMIT;
    bytes.truncate(OUTPUT_LIMIT as usize);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

async fn execute(mut command: Command, input: &str, timeout: Duration) -> Result<ProcessOutput> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Known platform credential variables are not passed to compiler/program processes.
    command
        .env_remove("EDUCODER_COOKIES")
        .env_remove("EDUCODER_SESSION")
        .env_remove("EDUCODER_AUTOLOGIN");
    #[cfg(windows)]
    command.creation_flags(0x08000000 | 0x00000004);
    #[cfg(unix)]
    command.process_group(0);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|_| Error::Execution("无法启动程序，请在设置中检查编译器/解释器路径".into()))?;
    #[cfg(unix)]
    let process_id = child.id();
    #[cfg(windows)]
    let job = job::ProcessJob::attach(&child)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Execution("stdout 不可用".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Execution("stderr 不可用".into()))?;
    let out = tokio::spawn(read_output(stdout));
    let err = tokio::spawn(read_output(stderr));
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Execution("stdin 不可用".into()))?;
    let input = input.to_owned();
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(input.as_bytes()).await;
    });
    let (exit_code, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => (status.map_err(Error::Io)?.code(), false),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            (None, true)
        }
    };
    #[cfg(windows)]
    drop(job);
    #[cfg(unix)]
    if let Some(pid) = process_id {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .status()
            .await;
    }
    writer.abort();
    let (stdout, out_truncated) = out
        .await
        .map_err(|_| Error::Background)?
        .map_err(Error::Io)?;
    let (stderr, err_truncated) = err
        .await
        .map_err(|_| Error::Background)?
        .map_err(Error::Io)?;
    Ok(ProcessOutput {
        exit_code,
        stdout,
        stderr,
        timed_out,
        truncated: out_truncated || err_truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(windows)]
mod job {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::*,
            JobObjects::*,
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    pub struct ProcessJob(HANDLE);
    // Handles are owned by this guard and never used concurrently.
    unsafe impl Send for ProcessJob {}
    impl ProcessJob {
        pub fn attach(child: &tokio::process::Child) -> Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(Error::Execution("无法创建本地运行进程组".into()));
                }
                let job = Self(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let process = child
                    .raw_handle()
                    .ok_or_else(|| Error::Execution("无法获取本地进程句柄".into()))?;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, process as HANDLE) == 0
                {
                    return Err(Error::Execution("无法限制本地运行进程组".into()));
                }
                // The child starts suspended so it cannot spawn descendants before
                // joining the kill-on-close job. Resume its sole initial thread.
                let pid = child
                    .id()
                    .ok_or_else(|| Error::Execution("本地进程已退出".into()))?;
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                if snapshot == INVALID_HANDLE_VALUE {
                    return Err(Error::Execution("无法枚举本地进程线程".into()));
                }
                let mut entry: THREADENTRY32 = std::mem::zeroed();
                entry.dwSize = std::mem::size_of_val(&entry) as u32;
                let mut thread_id = None;
                let mut more = Thread32First(snapshot, &mut entry);
                while more != 0 {
                    if entry.th32OwnerProcessID == pid {
                        thread_id = Some(entry.th32ThreadID);
                        break;
                    }
                    more = Thread32Next(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
                let thread = OpenThread(
                    THREAD_SUSPEND_RESUME,
                    0,
                    thread_id.ok_or_else(|| Error::Execution("无法找到本地进程主线程".into()))?,
                );
                if thread.is_null() {
                    return Err(Error::Execution("无法打开本地进程主线程".into()));
                }
                let resumed = ResumeThread(thread);
                CloseHandle(thread);
                if resumed == u32::MAX {
                    return Err(Error::Execution("无法启动本地进程主线程".into()));
                }
                Ok(job)
            }
        }
    }
    impl Drop for ProcessJob {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn compiler_errors_are_returned_and_descendants_cannot_outlive_samples() {
        let sample = [Sample {
            name: "test".into(),
            input: String::new(),
            expected: None,
        }];
        let result = run_samples(
            &[CodeFile {
                path: "main.rs".into(),
                content: b"fn main() { this is not Rust }".to_vec(),
            }],
            "main.rs",
            &sample,
            &Toolchains::default(),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_ne!(result.compilation.as_ref().unwrap().exit_code, Some(0));
        assert!(!result.compilation.unwrap().stderr.is_empty());
        assert!(result.samples.is_empty());

        let marker_dir = tempfile::tempdir().unwrap();
        let marker = marker_dir.path().join("descendant-marker");
        let source = format!(
            r#"fn main() {{ if std::env::args().nth(1).as_deref()==Some("child") {{std::thread::sleep(std::time::Duration::from_millis(600));std::fs::write({:?}, "leaked").unwrap();}} else {{std::process::Command::new(std::env::current_exe().unwrap()).arg("child").spawn().unwrap();println!("parent finished");}} }}"#,
            marker.to_string_lossy()
        );
        let result = run_samples(
            &[CodeFile {
                path: "main.rs".into(),
                content: source.into_bytes(),
            }],
            "main.rs",
            &sample,
            &Toolchains::default(),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(result.samples[0].output.exit_code, Some(0));
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert!(
            !marker.exists(),
            "descendant survived the sample process group"
        );
    }
    #[tokio::test]
    async fn compiles_samples_and_enforces_deadline_without_losing_diagnostics() {
        let files = vec![CodeFile {
            path: "main.rs".into(),
            content: b"fn main(){println!(\"hello\");}".to_vec(),
        }];
        let samples = vec![
            Sample {
                name: "match".into(),
                input: String::new(),
                expected: Some("hello\r\n".into()),
            },
            Sample {
                name: "mismatch".into(),
                input: String::new(),
                expected: Some("other".into()),
            },
        ];
        let result = run_samples(
            &files,
            "main.rs",
            &samples,
            &Toolchains::default(),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(result.samples[0].passed, Some(true));
        assert_eq!(result.samples[1].passed, Some(false));
        let files = vec![CodeFile {
            path: "main.rs".into(),
            content: b"fn main(){loop{std::thread::sleep(std::time::Duration::from_secs(1));}}"
                .to_vec(),
        }];
        let result = run_samples(
            &files,
            "main.rs",
            &samples[..1],
            &Toolchains::default(),
            Duration::from_millis(100),
        )
        .await
        .unwrap();
        assert!(result.samples[0].output.timed_out);
        let files = vec![CodeFile {
            path: "../main.rs".into(),
            content: vec![],
        }];
        assert!(
            run_samples(
                &files,
                "../main.rs",
                &samples,
                &Toolchains::default(),
                Duration::from_secs(1)
            )
            .await
            .is_err()
        );
    }
}
