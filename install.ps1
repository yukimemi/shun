#Requires -Version 5.1
$ErrorActionPreference = 'Stop'

$repo = 'yukimemi/shun'
$installDir = "$env:LOCALAPPDATA\shun"

Write-Host "Fetching latest release..."
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
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

# Add to user PATH (no admin required)
$userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable('PATH', "$userPath;$installDir", 'User')
    Write-Host "Added $installDir to user PATH"
}

Write-Host ""
Write-Host "shun $version installed successfully!"
Write-Host "Restart your terminal, then run: shun"
