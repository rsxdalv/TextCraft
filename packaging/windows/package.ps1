<#
.SYNOPSIS
  Build and package TextCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    textcraft-<version>-windows-x64.msi            per-machine installer (WiX v5)
    textcraft-<version>-windows-x64-portable.zip   textcraft.exe plus the licence and README

  The binary links the C runtime statically (+crt-static), so neither the MSI nor the zip
  needs the Visual C++ redistributable. The executable's icon and version information are
  embedded at build time and checked here.

  Needs: Rust (MSVC toolchain + the x86_64-pc-windows-msvc target), the Windows SDK, and
  WiX v5: dotnet tool install --global wix --version 5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1
  pwsh packaging/windows/package.ps1 -SkipBuild
#>
param(
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
$Version = $env:TEXTCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Target = 'x86_64-pc-windows-msvc'
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "TextCraft $Version for Windows x64 ($Target)"

if (-not $SkipBuild) {
  # Static CRT: no VC++ redistributable needed. Scoped to the target so host build scripts
  # and proc-macros are unaffected.
  $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
  # Fail the build (rather than warn) if the icon and version information cannot be embedded.
  $env:TEXTCRAFT_REQUIRE_WINRES = '1'
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p nc-app --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"
$Exe = Join-Path $Bin 'textcraft.exe'
if (-not (Test-Path $Exe)) { throw "$Exe was not built" }

# Check the PE header before packaging. Machine (COFF header) must be x64; subsystem (optional
# header) must be 2 = Windows GUI, so no console window opens behind the editor.
$bytes = [System.IO.File]::ReadAllBytes($Exe)
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C)
if ($machine -ne 0x8664) { throw "textcraft.exe is for machine 0x$('{0:X}' -f $machine), expected 0x8664 (x64)" }
if ($subsystem -ne 2) { throw "textcraft.exe has PE subsystem $subsystem, expected 2 (Windows GUI)" }
$info = (Get-Item $Exe).VersionInfo
if ($info.ProductName -ne 'TextCraft') { throw "textcraft.exe carries no TextCraft version resource (ProductName='$($info.ProductName)')" }
Write-Output "ok textcraft.exe: x64, GUI subsystem, version resource $($info.ProductVersion)"

$Stage = Join-Path $TargetDir 'windows-package\x64'
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item $Exe $Stage

# ---- MSI ---------------------------------------------------------------------------------------
$Msi = Join-Path $Dist "textcraft-$Version-windows-x64.msi"
Remove-Item -Force $Msi -ErrorAction SilentlyContinue
Invoke-Native 'wix build' {
  wix build (Join-Path $PSScriptRoot 'textcraft.wxs') -arch x64 `
    -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\textcraft.ico')" `
    -o $Msi
}
# wix writes its debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\textcraft-$Version-windows-x64-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage 'textcraft.exe') $Portable
foreach ($f in 'README.md', 'LICENSE') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
$Zip = Join-Path $Dist "textcraft-$Version-windows-x64-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

# Smoke-test the binary: --version runs and exits without opening a window.
$out = & (Join-Path $Stage 'textcraft.exe') --version | Out-String
if ($LASTEXITCODE -ne 0 -or $out -notmatch 'TextCraft') { throw "textcraft --version gave exit code $LASTEXITCODE and output '$out'" }
Write-Output "ok textcraft --version: $($out.Trim())"

Get-Item $Msi, $Zip | Format-Table Name, Length
