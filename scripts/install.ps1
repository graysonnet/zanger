$ErrorActionPreference = "Stop"

$Repo = "graysonnet/zanger"
$InstallDir = "$env:LOCALAPPDATA\zanger"

function Add-PathEntry {
    param(
        [AllowNull()]
        [AllowEmptyString()]
        [string]$PathValue,
        [Parameter(Mandatory = $true)]
        [string]$Directory
    )

    $NormalizedDirectory = $Directory.TrimEnd('\', '/')
    foreach ($Entry in ($PathValue -split ';')) {
        $NormalizedEntry = [Environment]::ExpandEnvironmentVariables($Entry.Trim().Trim('"')).TrimEnd('\', '/')
        if ($NormalizedEntry -ieq $NormalizedDirectory) {
            return $PathValue
        }
    }

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $Directory
    }
    return "$($PathValue.TrimEnd(';'));$Directory"
}

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

# Add to PATH for future terminals without copying the combined process PATH into the user PATH.
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$UpdatedUserPath = Add-PathEntry -PathValue $UserPath -Directory $InstallDir
if ($UpdatedUserPath -cne $UserPath) {
    [Environment]::SetEnvironmentVariable("Path", $UpdatedUserPath, "User")
    Write-Host "Added $InstallDir to your user PATH"
}

# Make the command available immediately when installing with Invoke-Expression.
$env:Path = Add-PathEntry -PathValue $env:Path -Directory $InstallDir

Write-Host "zanger $Tag installed to $InstallDir\zanger.exe"
Write-Host "Run 'zanger' now to get started!"
