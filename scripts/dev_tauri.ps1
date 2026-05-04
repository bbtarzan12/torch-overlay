param(
  [switch]$OpenDevTools
)

$ErrorActionPreference = "Stop"

$env:RUST_BACKTRACE = "1"
$env:RUST_LOG = "debug"

if ($OpenDevTools) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--auto-open-devtools-for-tabs"
}

npm run tauri:dev

