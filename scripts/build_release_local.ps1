$ErrorActionPreference = "Stop"

$privateKeyPath = "secrets\tauri-signing.key"
$passwordPath = "secrets\tauri-signing-password.txt"

if (-not (Test-Path -LiteralPath $privateKeyPath)) {
  throw "Missing $privateKeyPath"
}

if (-not (Test-Path -LiteralPath $passwordPath)) {
  throw "Missing $passwordPath"
}

$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw -LiteralPath $privateKeyPath
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content -Raw -LiteralPath $passwordPath

try {
  npm run tauri:build
}
finally {
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
  Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
}

Get-ChildItem -Recurse -File src-tauri\target\release\bundle |
  Select-Object FullName, Length |
  Format-Table -AutoSize

