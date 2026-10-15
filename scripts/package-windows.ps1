param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
$previousFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
try {
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $version = ($metadata.packages | Where-Object name -eq 'educoder-ui').version
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
    cargo build --locked --release --target x86_64-pc-windows-msvc -p educoder-ui
    if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
    $output = Join-Path $metadata.target_directory 'release-assets'
    New-Item -ItemType Directory -Force $output | Out-Null
    $name = "EducoderPlusPlus-$version-windows-x64.exe"
    $binary = Join-Path $output $name
    Copy-Item -LiteralPath (Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release/educoder-ui.exe') -Destination $binary
    Copy-Item -LiteralPath 'LICENSE','THIRD_PARTY_NOTICES.md' -Destination $output
    $hash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $name" | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii
    Write-Output "Release assets: $output"
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousFlags
    Pop-Location
}
