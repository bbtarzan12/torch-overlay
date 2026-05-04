param(
  [string]$OldVersion = "0.1.1",
  [string]$NewVersion = "0.1.2",
  [int]$Port = 7531,
  [string]$OutputDir = "artifacts\update-test"
)

$ErrorActionPreference = "Stop"

if ([version]$NewVersion -le [version]$OldVersion) {
  throw "NewVersion must be greater than OldVersion."
}

$productName = "Torch Overlay Update Test"
$identifier = "kr.tli.torch-overlay.update-test"
$commit = (git rev-parse --short HEAD).Trim()
$rootOutput = New-Item -ItemType Directory -Force -Path $OutputDir
$keysDir = New-Item -ItemType Directory -Force -Path (Join-Path $rootOutput.FullName "keys")
$certsDir = New-Item -ItemType Directory -Force -Path (Join-Path $rootOutput.FullName "certs")
$oldDir = New-Item -ItemType Directory -Force -Path (Join-Path $rootOutput.FullName "old")
$serverDir = New-Item -ItemType Directory -Force -Path (Join-Path $rootOutput.FullName "server")

$privateKeyPath = Join-Path $keysDir.FullName "update-test.key"
$publicKeyPath = "$privateKeyPath.pub"

if (-not (Test-Path -LiteralPath $privateKeyPath) -or -not (Test-Path -LiteralPath $publicKeyPath)) {
  npm run tauri -- signer generate -w $privateKeyPath --ci
}

$publicKey = (Get-Content -LiteralPath $publicKeyPath -Raw).Trim()
$certPfxPath = Join-Path $certsDir.FullName "localhost.pfx"
$certCerPath = Join-Path $certsDir.FullName "localhost.cer"
$certPassword = "torch-overlay-update-test"

if (-not (Test-Path -LiteralPath $certPfxPath) -or -not (Test-Path -LiteralPath $certCerPath)) {
  $rsa = [System.Security.Cryptography.RSA]::Create(2048)
  $request = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
    "CN=localhost",
    $rsa,
    [System.Security.Cryptography.HashAlgorithmName]::SHA256,
    [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
  )
  $sanBuilder = [System.Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new()
  $sanBuilder.AddDnsName("localhost")
  $request.CertificateExtensions.Add($sanBuilder.Build())
  $request.CertificateExtensions.Add(
    [System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($false, $false, 0, $false)
  )
  $request.CertificateExtensions.Add(
    [System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new(
      [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature -bor
        [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::KeyEncipherment,
      $false
    )
  )

  $cert = $request.CreateSelfSigned((Get-Date).AddDays(-1), (Get-Date).AddYears(3))
  [System.IO.File]::WriteAllBytes(
    $certPfxPath,
    $cert.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx, $certPassword)
  )
  [System.IO.File]::WriteAllBytes(
    $certCerPath,
    $cert.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
  )
}

certutil -user -addstore Root $certCerPath | Out-Null

$endpoint = "https://localhost:$Port/latest.json"

function New-UpdateTestConfig {
  param(
    [string]$Version,
    [bool]$CreateUpdaterArtifacts
  )

  @{
    productName = $productName
    version = $Version
    identifier = $identifier
    app = @{
      windows = @(
        @{
          title = $productName
          label = "main"
          width = 1404
          height = 46
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
        pubkey = $publicKey
        endpoints = @($endpoint)
        windows = @{
          installMode = "passive"
        }
      }
    }
    bundle = @{
      active = $true
      targets = "nsis"
      createUpdaterArtifacts = $CreateUpdaterArtifacts
    }
  }
}

function Invoke-UpdateTestBuild {
  param(
    [string]$Version,
    [bool]$CreateUpdaterArtifacts
  )

  $tempConfigDir = New-Item -ItemType Directory -Force -Path (Join-Path ([System.IO.Path]::GetTempPath()) ("torch-overlay-update-test-config-" + [System.Guid]::NewGuid().ToString("N")))
  $tempConfigPath = Join-Path $tempConfigDir.FullName "tauri.update-test.conf.json"
  New-UpdateTestConfig -Version $Version -CreateUpdaterArtifacts $CreateUpdaterArtifacts |
    ConvertTo-Json -Depth 20 |
    Set-Content -LiteralPath $tempConfigPath -Encoding UTF8

  $previousPrivateKey = $env:TAURI_SIGNING_PRIVATE_KEY
  $previousPrivateKeyPath = $env:TAURI_SIGNING_PRIVATE_KEY_PATH
  $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -LiteralPath $privateKeyPath -Raw).Trim()
  Remove-Item Env:\TAURI_SIGNING_PRIVATE_KEY_PATH -ErrorAction SilentlyContinue

  try {
    npm run tauri -- build --config $tempConfigPath --ci
  }
  finally {
    if ($null -eq $previousPrivateKey) {
      Remove-Item Env:\TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    } else {
      $env:TAURI_SIGNING_PRIVATE_KEY = $previousPrivateKey
    }

    if ($null -eq $previousPrivateKeyPath) {
      Remove-Item Env:\TAURI_SIGNING_PRIVATE_KEY_PATH -ErrorAction SilentlyContinue
    } else {
      $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $previousPrivateKeyPath
    }

    Remove-Item -LiteralPath $tempConfigDir.FullName -Recurse -Force -ErrorAction SilentlyContinue
  }

  $bundleDir = "src-tauri\target\release\bundle\nsis"
  $installer = Get-ChildItem -LiteralPath $bundleDir -Filter "$productName`_${Version}_x64-setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

  if (-not $installer) {
    throw "Installer was not found for $productName $Version under $bundleDir"
  }

  [pscustomobject]@{
    Version = $Version
    Installer = $installer.FullName
    Signature = "$($installer.FullName).sig"
  }
}

$oldBuild = Invoke-UpdateTestBuild -Version $OldVersion -CreateUpdaterArtifacts $false
$newBuild = Invoke-UpdateTestBuild -Version $NewVersion -CreateUpdaterArtifacts $true

if (-not (Test-Path -LiteralPath $newBuild.Signature)) {
  throw "Updater signature was not generated: $($newBuild.Signature)"
}

$oldInstallerPath = Join-Path $oldDir.FullName "Torch.Overlay.Update.Test_${OldVersion}_x64-setup.exe"
$updateInstallerPath = Join-Path $serverDir.FullName "update-x64-setup.exe"
$signaturePath = Join-Path $serverDir.FullName "update-x64-setup.exe.sig"

Copy-Item -LiteralPath $oldBuild.Installer -Destination $oldInstallerPath -Force
Copy-Item -LiteralPath $newBuild.Installer -Destination $updateInstallerPath -Force
Copy-Item -LiteralPath $newBuild.Signature -Destination $signaturePath -Force

$signature = (Get-Content -LiteralPath $signaturePath -Raw).Trim()
$updateUrl = "https://localhost:$Port/update-x64-setup.exe"
$platform = [ordered]@{
  signature = $signature
  url = $updateUrl
}
$latest = [ordered]@{
  version = $NewVersion
  notes = "Local updater test build from $commit."
  pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64-nsis" = $platform
    "windows-x86_64" = $platform
  }
}
$latestPath = Join-Path $serverDir.FullName "latest.json"
$latest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $latestPath -Encoding UTF8

$metadata = [ordered]@{
  localOnly = $true
  productName = $productName
  identifier = $identifier
  oldVersion = $OldVersion
  newVersion = $NewVersion
  commit = $commit
  endpoint = $endpoint
  publicKey = $publicKey
  privateKeyPath = $privateKeyPath
  certPfxPath = $certPfxPath
  certCerPath = $certCerPath
  certPassword = $certPassword
  oldInstaller = $oldInstallerPath
  updateInstaller = $updateInstallerPath
  latestJson = $latestPath
  serverCommand = "npm run update:test:server"
  installOldCommand = "npm run update:test:install-old"
}
$metadataPath = Join-Path $rootOutput.FullName "update-test.json"
$metadata | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $metadataPath -Encoding UTF8

Get-ChildItem -LiteralPath $rootOutput.FullName, $oldDir.FullName, $serverDir.FullName |
  Select-Object FullName, Length, LastWriteTime |
  Format-Table -AutoSize
