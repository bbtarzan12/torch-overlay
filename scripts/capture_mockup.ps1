param(
  [string]$InputPath = "mockups/tli-tracker-mockup.html",
  [string]$OutputDir = "artifacts/mockups",
  [int]$Width = 1500,
  [int]$Height = 665,
  [switch]$Open
)

$ErrorActionPreference = "Stop"

function Find-Browser {
  $candidates = @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
    "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
  )

  foreach ($candidate in $candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate)) {
      return $candidate
    }
  }

  throw "Chrome or Edge executable was not found."
}

function Capture-Page {
  param(
    [string]$Browser,
    [string]$Url,
    [string]$OutputPath,
    [int]$Width,
    [int]$Height
  )

  $profileDir = Join-Path ([System.IO.Path]::GetTempPath()) ("tli-tracker-capture-" + [System.Guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $profileDir | Out-Null

  try {
    $arguments = @(
      "--headless=new",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      "--user-data-dir=$profileDir",
      "--window-size=$Width,$Height",
      "--screenshot=$OutputPath",
      $Url
    )

    $process = Start-Process -FilePath $Browser -ArgumentList $arguments -NoNewWindow -Wait -PassThru
    if ($process.ExitCode -ne 0) {
      throw "Browser screenshot failed with exit code $($process.ExitCode)."
    }
  }
  finally {
    Remove-Item -LiteralPath $profileDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}

$browser = Find-Browser
$resolvedInput = Resolve-Path -LiteralPath $InputPath
$resolvedOutputDir = New-Item -ItemType Directory -Force -Path $OutputDir
$fileUrl = ([System.Uri]$resolvedInput.Path).AbsoluteUri

$barOutput = Join-Path $resolvedOutputDir.FullName "tli-tracker-bar.png"
$detailsOutput = Join-Path $resolvedOutputDir.FullName "tli-tracker-details.png"
$cumulativeOutput = Join-Path $resolvedOutputDir.FullName "tli-tracker-details-cumulative.png"
$plainOutput = Join-Path $resolvedOutputDir.FullName "tli-tracker-details-plain.png"

Capture-Page -Browser $browser -Url "${fileUrl}?preview=1" -OutputPath $barOutput -Width $Width -Height $Height
Capture-Page -Browser $browser -Url "${fileUrl}?preview=1&details=1" -OutputPath $detailsOutput -Width $Width -Height $Height
Capture-Page -Browser $browser -Url "${fileUrl}?preview=1&details=1&chart=cumulative" -OutputPath $cumulativeOutput -Width $Width -Height $Height
Capture-Page -Browser $browser -Url "${fileUrl}?details=1" -OutputPath $plainOutput -Width $Width -Height $Height

[pscustomobject]@{
  Browser = $browser
  BarScreenshot = $barOutput
  DetailsScreenshot = $detailsOutput
  CumulativeScreenshot = $cumulativeOutput
  PlainDetailsScreenshot = $plainOutput
  Viewport = "${Width}x${Height}"
} | Format-List

if ($Open) {
  Invoke-Item -LiteralPath $detailsOutput
}
