# H.264 / AAC (proprietary codecs)

The standard CEF builds that `scripts/setup-cef.sh` downloads (from
cef-builds.spotifycdn.com) are built **without** proprietary codecs. Ferrite
then plays WebM/VP9/AV1/Opus/MP3 but not MP4 with H.264/AAC
(`DEMUXER_ERROR_NO_SUPPORTED_STREAMS`). Sites that only serve H.264 video
won't play. YouTube normally serves VP9/AV1 and works. DRM (Widevine) is a
separate topic and is not covered here.

Check what your build plays at `ferrite://about/` ("Media codecs").

Ferrite's own code needs no change: codecs live inside `libcef.so` (Chromium's
bundled FFmpeg). You only build Ferrite against a different CEF.

## Option A: build CEF yourself (recommended)

```sh
scripts/setup-cef.sh            # once: tells the script which CEF commit to build
scripts/build-cef-codecs.sh     # builds and installs into ~/.local/share/cef-codecs
CEF_PATH=$HOME/.local/share/cef-codecs cargo build --release
CEF_PATH=$HOME/.local/share/cef-codecs scripts/bundle.sh
```

- It builds the **same** CEF commit the pinned `cef` crate uses (read from
  `Cargo.lock` and the standard download's `archive.json`, e.g. CEF branch
  8078 at `8ea0c73` = CEF 156.0.26, Chromium 156.0.8078.88). It uses CEF's
  official `tools/automate/automate-git.py` with
  `GN_DEFINES="is_official_build=true proprietary_codecs=true ffmpeg_branding=Chrome use_sysroot=true symbol_level=0 chrome_pgo_phase=0"`
  and `--minimal-distrib-only --no-debug-build --x64-build`.
- Requirements, which are Chromium's: Linux x86_64, about 120 GB free disk,
  16 GB RAM minimum (32 GB recommended), and a fast network for about 30 GB of
  sources. Expect about 4 h on a 16-core machine and a day or more on 4 cores.
  On the first run, install Chromium's build dependencies when automate-git
  asks: `sudo <work>/chromium_git/chromium/src/build/install-build-deps.sh --no-arm --no-nacl`.
- Options: `--work DIR` (big disk), `--out DIR`, `--jobs N`, `--dry-run`
  (print automate-git's plan only).
- `--install-only DIR` installs an already built `cef_binary_*` directory into
  the flat layout cef-rs expects. It also writes `archive.json`, using the
  version taken from `include/cef_version.h`.

**What was tested:** the version detection, the dry run up to automate-git's
first real checkout step (CEF branch 8078 and commit `8ea0c73` resolved
correctly), and `--install-only` on a real codec-enabled CEF 8078
distribution, which produced the expected layout and `archive.json`. A full
Chromium build was **not** run: the development container has 12 GB of free
disk and 4 cores.

## Option B: a third-party prebuilt (not recommended for daily use)

The only maintained Linux x64 codec build found is **obs-rust/cef, release
8078** (`cef_binary_8078_linux_x86_64.tar.xz`, sha256
`fb36df11bc8280c9cb666732becc1fe267f833bb964dbb13d69cad33488171dc`). It was
built for OBS with `proprietary_codecs=true ffmpeg_branding=Chrome`.

- It is **CEF 156.0.8 / Chromium 156.0.8078.48**, older than Ferrite's pinned
  156.0.26 / 156.0.8078.88, so it **lacks the Chromium security fixes in
  between**. A browser runs untrusted web content, so this matters.
- It comes from a small third-party organisation with unknown provenance and
  update cadence. Verify the sha256 and pin it.
- The API version (15600) matches. `build-cef-codecs.sh --install-only` accepts
  it and warns about the version. Ferrite was **not** run against it here,
  because the container's safety policy blocks executing that third-party
  binary.

```sh
curl -LO https://github.com/obs-rust/cef/releases/download/8078/cef_binary_8078_linux_x86_64.tar.xz
echo "fb36df11bc8280c9cb666732becc1fe267f833bb964dbb13d69cad33488171dc  cef_binary_8078_linux_x86_64.tar.xz" | sha256sum -c -
tar -xJf cef_binary_8078_linux_x86_64.tar.xz
scripts/build-cef-codecs.sh --install-only cef_binary_8078_linux_x86_64
CEF_PATH=$HOME/.local/share/cef-codecs cargo build --release
```

## Licensing

H.264 and AAC are covered by patent pools (Via LA, formerly MPEG LA, and
others). CEF disables them by default for this reason. See the CEF maintainer
in https://github.com/chromiumembedded/cef/issues/3559: the issue is FFmpeg
software decoding with `ffmpeg_branding=Chrome`.

- **Building for your own personal use** is the common case and is generally
  considered low risk.
- **Redistributing binaries** (sharing a bundle, packaging it) may require
  patent licences. Linux distributions leave these codecs out of their
  Chromium for this reason; Fedora uses Cisco's OpenH264 instead.

This is not legal advice. If you plan to share builds, check the terms
yourself.

Alternatives considered: `use_system_ffmpeg` is not supported for CEF on
Linux (not verified in depth). Chromium's OpenH264 integration is for WebRTC
encoding and does not provide `<video>` H.264 or AAC playback.
