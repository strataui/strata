# Strata Universal Frontend Engine - CLI Installer for Windows
# Zero Chromium. Pure Rust. Pixel-Perfect CSS & Flexbox/Grid.
#
# Usage:
#   irm https://strata.dev/install.ps1 | iex

$ErrorActionPreference = "Stop"

Write-Host "▲ Installing Strata Universal Frontend Engine..."
$Arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }

$InstallDir = "$Home\.cargo\bin"
if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Write-Host "  › Target: windows-$Arch"
Write-Host "  › Destination: $InstallDir"

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "  › Compiling and installing strata via cargo..."
    cargo install strata_cli --bin strata --quiet
}

$StrataExe = Join-Path $InstallDir "strata.exe"
if (Test-Path $StrataExe) {
    Write-Host "  √ Strata CLI successfully installed to $StrataExe"
    Write-Host "  › Run 'strata new' to start your first project!"
} else {
    Write-Host "  › Please ensure $InstallDir is in your User PATH."
}
