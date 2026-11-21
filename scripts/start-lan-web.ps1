# Start the password-protected LAN web experience on Windows.
#
# The product API stays bound to 127.0.0.1; Vite also stays local. This script
# starts both if needed, then puts the authenticated gateway in the foreground.
# Ctrl+C stops the gateway and any child processes started here.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\start-lan-web.ps1
#
# Optional overrides:
#   $env:SELF_TOOLS_LAN_PORT = 8443
#   $env:SELF_TOOLS_LAN_CERT / $env:SELF_TOOLS_LAN_KEY = paths to PEM files

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$logDir = Join-Path $repoRoot "output\web-logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$serverExe = Join-Path $repoRoot "target\debug\devtoolbox-server.exe"
$started = @()

# 网关与上游必须读同一份数据目录：网关据此找到 gateway-signing.key
#（首次启动由任一方生成，权限限当前用户），上游据此持久化数据。
$env:SELF_TOOLS_DATA_DIR = Join-Path $repoRoot "config"

function Test-Listening([int]$Port) {
    return [bool](Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
}

function Wait-Listening([int]$Port, [int]$Seconds) {
    for ($i = 0; $i -lt $Seconds * 2; $i++) {
        if (Test-Listening $Port) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

function Start-LoggedProcess([string]$FilePath, [string[]]$Arguments, [string]$Name) {
    $stdout = Join-Path $logDir "$Name.log"
    $stderr = Join-Path $logDir "$Name.err.log"
    $procParams = @{
        FilePath               = $FilePath
        WorkingDirectory       = $repoRoot
        WindowStyle            = "Hidden"
        RedirectStandardOutput = $stdout
        RedirectStandardError  = $stderr
        PassThru               = $true
    }
    if ($Arguments.Count -gt 0) { $procParams["ArgumentList"] = $Arguments }
    $proc = Start-Process @procParams
    $script:started += $proc
    Write-Host "· 已启动 $Name (PID $($proc.Id))，日志：$stdout"
}

try {
    if (-not (Test-Listening 8080)) {
        if (-not (Test-Path $serverExe)) {
            throw "未找到数据服务 $serverExe，请先运行：cargo build -p devtoolbox-server"
        }
        Start-LoggedProcess $serverExe @() "server"
        if (-not (Wait-Listening 8080 30)) {
            throw "数据服务未能在 8080 端口就绪，请查看 output\web-logs\server.err.log"
        }
    } else {
        Write-Host "· 数据服务已在 127.0.0.1:8080 运行。"
    }

    if (-not (Test-Listening 1420)) {
        Start-LoggedProcess "npm.cmd" @("--prefix", "apps/desktop/ui", "run", "dev") "ui"
        if (-not (Wait-Listening 1420 60)) {
            throw "网页前端未能在 1420 端口就绪，请查看 output\web-logs\ui.err.log"
        }
    } else {
        Write-Host "· 网页前端已在 127.0.0.1:1420 运行。"
    }

    $secure = Read-Host "请输入局域网访问密码（至少 8 位）" -AsSecureString
    $password = [System.Runtime.InteropServices.Marshal]::PtrToStringUni(
        [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    )
    if ($password.Length -lt 8) {
        throw "访问密码至少需要 8 位。"
    }
    $env:SELF_TOOLS_LAN_PASSWORD = $password
    $password = $null
    [GC]::Collect()

    $port = $env:SELF_TOOLS_LAN_PORT
    if (-not $port) { $port = "1421" }
    Write-Host ""
    Write-Host "局域网网关启动后，在同一 Wi-Fi 下用浏览器打开："
    Write-Host "  http://<这台电脑的局域网IP>:$port"
    Write-Host "数据服务仍只监听 127.0.0.1，未认证端口不会暴露到局域网。"
    Write-Host ""

    & node (Join-Path $PSScriptRoot "lan-gateway.mjs")
}
finally {
    foreach ($proc in $started) {
        if (-not $proc.HasExited) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            Write-Host "· 已停止 $($proc.ProcessName) (PID $($proc.Id))。"
        }
    }
}
