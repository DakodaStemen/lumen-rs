# Downloads a libmpv development build for Windows (MSVC), generates mpv.lib and exports
# LIB / LUMEN_LIBMPV_DIR so `cargo build` can link and the DLL can be copied next to the exe.
# Usage: pwsh scripts/windows-libmpv.ps1 [-Tag 20261021] [-Dest vendor/libmpv]
param(
  [string]$Tag = "20261021",
  [string]$Dest = "vendor/libmpv"
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $Dest | Out-Null
$Dest = (Resolve-Path $Dest).Path
$asset = "mpv-dev-x86_64-$Tag-git-*.7z"
gh release download $Tag --repo shinchiro/mpv-winbuild-cmake --pattern $asset --dir $Dest --clobber
$archive = Get-ChildItem $Dest -Filter "mpv-dev-x86_64-*.7z" | Select-Object -First 1
7z x -y "-o$Dest" $archive.FullName | Out-Null
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
