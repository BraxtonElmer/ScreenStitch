# Builds ScreenStitch and puts a ready-to-run copy in dist/.
#   ./scripts/build.ps1
# Needs Rust (GNU or MSVC toolchain) and Node.js on PATH.

$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

function Step([scriptblock]$run) {
    & $run
    if ($LASTEXITCODE -ne 0) { Write-Host 'Build failed.' -ForegroundColor Red; exit $LASTEXITCODE }
}

Push-Location settings
if (-not (Test-Path node_modules)) { Step { npm ci } }
Step { npm run build }
Pop-Location

Step { cargo build --release -p screenstitch -p screenstitch-settings }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item target/release/screenstitch.exe (Join-Path $dist 'ScreenStitch.exe') -Force
Copy-Item target/release/screenstitch-settings.exe $dist -Force
# Only present with the GNU toolchain; MSVC builds link WebView2 statically.
if (Test-Path target/release/WebView2Loader.dll) { Copy-Item target/release/WebView2Loader.dll $dist -Force }

Write-Host "Done: $dist\ScreenStitch.exe"
