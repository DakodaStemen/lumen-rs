# Downloads a libmpv development build for Windows (MSVC), generates mpv.lib and exports
# LIB / LUMEN_LIBMPV_DIR so `cargo build` can link and the DLL can be copied next to the exe.
# Usage: pwsh scripts/windows-libmpv.ps1 [-Dest vendor/libmpv]
param(
  [string]$Dest = "vendor/libmpv"
)
$ErrorActionPreference = "Stop"
# Update tag, exact asset and digest together after reviewing upstream licensing.
# GitHub release asset 660943567 API digest, independently checked on download.
# This is a GPL-enabled build, NOT an LGPL-only build; see THIRD_PARTY_NOTICES.md.
$Tag = "20261021"
$asset = "mpv-dev-x86_64-20261021-git-e471496c56.7z"
$sha256 = "efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633"
New-Item -ItemType Directory -Force $Dest | Out-Null
$Dest = (Resolve-Path $Dest).Path
gh release download $Tag --repo shinchiro/mpv-winbuild-cmake --pattern $asset --dir $Dest --clobber
if ($LASTEXITCODE -ne 0) { throw "libmpv download failed" }
$archive = Get-Item (Join-Path $Dest $asset)
$actual = (Get-FileHash -Algorithm SHA256 $archive.FullName).Hash
if ($actual -ne $sha256) { throw "libmpv SHA-256 mismatch: expected $sha256, got $actual" }
7z x -y "-o$Dest" $archive.FullName | Out-Null
if ($LASTEXITCODE -ne 0) { throw "libmpv extraction failed" }
$dll = Get-ChildItem $Dest -Recurse -Filter "libmpv-2.dll" | Select-Object -First 1
if (-not $dll) { throw "libmpv-2.dll missing from $($archive.Name)" }
# MSVC links against an import library; build one from the .def (exports are the same).
$libexe = (Get-Command lib.exe -ErrorAction SilentlyContinue).Source
if (-not $libexe) {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $libexe = & $vswhere -latest -find "VC\Tools\MSVC\**\bin\Hostx64\x64\lib.exe" | Select-Object -First 1
}
if (-not $libexe) { throw "lib.exe not found; install the MSVC build tools" }
# The archive only ships a MinGW libmpv.dll.a, so derive a .def from the DLL exports.
$dumpbin = Join-Path (Split-Path $libexe) "dumpbin.exe"
$names = & $dumpbin /nologo /exports $dll.FullName | ForEach-Object {
  if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(\S+)') { $Matches[1] }
}
if (-not $names) { throw "no exports found in $($dll.Name)" }
$def = Join-Path $Dest "mpv.def"
@("LIBRARY libmpv-2", "EXPORTS") + $names | Set-Content -Encoding ascii $def
& $libexe /nologo /machine:x64 "/def:$def" "/out:$Dest\mpv.lib"
if ($LASTEXITCODE -ne 0) { throw "lib.exe failed" }
if ((Split-Path $dll.FullName) -ne $Dest) { Copy-Item $dll.FullName $Dest -Force }
if ($env:GITHUB_ENV) {
  "LUMEN_LIBMPV_DIR=$Dest" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
  "LIB=$Dest;$env:LIB" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
  "PATH=$Dest;$env:PATH" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
}
Write-Host "libmpv ready in $Dest"
