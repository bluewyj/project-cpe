$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$Version = (Get-Content "VERSION" -Raw).Trim()
$Commit = "unknown"
if (Test-Path ".git") {
    try { $Commit = (git rev-parse --short HEAD).Trim() } catch {}
}
$BuildTime = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$Arch = "aarch64-unknown-linux-gnu"

$ChangelogPath = Join-Path $PSScriptRoot "ota-changelog.txt"
$Changelog = $null
if (Test-Path $ChangelogPath) {
    $Changelog = (Get-Content $ChangelogPath -Raw -Encoding UTF8).Trim()
    if ($Changelog.Length -eq 0) { $Changelog = $null }
}

$BinaryPath = "backend/target/aarch64-unknown-linux-gnu.2.27/release/udx710"
if (-not (Test-Path $BinaryPath)) {
    $BinaryPath = "backend/target/aarch64-unknown-linux-gnu/release/udx710"
}
$FrontendDir = "frontend/dist"

if (-not (Test-Path $BinaryPath)) {
    throw "Backend binary not found: $BinaryPath"
}
if (-not (Test-Path $FrontendDir)) {
    throw "Frontend dist not found: $FrontendDir"
}

$OtaTmp = Join-Path $env:TEMP ("ota_" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $OtaTmp | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $OtaTmp "www") | Out-Null

try {
    Copy-Item $BinaryPath (Join-Path $OtaTmp "udx710") -Force
    Copy-Item "$FrontendDir/*" (Join-Path $OtaTmp "www") -Recurse -Force

    $md5 = [System.Security.Cryptography.MD5]::Create()
    function Get-FileMd5Hex([string]$Path) {
        $bytes = [System.IO.File]::ReadAllBytes($Path)
        return ([BitConverter]::ToString($md5.ComputeHash($bytes)).Replace("-", "").ToLowerInvariant())
    }

    $BinaryMd5 = Get-FileMd5Hex (Join-Path $OtaTmp "udx710")

    $hashes = Get-ChildItem (Join-Path $OtaTmp "www") -Recurse -File | ForEach-Object {
        Get-FileMd5Hex $_.FullName
    } | Sort-Object
    $payload = ($hashes -join "`n")
    if ($payload.Length -gt 0) { $payload += "`n" }
    $FrontendMd5 = ([BitConverter]::ToString($md5.ComputeHash([Text.Encoding]::UTF8.GetBytes($payload))).Replace("-", "").ToLowerInvariant())

    $meta = [ordered]@{
        version = $Version
        commit = $Commit
        build_time = $BuildTime
        binary_md5 = $BinaryMd5
        frontend_md5 = $FrontendMd5
        arch = $Arch
    }
    if ($Changelog) { $meta.changelog = $Changelog }
    $metaJson = $meta | ConvertTo-Json -Depth 3 -Compress:$false
    $metaPath = Join-Path $OtaTmp "meta.json"
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($metaPath, $metaJson + "`n", $utf8NoBom)

    New-Item -ItemType Directory -Force -Path "release" | Out-Null
    $OtaFile = (Resolve-Path "release").Path + "\udx710-ota-$Version.tar.gz"
    if (Test-Path $OtaFile) { Remove-Item $OtaFile -Force }

    Push-Location $OtaTmp
    tar -czf $OtaFile meta.json udx710 www
    Pop-Location

    $OtaMd5 = Get-FileMd5Hex $OtaFile

    Write-Host ""
    Write-Host "OTA package created: release/udx710-ota-$Version.tar.gz"
    Write-Host "Binary MD5: $BinaryMd5"
    Write-Host "Frontend MD5: $FrontendMd5"
    Write-Host "Package MD5: $OtaMd5"
    Get-Item $OtaFile | Format-List FullName, Length, LastWriteTime
    Write-Host "Package contents:"
    tar -tzf $OtaFile | Select-Object -First 20
}
finally {
    if (Test-Path $OtaTmp) { Remove-Item $OtaTmp -Recurse -Force }
}
