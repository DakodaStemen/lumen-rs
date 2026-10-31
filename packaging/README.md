# Packaging

| OS | Artifacts | libmpv |
| --- | --- | --- |
| Windows | `.zip` (portable), `.msi` (WiX via cargo-packager) | `libmpv-2.dll` from shinchiro release `20261021`, exact asset `mpv-dev-x86_64-20261021-git-e471496c56.7z`; **GPL-enabled**, no LGPL variant listed in that release |
| macOS | `.dmg` | Homebrew `mpv` (floating; reviewed formula: 0.41.0 revision 12, GPL-2.0-or-later AND LGPL-2.1-or-later), plus dependencies copied to `Contents/Frameworks` |
| Linux | `.deb`, `.rpm`, AppImage, Flatpak | deb/rpm use distro libmpv; AppImage uses Ubuntu runner packages (floating, exact closure/license unverified); Flatpak builds mpv v0.40.0 + FFmpeg n7.1.1 with **GPL enabled**, including FFmpeg version3 |

## Bundled-library licensing and source/relinking

Lumen's source is MIT; bundled binaries are **not necessarily MIT-only or
LGPL-only**. Dynamic linking does not remove GPL obligations. The Windows
archive is verified before extraction with SHA-256
`efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633`
(GitHub release asset 660943567 API digest, also independently hashed after
`gh release download`). Change the tag, exact asset name and digest together.

[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md#bundled-libmpv-inventory-and-source)
records platform-specific provenance, licenses, source locations, replacement
and rebuild instructions, and the unresolved corresponding-source inventory.
Users can replace the DLL next to the Windows executable, dylibs in
`Contents/Frameworks`, libraries in an extracted AppImage, or rebuild Flatpak's
source modules; replacements must preserve ABI compatibility. Rebuild Lumen
from the matching release commit and locked crates to relink the LGPL Rust
bindings. Distro package sources come from matching `apt-get source mpv` /
distro source repositories; Homebrew source URLs/patches come from the matching
formula revision; Flatpak source URLs/tags/options are in the manifest.

**Before publishing:** retain matching source, patches, build scripts and all
license/copyright notices for the complete bundled dependency graph, review
GPL combined-distribution requirements, and provide the necessary relinking
materials for LGPL components. Upstream links alone are not a complete source
offer. The workflows do not currently capture all these materials; this is an
explicit release-maintainer gate, not a claim that existing bundles comply.

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
