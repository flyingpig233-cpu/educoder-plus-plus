fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/build.mjs");
    println!("cargo:rerun-if-changed=web/package-lock.json");
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let result = std::process::Command::new(npm)
        .arg("run")
        .arg("build")
        .current_dir(&root)
        .status()
        .expect("Node.js/npm required: run npm ci in crates/ui/web first");
    assert!(
        result.success(),
        "web workspace build failed; run npm ci in crates/ui/web"
    );
    slint_build::compile("ui/app.slint").expect("compile Slint UI");
    #[cfg(windows)]
    embed_icon();
}

/// Embeds `assets/app-icon.ico` into the executable, so Explorer, the taskbar
/// and shortcuts show the application icon instead of the generic default.
#[cfg(windows)]
fn embed_icon() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    let Some(rc) = find_rc() else {
        // Without a resource compiler the executable keeps the default icon; the
        // icon applied to the winit window at runtime still covers the taskbar,
        // Alt+Tab and the custom title bar.
        println!("cargo:warning=rc.exe not found, skipping the embedded Windows icon");
        return;
    };
    // SAFETY: a build script runs on its own thread before any other code reads
    // the environment, and RC_PATH only steers winresource's rc.exe lookup.
    unsafe { std::env::set_var("RC_PATH", &rc) };
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/app-icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon.to_str().expect("icon path is valid UTF-8"));
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the Windows icon: {error}");
    }
}

/// Locates the Windows SDK resource compiler. Passing it to winresource avoids
/// its registry lookup, which needs `reg.exe` and a Developer Command Prompt.
#[cfg(windows)]
fn find_rc() -> Option<std::path::PathBuf> {
    if let Some(rc) = std::env::var_os("RC_PATH") {
        let rc = std::path::PathBuf::from(rc);
        if rc.is_file() {
            return Some(rc);
        }
    }
    if let Some(bin) = std::env::var_os("WindowsSdkVerBinPath") {
        if let Some(rc) = rc_in(std::path::Path::new(&bin)) {
            return Some(rc);
        }
    }
    let mut kits = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(root) = std::env::var_os(variable) else {
            continue;
        };
        let bin = std::path::PathBuf::from(root).join("Windows Kits/10/bin");
        let Ok(entries) = std::fs::read_dir(&bin) else {
            continue;
        };
        for kit in entries.flatten().map(|entry| entry.path()) {
            let key = kit
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(kit_version);
            if let (Some(key), Some(rc)) = (key, rc_in(&kit)) {
                kits.push((key, rc));
            }
        }
    }
    kits.sort();
    kits.pop().map(|(_, rc)| rc)
}

/// Converts an SDK directory name such as `10.0.26100.0` into a sortable key.
#[cfg(windows)]
fn kit_version(name: &str) -> Option<[u32; 4]> {
    let mut parts = name.split('.');
    let mut version = [0u32; 4];
    for part in &mut version {
        *part = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(version)
}

/// Returns `bin/<host architecture>/rc.exe` when the SDK ships it.
#[cfg(windows)]
fn rc_in(bin: &std::path::Path) -> Option<std::path::PathBuf> {
    ["x64", "x86"]
        .iter()
        .map(|arch| bin.join(arch).join("rc.exe"))
        .find(|rc| rc.is_file())
}
