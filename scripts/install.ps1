$ErrorActionPreference = "Stop"

$Repo = "graysonnet/zanger"
$InstallDir = "$env:LOCALAPPDATA\zanger"

Write-Host "Installing zanger..."

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($Arch) {
    "X64"   { $Asset = "zanger-windows-x86_64.zip" }
    "Arm64" { $Asset = "zanger-windows-arm64.zip" }
    default { Write-Error "Unsupported architecture: $Arch"; exit 1 }
}

$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name

if (-not $Tag) {
    Write-Error "Could not determine latest release"
    exit 1
}

Write-Host "Downloading $Asset ($Tag)..."

$TmpDir = New-Item -ItemType Directory -Path "$env:TEMP\zanger-install" -Force
$ZipPath = "$TmpDir\$Asset"

Invoke-WebRequest -Uri "https://github.com/$Repo/releases/download/$Tag/$Asset" -OutFile $ZipPath

Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Move-Item -Path "$TmpDir\zanger.exe" -Destination "$InstallDir\zanger.exe" -Force

Remove-Item -Recurse -Force $TmpDir

# Add to PATH if not already there
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to your PATH (restart your terminal to apply)"
}

Write-Host "zanger $Tag installed to $InstallDir\zanger.exe"
Write-Host "Run 'zanger' to get started!"
