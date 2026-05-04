param(
  [string]$OutputDir = "artifacts\local-build"
)

$ErrorActionPreference = "Stop"

$package = Get-Content -Raw -LiteralPath "package.json" | ConvertFrom-Json
$version = [string]$package.version
$commit = (git rev-parse --short HEAD).Trim()
$dirty = (git status --short).Trim().Length -gt 0
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$buildId = if ($dirty) { "$commit-dirty-$timestamp" } else { "$commit-$timestamp" }

$localProductName = "Torch Overlay Local"
$localIdentifier = "kr.tli.torch-overlay.local"

$tauriOverride = @{
  productName = $localProductName
  identifier = $localIdentifier
  app = @{
    windows = @(
      @{
        title = $localProductName
        label = "main"
        width = 1380
        height = 420
        decorations = $false
        transparent = $true
        alwaysOnTop = $true
        resizable = $false
        skipTaskbar = $false
      }
    )
    security = @{
      csp = $null
    }
  }
  plugins = @{
    updater = @{
      pubkey = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDE4QTI3RUJDODg4NUQ2QTcKUldTbjFvV0l2SDZpR05ncmRrbTRzN2FINW1vTkxEeEw2NlE5bTJXaXJiYUlMR1lMOWRCUk4rL28K"
      endpoints = @("http://127.0.0.1:9/torch-overlay-local/latest.json")
      windows = @{
        installMode = "passive"
      }
    }
  }
  bundle = @{
    active = $true
    targets = "nsis"
    createUpdaterArtifacts = $false
  }
}

$tempConfigDir = New-Item -ItemType Directory -Force -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("torch-overlay-local-config-" + [System.Guid]::NewGuid().ToString("N")))
$tempConfigPath = Join-Path $tempConfigDir.FullName "tauri.local.conf.json"
$tauriOverride | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $tempConfigPath -Encoding UTF8

try {
  npm run tauri -- build --config $tempConfigPath --ci
}
finally {
  Remove-Item -LiteralPath $tempConfigDir.FullName -Recurse -Force -ErrorAction SilentlyContinue
}

$bundleDir = "src-tauri\target\release\bundle\nsis"
$installer = Get-ChildItem -LiteralPath $bundleDir -Filter "Torch Overlay Local_*_x64-setup.exe" |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $installer) {
  throw "Local installer was not found under $bundleDir"
}

$resolvedOutputDir = New-Item -ItemType Directory -Force -Path $OutputDir
$localInstallerName = "Torch.Overlay.Local_${version}_${buildId}_x64-setup.exe"
$localInstallerPath = Join-Path $resolvedOutputDir.FullName $localInstallerName

Copy-Item -LiteralPath $installer.FullName -Destination $localInstallerPath -Force

$metadata = [ordered]@{
  localOnly = $true
  productName = $localProductName
  identifier = $localIdentifier
  version = $version
  commit = $commit
  dirty = $dirty
  buildId = $buildId
  builtAt = (Get-Date).ToString("o")
  sourceInstaller = $installer.FullName
  localInstaller = $localInstallerPath
  updaterArtifacts = $false
  releaseSafe = "This installer uses a local-only product name and identifier, and is copied under artifacts/local-build."
}

$metadataPath = Join-Path $resolvedOutputDir.FullName "Torch.Overlay.Local_${version}_${buildId}.json"
$metadata | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $metadataPath -Encoding UTF8

Get-ChildItem -LiteralPath $resolvedOutputDir.FullName -Filter "Torch.Overlay.Local_${version}_${buildId}*" |
  Select-Object FullName, Length |
  Format-Table -AutoSize
