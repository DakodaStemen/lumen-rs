# Packaging

| OS | Artifacts | libmpv |
| --- | --- | --- |
| Windows | `.zip` (portable), `.msi` (WiX via cargo-packager) | `libmpv-2.dll` from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake) is copied next to `lumen-rs.exe` |
| macOS | `.dmg` | `libmpv.dylib` plus dependencies bundled into `Lumen.app/Contents/Frameworks` with `dylibbundler` |
| Linux | `.deb`, `.rpm`, AppImage, Flatpak | deb/rpm depend on the distro's libmpv (`libmpv2`, `mpv-libs`); AppImage and Flatpak carry their own |

Everything is built by `.github/workflows/release.yml` (manual run, or a `v*` tag). Local builds:

```sh
cargo install cargo-packager --locked
cargo packager --release --formats deb        # or appimage / app / wix
cargo install cargo-generate-rpm && cargo build --release && cargo generate-rpm
flatpak-builder --user --install --force-clean build packaging/flatpak/io.github.DakodaStemen.Lumen.yml
```

## Not done / needs credentials

* **Windows signing**: needs an Authenticode certificate (OV/EV or Azure Trusted Signing). Installers are unsigned, so SmartScreen will warn.
* **macOS signing and notarization**: needs an Apple Developer ID certificate and notarytool credentials. Unsigned dmgs need right-click, Open. Only the runner's architecture (arm64) is built.
* **Flatpak**: the manifest is a draft; the mpv dependency modules (ffmpeg, libplacebo, libass) are not filled in, and Flathub needs vendored cargo sources.
* **MPRIS / media keys / OS media controls**: Lumen never had these, so nothing was ported. They would be a new feature (`souvlaki` covers MPRIS, Windows SMTC and macOS Now Playing).
* **macOS "open with"**: Finder delivers files through Apple events, not argv, so opening a file from Finder starts Lumen without loading it. Drag-and-drop onto the window works.
* The streams integration tests use POSIX shell scripts as fake tools and run on Unix only.
