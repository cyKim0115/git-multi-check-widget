# Launches the standalone widget for non-developers.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "package.json"))) {
  $Root = $PSScriptRoot
}

$InstallDir = Join-Path $env:LOCALAPPDATA "GitMultiCheckWidget"
$InstallExe = Join-Path $InstallDir "git-multi-check-widget.exe"
$ReleaseExe = Join-Path $Root "src-tauri\target\release\git-multi-check-widget.exe"

function Show-Error([string]$Message) {
  Add-Type -AssemblyName PresentationFramework | Out-Null
  [System.Windows.MessageBox]::Show($Message, "Git Multi-Check Widget", "OK", "Error") | Out-Null
}

function Ensure-VcEnv {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $vsPath = $null
  if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
  }
  if (-not $vsPath) {
    $fallback = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
    if (Test-Path $fallback) { $vsPath = $fallback }
  }
  if (-not $vsPath) { return $null }
  $bat = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
  if (-not (Test-Path $bat)) { return $null }
  return $bat
}

function Build-Release {
  $vcvars = Ensure-VcEnv
  if (-not $vcvars) {
    Show-Error "Release build needs Visual Studio C++ Build Tools.`n`nRun once in a dev shell:`n  npm run build:app`n`nThen double-click start.bat again."
    exit 1
  }

  Write-Host "Building release (first time may take a few minutes)..."
  $cmd = "`"$vcvars`" && cd /d `"$Root`" && npm run build:app"
  cmd /c $cmd
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $ReleaseExe)) {
    Show-Error "Build failed. Check Node.js / Rust / VS Build Tools."
    exit 1
  }
}

if (-not (Test-Path $ReleaseExe)) {
  if (-not (Test-Path (Join-Path $Root "node_modules"))) {
    Write-Host "Installing npm dependencies..."
    Push-Location $Root
    npm install
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
      Show-Error "npm install failed."
      exit 1
    }
  }
  Build-Release
}

if (-not (Test-Path $InstallDir)) {
  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Copy-Item -Force $ReleaseExe $InstallExe
$configDir = Join-Path $InstallDir "config"
if (-not (Test-Path $configDir)) {
  New-Item -ItemType Directory -Path $configDir -Force | Out-Null
}
$defaultConfig = Join-Path $Root "config\repos.default.json"
$userConfig = Join-Path $env:LOCALAPPDATA "GitMultiCheckWidget\repos.json"
if ((Test-Path $defaultConfig) -and -not (Test-Path $userConfig)) {
  Copy-Item -Force $defaultConfig $userConfig
}

Write-Host "Starting $InstallExe"
Start-Process -FilePath $InstallExe
