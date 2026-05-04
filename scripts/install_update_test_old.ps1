param(
  [string]$MetadataPath = "artifacts\update-test\update-test.json",
  [switch]$NoLaunch
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $MetadataPath)) {
  throw "Update test metadata was not found. Run npm run update:test:build first."
}

$metadata = Get-Content -Raw -LiteralPath $MetadataPath | ConvertFrom-Json
$installer = [string]$metadata.oldInstaller
$installDir = Join-Path $env:LOCALAPPDATA $metadata.productName
$exe = Join-Path $installDir "torch-overlay.exe"
$uninstaller = Join-Path $installDir "uninstall.exe"

if (-not (Test-Path -LiteralPath $installer)) {
  throw "Old installer was not found: $installer"
}

Get-Process -Name "torch-overlay" -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -and $_.Path.StartsWith($installDir, [System.StringComparison]::OrdinalIgnoreCase) } |
  Stop-Process -Force

Start-Sleep -Milliseconds 500

if (Test-Path -LiteralPath $uninstaller) {
  Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -WindowStyle Hidden
  Start-Sleep -Seconds 1
}

Start-Process -FilePath $installer -ArgumentList "/S" -Wait -WindowStyle Hidden

if (-not (Test-Path -LiteralPath $exe)) {
  throw "Installed executable was not found: $exe"
}

$item = Get-Item -LiteralPath $exe
$process = $null

if (-not $NoLaunch) {
  $process = Start-Process -FilePath $exe -PassThru
  Start-Sleep -Seconds 2
}

[pscustomobject]@{
  ProductName = $metadata.productName
  InstalledVersion = $metadata.oldVersion
  UpdateVersion = $metadata.newVersion
  Endpoint = $metadata.endpoint
  Exe = $item.FullName
  LastWriteTime = $item.LastWriteTime
  StartedPid = if ($process) { $process.Id } else { $null }
} | Format-List
