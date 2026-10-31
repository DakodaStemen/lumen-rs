# Third-party notices

Lumen Rust is MIT licensed. Its dependencies retain their own licenses.
`Cargo.lock` records the exact dependency graph.

Core components include:

- mpv / libmpv: LGPL-2.1-or-later **only when built with GPL components disabled**;
  otherwise GPL-2.0-or-later, with GPL dependencies potentially requiring GPLv3.
  Lumen dynamically links libmpv and **bundles it in Windows, macOS, AppImage
  and Flatpak artifacts**. deb/rpm use system packages. See the build-specific
  inventory and redistribution requirements below — https://mpv.io
- libmpv2 (Rust bindings): LGPL-2.1 — https://github.com/kohsine/libmpv-rs
- egui/eframe: MIT OR Apache-2.0 — https://github.com/emilk/egui
- serde/serde_json: MIT OR Apache-2.0 — https://github.com/serde-rs
- sha2: MIT OR Apache-2.0 — https://github.com/RustCrypto/hashes
- fastrand: Apache-2.0 OR MIT — https://github.com/smol-rs/fastrand
- ureq, with rustls: MIT OR Apache-2.0 — https://github.com/algesten/ureq

Programs Lumen runs but does not bundle:

- Streamlink: BSD-2-Clause — https://streamlink.github.io
- yt-dlp: Unlicense — https://github.com/yt-dlp/yt-dlp

SponsorBlock segment data comes from https://sponsor.ajay.app and is licensed
CC BY-NC-SA 4.0 by its contributors.

Rust does not remove licensing obligations for bundled assets, fonts or third-party code.

## Adwaita symbolic icons

`assets/icons/*-symbolic.svg` are unmodified copies of icons from the GNOME
[Adwaita icon theme](https://gitlab.gnome.org/GNOME/adwaita-icon-theme)
(version 51), © the GNOME Project, licensed under
[CC-BY-SA-3.0](https://creativecommons.org/licenses/by-sa/3.0/) or
[LGPL-3.0](https://www.gnu.org/licenses/lgpl-3.0.html). Lumen draws the copies in
the desktop's own icon theme when one is installed and falls back to these.
Recolouring them at run time (white, then tinted) is not a change to the files.

## Bundled libmpv inventory and source

The MIT license covers Lumen's own source, not the entire binary distribution.
Dynamic linking alone does not turn a GPL build into LGPL software.

- **Windows zip/MSI:** `shinchiro/mpv-winbuild-cmake` release `20261021`,
  `mpv-dev-x86_64-20261021-git-e471496c56.7z`, containing `libmpv-2.dll`.
  SHA-256: `efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633`.
  [Release assets/notes](https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261021)
  provide no LGPL-labelled variant and link to
  [build run 39518569492](https://github.com/shinchiro/mpv-winbuild-cmake/actions/runs/39518569492).
  That run uses build scripts at
  [`1f8d2e76255abc48f0456d9be99b4cab9c6566e6`](https://github.com/shinchiro/mpv-winbuild-cmake/tree/1f8d2e76255abc48f0456d9be99b4cab9c6566e6);
  `packages/ffmpeg.cmake` enables `--enable-gpl --enable-version3`.
  Treat this as a **GPL-enabled, not LGPL-only, build** (GPLv3-enabled FFmpeg).
  mpv source revision is `e471496c56` at https://github.com/mpv-player/mpv;
  the build scripts identify the other upstream source repositories. The
  release notes do not establish a complete corresponding-source archive for
  every dependency: that remains a release compliance task, not a verified
  source offer. No verified LGPL alternative was found among this release's assets.
- **macOS DMG:** `brew install mpv` supplies libmpv and `dylibbundler` copies
  its dependencies into `Contents/Frameworks`. This is **not version-pinned**.
  The [Homebrew formula](https://github.com/Homebrew/homebrew-core/blob/HEAD/Formula/m/mpv.rb)
  currently identifies mpv 0.41.0, revision 12, with
  `GPL-2.0-or-later AND LGPL-2.1-or-later`; do not describe it as LGPL-only.
  `brew info --json=v2 mpv ffmpeg` and `brew cat mpv` identify the installed
  versions, source URLs, patches and build recipe; obtain source using those
  URLs and the matching Homebrew/core revision. Release-specific transitive
  dylib versions/licenses still need recording.
- **Linux AppImage:** cargo-packager consumes the Ubuntu runner's
  `libmpv-dev`/`libmpv2` and their dependencies. Neither Ubuntu image nor apt
  versions are pinned, so the exact binary version and aggregate license are
  **not established by the workflow**. Do not assume LGPL-only. Record
  `dpkg-query -W libmpv-dev libmpv2` and retain package copyright files under
  `/usr/share/doc/`; `apt-get source mpv` (with matching `deb-src` repositories
  enabled) retrieves the distro source and patches. Inspect the final AppImage
  inventory before publishing; its actual bundled closure is not verified here.
- **Flatpak:** the checked-in manifest builds mpv **v0.40.0** and FFmpeg
  **n7.1.1**, plus libass 0.17.3 and libplacebo v7.349.0. mpv's GPL default
  is not disabled and FFmpeg uses `--enable-gpl --enable-version3`:
  **GPL-enabled, not LGPL-only**. The manifest contains source repository URLs,
  tags and build options. Fetch those refs to rebuild with `flatpak-builder`;
  tags for git sources are not immutable commit pins. SDK/runtime dependencies
  retain their own licenses/source arrangements.
- **Linux deb/rpm:** libmpv is a system dependency, not bundled. Obtain exact
  version, copyright notices and corresponding source from the distribution's
  package manager/source repositories.

## Replacement, rebuilding and redistribution

Users may replace `libmpv-2.dll` beside the Windows executable, the matching
libmpv dylib and dependencies in macOS `Contents/Frameworks`, or libmpv in an
extracted AppImage (`--appimage-extract`, then `squashfs-root/AppRun`). Preserve
ABI compatibility and install replacement dependencies as needed. For Flatpak,
change the mpv/FFmpeg modules and rebuild the manifest. Build Lumen against a
replacement library with `cargo build --release --locked` (Windows: generate
an MSVC import library as in `scripts/windows-libmpv.ps1`). Lumen source and
`Cargo.lock` are available at https://github.com/DakodaStemen/lumen-rs; use the
artifact's matching release commit. The LGPL Rust bindings are compiled into
Lumen; use the matching locked crate sources from crates.io and rebuild the
application to modify/relink those bindings.

For LGPL-covered components, preserve license/copyright notices, provide the
required corresponding source including modifications/build instructions,
and permit modification and reverse engineering for debugging those changes.
Provide the application source/build materials or other suitable relinking
materials where required; an upstream homepage alone is not a source offer.
See [LGPL 2.1](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html).
For GPL-enabled distributions, review the **combined distribution** obligations,
provide matching complete corresponding source and build scripts under the
applicable GPL terms, and include the relevant license texts. MIT source can
remain MIT, but that does not exempt distributed combined binaries from GPL
requirements. See [mpv copyright](https://github.com/mpv-player/mpv/blob/master/Copyright)
and [FFmpeg legal guidance](https://ffmpeg.org/legal.html).

**Release gate:** this inventory is not a legal compliance certification. Before
publishing bundled artifacts, the release maintainer must capture exact build
and dependency revisions, retain all applicable license/copyright texts, provide
complete matching source/build materials, and review GPL redistribution terms
(or replace the stack with verified LGPL-only builds). The current workflow
does not automate those obligations; source completeness and final artifact
contents remain unverified. No claim of an LGPL-only release is made.
