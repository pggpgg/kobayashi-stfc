# Fetch STFCcommunity/data and extract hostiles + ships for KOBAYASHI.
# Run from repo root. Output: data/upstream/stfccommunity-data/{hostiles,ships}/
# See docs/data-acquisition plan: treat upstream as read-only baseline (repo is outdated ~3y).

$ErrorActionPreference = "Stop"
$RepoZip = "https://github.com/STFCcommunity/data/archive/refs/heads/main.zip"
$UpstreamDir = "data/upstream/stfccommunity-data"
$ZipPath = "data/upstream/stfccommunity-data.zip"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $RepoRoot

New-Item -ItemType Directory -Force -Path "data/upstream" | Out-Null

Write-Host "Downloading STFCcommunity/data..."
Invoke-WebRequest -Uri $RepoZip -OutFile $ZipPath -UseBasicParsing

Write-Host "Extracting hostiles and ships..."
$TempExtract = "data/upstream/stfccommunity-data-extract"
if (Test-Path $TempExtract) { Remove-Item -Recurse -Force $TempExtract }
Expand-Archive -Path $ZipPath -DestinationPath $TempExtract -Force

$ArchiveRoot = Get-ChildItem $TempExtract -Directory | Select-Object -First 1
if (-not $ArchiveRoot) { throw "Archive layout unexpected" }
$SrcHostiles = Join-Path $ArchiveRoot.FullName "hostiles"
$SrcShips = Join-Path $ArchiveRoot.FullName "ships"
if (-not (Test-Path $SrcHostiles)) { throw "hostiles/ not found in archive" }
if (-not (Test-Path $SrcShips)) { throw "ships/ not found in archive" }

New-Item -ItemType Directory -Force -Path $UpstreamDir | Out-Null
Copy-Item -Path "$SrcHostiles\*" -Destination $UpstreamDir -Recurse -Force
New-Item -ItemType Directory -Force -Path "$UpstreamDir/ships" | Out-Null
Copy-Item -Path "$SrcShips\*" -Destination "$UpstreamDir/ships" -Recurse -Force

Remove-Item -Recurse -Force $TempExtract
Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue

Write-Host "Done. Upstream data at $UpstreamDir"
Write-Host "Run the normalizer next: cargo run --bin normalize_stfc_data"
