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
}
