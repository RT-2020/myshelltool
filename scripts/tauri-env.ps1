# tauri-env.ps1 — Windows 环境下的 Tauri 构建/开发包装脚本
#
# Toolchain 决策（GNU vs MSVC）：
# - 项目当前默认用 `x86_64-pc-windows-gnu`（Rust 1.95.0 host），原因是 MSVC toolchain
#   在该 Windows 11 build 26200 下需要 Visual Studio Build Tools，且历史上
#   Tauri + wry + windows-rs 链入路径与 MSVC linker 偶有兼容问题。
# - 如果未来切换到 MSVC toolchain：
#     1. `rustup default stable-x86_64-pc-windows-msvc`
#     2. 移除下方的 Use-BundledWindresIfAvailable（windres 是 GNU 工具）
#     3. CARGO_TARGET_DIR 的"路径含空格"逻辑保留（MSVC 也怕空格）
#
# 路径含空格的处理：
# - 项目根目录 `D:\Project\Person Github Project\myshelltool` 含空格。
# - MSVC linker (link.exe) 和 GNU dlltool 都对空格路径有问题，且 Rust 的
#   build scripts 偶尔会把路径当作 shell 参数解析。
# - 解决：如果 cwd 含空格，把 CARGO_TARGET_DIR 重定向到 `%TEMP%\myshelltool-tauri-target`。
#
# Windres 处理（仅 GNU toolchain）：
# - GNU toolchain 的 build script 调用 windres 编译 Windows 资源（.rc → .o）。
# - 如果 windres 不在 PATH，自动从 msys2 安装目录（mingw64/ucrt64/clang64）找。

$ErrorActionPreference = "Stop"

function Get-SafeCargoTargetDir {
    param(
        [string]$ProjectName
    )

    $tmpRoot = [System.IO.Path]::GetTempPath().TrimEnd('\')
    return Join-Path $tmpRoot "$ProjectName-tauri-target"
}

function Publish-TauriBuildArtifacts {
    param(
        [string]$ProjectName,
        [string]$ProjectRoot,
        [string]$CargoTargetDir
    )

    $profileDir = Join-Path $CargoTargetDir "release"
    $bundleDir = Join-Path $profileDir "bundle"
    $localReleaseDir = Join-Path $ProjectRoot "release"

    New-Item -ItemType Directory -Force -Path $localReleaseDir | Out-Null

    $appExe = Join-Path $profileDir "$ProjectName.exe"
    if (Test-Path $appExe) {
        Copy-Item -Path $appExe -Destination $localReleaseDir -Force
    }

    if (Test-Path $bundleDir) {
        Get-ChildItem -Path $bundleDir -Recurse -File | ForEach-Object {
            Copy-Item -Path $_.FullName -Destination $localReleaseDir -Force
        }
    }

    Write-Host "Published release artifacts to: $localReleaseDir"
}

function Use-BundledWindresIfAvailable {
    if (Get-Command windres -ErrorAction SilentlyContinue) {
        return
    }

    $candidateDirs = @(
        "C:\msys64\mingw64\bin",
        "C:\msys64\ucrt64\bin",
        "C:\msys64\clang64\bin"
    )

    foreach ($dir in $candidateDirs) {
        $candidate = Join-Path $dir "windres.exe"
        if (Test-Path $candidate) {
            $env:PATH = "$dir;$env:PATH"
            Write-Host "Using windres from: $candidate"
            return
        }
    }
}

$projectName = "myshelltool"
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cwd = (Get-Location).Path
$cargoTargetDir = Join-Path $projectRoot "src-tauri\target"

if ($cwd -match "\s") {
    $safeTargetDir = Get-SafeCargoTargetDir -ProjectName $projectName
    New-Item -ItemType Directory -Force -Path $safeTargetDir | Out-Null
    $env:CARGO_TARGET_DIR = $safeTargetDir
    $cargoTargetDir = $safeTargetDir
    Write-Host "Using Windows-safe CARGO_TARGET_DIR: $safeTargetDir"
}

Use-BundledWindresIfAvailable

if ($args.Count -eq 0) {
    throw "Missing tauri subcommand. Expected 'build' or 'dev'."
}

$subcommand = $args[0]
$extraArgs = @()
if ($args.Count -gt 1) {
    $extraArgs = $args[1..($args.Count - 1)]
}

& npx tauri $subcommand @extraArgs
$exitCode = $LASTEXITCODE

if ($exitCode -eq 0 -and $subcommand -eq "build") {
    Publish-TauriBuildArtifacts -ProjectName $projectName -ProjectRoot $projectRoot -CargoTargetDir $cargoTargetDir
}

exit $exitCode
