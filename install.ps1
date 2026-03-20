#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repo = 'yukimemi/shun'
$installDir = "$env:LOCALAPPDATA\shun"

Write-Host "Fetching latest release..."
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$version = $release.tag_name

$asset = $release.assets | Where-Object { $_.name -eq 'shun-windows-x64.zip' } | Select-Object -First 1
if (-not $asset) {
    Write-Error "Could not find shun-windows-x64.zip in release $version"
    exit 1
}

$tmp = "$env:TEMP\shun-windows-x64.zip"

Write-Host "Downloading shun $version..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tmp -UseBasicParsing

Write-Host "Installing to $installDir ..."
if (Test-Path $installDir) { Remove-Item $installDir -Recurse -Force }
New-Item -ItemType Directory -Path $installDir | Out-Null
Expand-Archive -Path $tmp -DestinationPath $installDir -Force
Remove-Item $tmp

$exe = "$installDir\shun.exe"
$wsh = New-Object -ComObject WScript.Shell

# Start Menu shortcut
$startMenuDir = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
$shortcut = $wsh.CreateShortcut("$startMenuDir\shun.lnk")
$shortcut.TargetPath = $exe
$shortcut.Save()
Write-Host "Created Start Menu shortcut"

# Startup shortcut (auto-launch on login)
$startupDir = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup"
$shortcut = $wsh.CreateShortcut("$startupDir\shun.lnk")
$shortcut.TargetPath = $exe
$shortcut.Save()
Write-Host "Registered shun to run on startup"

Write-Host ""
Write-Host "shun $version installed successfully!"
Write-Host "shun will start automatically on next login."
Write-Host "To start now: $exe"
