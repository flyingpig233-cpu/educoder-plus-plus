param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $version = ($metadata.packages | Where-Object name -eq 'educoder-ui').version
    $binary = Join-Path $metadata.target_directory "release-assets/EducoderPlusPlus-$version-windows-x64.exe"
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = & $vswhere -latest -property installationPath
    $dumpbin = Get-ChildItem (Join-Path $vs 'VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe') | Sort-Object FullName -Descending | Select-Object -First 1
    $imports = & $dumpbin.FullName /dependents $binary
    if ($LASTEXITCODE -ne 0) { throw 'dumpbin failed' }
    $imports | Write-Output
    if ($imports -match '(?i)(VCRUNTIME|MSVCP|WebView2Loader|ucrtbase)\S*\.dll') {
        throw 'The executable imports a distributable runtime DLL'
    }
    $headers = & $dumpbin.FullName /headers $binary
    if ($LASTEXITCODE -ne 0 -or -not ($headers -match '2 subsystem \(Windows GUI\)')) {
        throw 'Expected Windows GUI subsystem'
    }
    $probe = Join-Path $metadata.target_directory ('portable-check/中文 空格 ' + [guid]::NewGuid().ToString('N'))
    $mainDir = Join-Path $probe 'app'
    New-Item -ItemType Directory -Force $mainDir | Out-Null
    $copy = Join-Path $mainDir (Split-Path $binary -Leaf)
    Copy-Item -LiteralPath $binary -Destination $copy
    $previousPath = $env:PATH
    try {
        $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
        $process = Start-Process -FilePath $copy -WorkingDirectory $mainDir -WindowStyle Hidden -PassThru
        try {
            Start-Sleep -Seconds 5
            $process.Refresh()
            if ($process.HasExited) { throw "Relocated application exited early: $($process.ExitCode)" }
            if (-not $process.CloseMainWindow()) { throw 'Relocated application did not create a closable main window' }
            if (-not $process.WaitForExit(15000)) { throw 'Relocated application did not close normally' }
            if ($process.ExitCode -ne 0) { throw "Relocated application failed: $($process.ExitCode)" }
            Write-Output 'PASS: release EXE starts and closes from a standalone Unicode/spaced directory'
        } finally {
            if (-not $process.HasExited) { Stop-Process -Id $process.Id }
        }
        foreach ($example in @('native_workspace','navigation')) {
            $dir = Join-Path $probe $example
            New-Item -ItemType Directory -Force $dir | Out-Null
            $source = Join-Path $metadata.target_directory "x86_64-pc-windows-msvc/release/examples/$example.exe"
            $exe = Join-Path $dir "$example.exe"
            Copy-Item -LiteralPath $source -Destination $exe
            $process = Start-Process -FilePath $exe -WorkingDirectory $dir -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $dir 'stdout.log') -RedirectStandardError (Join-Path $dir 'stderr.log')
            try {
                if (-not $process.WaitForExit(60000)) { throw "$example timed out" }
                Get-Content (Join-Path $dir 'stdout.log') | Write-Output
                Get-Content (Join-Path $dir 'stderr.log') | Write-Output
                if ($process.ExitCode -ne 0) { throw "$example failed: $($process.ExitCode)" }
                Write-Output "PASS: relocated $example with only Windows system directories on PATH"
            } finally {
                if (-not $process.HasExited) { Stop-Process -Id $process.Id }
            }
        }
    } finally {
        $env:PATH = $previousPath
    }
    $expected = (Get-Content (Join-Path $metadata.target_directory 'release-assets/SHA256SUMS.txt')).Split(' ')[0]
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
        throw 'SHA-256 verification failed'
    }
    Write-Output "PASS: SHA-256; probe outputs: $probe"
} finally {
    Pop-Location
}
