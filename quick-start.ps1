param(
  [ValidateSet("dev", "web", "build", "install", "help")]
  [string]$Mode = "dev"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

function Fail([string]$Message) {
  Write-Host ""
  Write-Host "错误：$Message" -ForegroundColor Red
  exit 1
}

function Require-Command([string]$Name, [string]$Hint) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    Fail "找不到 $Name，请先安装 $Hint。"
  }
}

Write-Host ""
Write-Host "CanonWeave · 检查运行环境" -ForegroundColor Cyan
Require-Command "node" "Node.js 20.19+ 或 22.12+"
Require-Command "npm" "npm"
Require-Command "cargo" "Rust 1.77+"

$NodeVersion = node -p "process.versions.node"
$NodeParts = $NodeVersion.Split(".")
$NodeMajor = [int]$NodeParts[0]
$NodeMinor = [int]$NodeParts[1]
$NodeSupported =
  ($NodeMajor -eq 20 -and $NodeMinor -ge 19) -or
  ($NodeMajor -eq 22 -and $NodeMinor -ge 12) -or
  ($NodeMajor -gt 22)
if (-not $NodeSupported) {
  Fail "请升级到 Node.js 20.19+ 或 22.12+。"
}

$LockFile = Join-Path $Root "package-lock.json"
$InstalledLock = Join-Path $Root "node_modules/.package-lock.json"
$NeedsInstall = -not (Test-Path $InstalledLock)
if (-not $NeedsInstall) {
  $NeedsInstall = (Get-Item $LockFile).LastWriteTimeUtc -gt (Get-Item $InstalledLock).LastWriteTimeUtc
}
if ($NeedsInstall) {
  Write-Host ""
  Write-Host "首次运行或依赖有更新，正在安装 npm 依赖…"
  npm install
  if ($LASTEXITCODE -ne 0) { Fail "npm install 失败。" }
}

switch ($Mode) {
  "help" {
    Write-Host ""
    Write-Host "用法："
    Write-Host "  .\quick-start.ps1           启动桌面应用"
    Write-Host "  .\quick-start.ps1 web       浏览器预览"
    Write-Host "  .\quick-start.ps1 build     构建发行版"
    Write-Host "  .\quick-start.ps1 install   只安装依赖"
  }
  "install" {
    Write-Host ""
    Write-Host "环境与依赖已就绪。" -ForegroundColor Green
  }
  "web" {
    Write-Host ""
    Write-Host "启动浏览器预览。项目数据功能需要在桌面应用中运行。"
    npm run dev
  }
  "build" {
    Write-Host ""
    Write-Host "开始构建 CanonWeave 发行版…"
    npm run tauri -- build
  }
  default {
    Write-Host ""
    Write-Host "启动 CanonWeave 桌面应用…" -ForegroundColor Green
    npm run tauri -- dev
  }
}

if ($LASTEXITCODE -ne 0) {
  Fail "启动或构建失败，请查看上方日志。"
}
