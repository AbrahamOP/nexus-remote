# Nexus Remote — Third-party notices

This release uses open-source libraries and embedded fonts. Their original copyright notices and license texts are reproduced in **THIRD_PARTY_LICENSES.txt**, which must accompany redistributions of this application.

The inventory was generated from this release's Cargo.lock and the actual published package sources. It records 382 registry packages across all platforms, including optional dependencies and tools used only during compilation. The Windows normal/build dependency graph contains 161 third-party packages. Full standalone or original upstream license texts are present for every package in that Windows graph; optional packages for other targets are listed for source-resolution traceability.

## Main components

| Component | Locked version | License declared by package |
| --- | --- | --- |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| eframe | 0.33.3 | MIT OR Apache-2.0 |
| egui | 0.33.3 | MIT OR Apache-2.0 |
| epaint_default_fonts | 0.33.3 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 |
| libssh2-sys | 0.3.4 | MIT OR Apache-2.0 |
| libz-sys | 1.1.33 | MIT OR Apache-2.0 |
| openssl-sys | 0.9.118 | MIT |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| rfd | 0.15.4 | MIT |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| ssh2 | 0.9.7 | MIT OR Apache-2.0 |
| uuid | 1.29.2 | Apache-2.0 OR MIT |
| vt100 | 0.15.2 | MIT |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| winresource | 0.1.34 | MIT |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |

- **eframe / egui**: native application framework and GUI, by Emil Ernerfeldt and contributors. Source and documentation: https://github.com/emilk/egui . The upstream MIT and Apache-2.0 files from the exact published source commit are included.
- **libssh2**, vendored by libssh2-sys, supplies SSH/SFTP transport. Its BSD-3-Clause COPYING file and 25 distinct complete copyright/license blocks from its native source headers are reproduced. Upstream: https://www.libssh2.org/ . The source snapshot identifies itself as libssh2 1.11.2_DEV.
- **OpenSSL 3.5.9 LTS**, built separately from the official source archive and linked statically, provides cryptography for SSH. The archive SHA-256 is 603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a. Its original Apache License 2.0 text is included in assets/licenses/OpenSSL-3.5.9-LICENSE.txt and in THIRD_PARTY_LICENSES.txt. Upstream: https://www.openssl-library.org/source/ . No separate Perl interpreter is distributed with Nexus Remote.
- **zlib 1.4**, vendored by libz-sys, is copyright (C) 1995–2026 Jean-loup Gailly and Mark Adler. Its complete zlib.h copyright and license block is included. Upstream: https://zlib.net/ .
- **Khronos registry data** used to generate graphics bindings: 6 distinct original XML copyright/license notices are reproduced alongside the generator's full Apache-2.0 license.
- **Rust standard library**: the exact compiler-distributed library copyright manifest is preserved in RUST_LIBRARY_COPYRIGHT.html, including its transitive runtime-component licenses. This covers the statically included Rust library separately from the application's Cargo packages.
- **Windows RDP** uses the Remote Desktop and ATL components already installed with Windows. These operating-system components are not included in the application package and remain subject to Microsoft's own terms.

## Embedded fonts

Default fonts come from epaint_default_fonts 0.33.3 and are embedded without modification. The complete supplied notices are included:

- **Hack Regular**: Source Foundry authors (MIT), DejaVu contributions (public domain), and Bitstream Vera origins (Bitstream Vera license). See fonts/Hack-Regular.txt.
- **Noto Emoji Regular**: Copyright 2013 Google Inc., SIL Open Font License 1.1, fonts/OFL.txt.
- **Ubuntu Light**: Copyright 2011 Canonical Ltd., Ubuntu Font Licence 1.0, fonts/UFL.txt.
- **emoji-icon-font**: Copyright (c) 2014 John Slegers, MIT, fonts/emoji-icon-font-mit-license.txt.

The appendix in THIRD_PARTY_LICENSES.txt also preserves the copyright, trademark, manufacturer and license records read directly from each font's embedded OpenType name table. Font source: https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts .

## Complete resolved-package inventory

Windows / build identifies packages in cargo tree --target x86_64-pc-windows-msvc --edges normal,build. Other / optional entries are target-specific or inactive for this executable; listing them does not imply that they are linked or distributed.

| Package | Version | Declared license | Dependency scope |
| --- | --- | --- | --- |
| ab_glyph | 0.2.32 | Apache-2.0 | Windows / build |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | Windows / build |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | Windows / build |
| ahash | 0.8.12 | MIT OR Apache-2.0 | Windows / build |
| android-activity | 0.6.1 | MIT OR Apache-2.0 | Other / optional |
| android-properties | 0.2.2 | MIT | Other / optional |
| arboard | 3.7.0 | MIT OR Apache-2.0 | Windows / build |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | Windows / build |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | Other / optional |
| ashpd | 0.11.1 | MIT | Other / optional |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | Other / optional |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | Other / optional |
| async-executor | 1.14.1 | Apache-2.0 OR MIT | Other / optional |
| async-fs | 2.2.0 | Apache-2.0 OR MIT | Other / optional |
| async-io | 2.6.0 | Apache-2.0 OR MIT | Other / optional |
| async-lock | 3.5.0 | Apache-2.0 OR MIT | Other / optional |
| async-net | 2.0.0 | Apache-2.0 OR MIT | Other / optional |
| async-process | 2.5.0 | Apache-2.0 OR MIT | Other / optional |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 | Other / optional |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | Other / optional |
| async-task | 4.7.1 | Apache-2.0 OR MIT | Other / optional |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | Other / optional |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | Other / optional |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | Windows / build |
| base64 | 0.22.1 | MIT OR Apache-2.0 | Windows / build |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | Other / optional |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | Other / optional |
| bitflags | 1.3.2 | MIT/Apache-2.0 | Other / optional |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | Windows / build |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | Windows / build |
| block2 | 0.5.1 | MIT | Other / optional |
| block2 | 0.6.2 | MIT | Other / optional |
| blocking | 1.7.0 | Apache-2.0 OR MIT | Other / optional |
| bumpalo | 3.20.4 | MIT OR Apache-2.0 | Other / optional |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | Windows / build |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | Windows / build |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | Windows / build |
| bytes | 1.12.1 | MIT | Other / optional |
| calloop | 0.13.0 | MIT | Other / optional |
| calloop | 0.14.5 | MIT | Other / optional |
| calloop-wayland-source | 0.3.0 | MIT | Other / optional |
| calloop-wayland-source | 0.4.1 | MIT | Other / optional |
| cc | 1.11.2 | MIT OR Apache-2.0 | Windows / build |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | Windows / build |
| cfg_aliases | 0.2.2 | MIT | Windows / build |
| cgl | 0.3.2 | MIT / Apache-2.0 | Other / optional |
| clipboard-win | 5.4.3 | BSL-1.0 | Windows / build |
| codespan-reporting | 0.12.0 | Apache-2.0 | Other / optional |
| combine | 4.6.9 | MIT | Other / optional |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | Other / optional |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | Other / optional |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | Other / optional |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | Other / optional |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | Other / optional |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | Windows / build |
| crc32fast | 1.5.5 | MIT OR Apache-2.0 | Windows / build |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | Other / optional |
| crunchy | 0.2.4 | MIT | Other / optional |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | Windows / build |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | Windows / build |
| digest | 0.10.7 | MIT OR Apache-2.0 | Windows / build |
| dispatch | 0.2.0 | MIT | Other / optional |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | Other / optional |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | Windows / build |
| dlib | 0.5.3 | MIT | Other / optional |
| document-features | 0.2.12 | MIT OR Apache-2.0 | Windows / build |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | Other / optional |
| dpi | 0.1.2 | Apache-2.0 AND MIT | Windows / build |
| ecolor | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| eframe | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| egui | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| egui-wgpu | 0.33.3 | MIT OR Apache-2.0 | Other / optional |
| egui-winit | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| egui_glow | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| emath | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| endi | 1.1.1 | MIT | Other / optional |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | Other / optional |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | Other / optional |
| epaint | 0.33.3 | MIT OR Apache-2.0 | Windows / build |
| epaint_default_fonts | 0.33.3 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | Windows / build |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | Other / optional |
| errno | 0.3.15 | MIT OR Apache-2.0 | Other / optional |
| error-code | 3.4.0 | BSL-1.0 | Windows / build |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | Other / optional |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | Other / optional |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | Other / optional |
| fax | 0.2.7 | MIT | Other / optional |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | Windows / build |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | Windows / build |
| flate2 | 1.1.11 | MIT OR Apache-2.0 | Windows / build |
| foldhash | 0.2.0 | Zlib | Other / optional |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | Other / optional |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | Other / optional |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | Other / optional |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | Windows / build |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | Other / optional |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | Other / optional |
| generic-array | 0.14.7 | MIT | Windows / build |
| gethostname | 1.1.0 | Apache-2.0 | Other / optional |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | Other / optional |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | Windows / build |
| gl_generator | 0.14.0 | Apache-2.0 | Windows / build |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib | Windows / build |
| glutin | 0.32.3 | Apache-2.0 | Windows / build |
| glutin-winit | 0.5.0 | MIT | Windows / build |
| glutin_egl_sys | 0.7.1 | Apache-2.0 | Windows / build |
| glutin_glx_sys | 0.6.1 | Apache-2.0 | Other / optional |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | Windows / build |
| half | 2.7.1 | MIT OR Apache-2.0 | Other / optional |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | Other / optional |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | Other / optional |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | Other / optional |
| hex | 0.4.3 | MIT OR Apache-2.0 | Other / optional |
| hexf-parse | 0.2.1 | CC0-1.0 | Other / optional |
| icu_collections | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_locale_core | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_normalizer | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_properties | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_properties_data | 2.4.0 | Unicode-3.0 | Windows / build |
| icu_provider | 2.4.0 | Unicode-3.0 | Windows / build |
| idna | 1.1.0 | MIT OR Apache-2.0 | Windows / build |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | Windows / build |
| image | 0.25.10 | MIT OR Apache-2.0 | Windows / build |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | Other / optional |
| itoa | 1.0.19 | MIT OR Apache-2.0 | Windows / build |
| jni | 0.22.4 | MIT OR Apache-2.0 | Other / optional |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | Other / optional |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | Other / optional |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | Other / optional |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | Other / optional |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | Other / optional |
| js-sys | 0.3.107 | MIT OR Apache-2.0 | Other / optional |
| khronos_api | 3.1.0 | Apache-2.0 | Windows / build |
| libc | 0.2.191 | MIT OR Apache-2.0 | Windows / build |
| libloading | 0.8.9 | ISC | Windows / build |
| libm | 0.2.16 | MIT | Other / optional |
| libredox | 0.1.25 | MIT | Other / optional |
| libssh2-sys | 0.3.4 | MIT OR Apache-2.0 | Windows / build |
| libz-sys | 1.1.33 | MIT OR Apache-2.0 | Windows / build |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| litemap | 0.8.3 | Unicode-3.0 | Windows / build |
| litrs | 1.0.1 | MIT OR Apache-2.0 | Windows / build |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | Windows / build |
| log | 0.4.35 | MIT OR Apache-2.0 | Windows / build |
| memchr | 2.8.3 | Unlicense OR MIT | Windows / build |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | Other / optional |
| memoffset | 0.9.1 | MIT | Windows / build |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Windows / build |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | Windows / build |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | Windows / build |
| naga | 27.0.3 | MIT OR Apache-2.0 | Other / optional |
| ndk | 0.9.0 | MIT OR Apache-2.0 | Other / optional |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | Other / optional |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | Other / optional |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | Windows / build |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | Windows / build |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | Other / optional |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | Other / optional |
| objc-sys | 0.3.5 | MIT | Other / optional |
| objc2 | 0.5.3 | MIT | Other / optional |
| objc2 | 0.6.5 | MIT | Other / optional |
| objc2-app-kit | 0.2.2 | MIT | Other / optional |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Other / optional |
| objc2-cloud-kit | 0.2.2 | MIT | Other / optional |
| objc2-contacts | 0.2.2 | MIT | Other / optional |
| objc2-core-data | 0.2.2 | MIT | Other / optional |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Other / optional |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Other / optional |
| objc2-core-image | 0.2.2 | MIT | Other / optional |
| objc2-core-location | 0.2.2 | MIT | Other / optional |
| objc2-encode | 4.1.0 | MIT | Other / optional |
| objc2-foundation | 0.2.2 | MIT | Other / optional |
| objc2-foundation | 0.3.2 | MIT | Other / optional |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | Other / optional |
| objc2-link-presentation | 0.2.2 | MIT | Other / optional |
| objc2-metal | 0.2.2 | MIT | Other / optional |
| objc2-quartz-core | 0.2.2 | MIT | Other / optional |
| objc2-symbols | 0.2.2 | MIT | Other / optional |
| objc2-ui-kit | 0.2.2 | MIT | Other / optional |
| objc2-uniform-type-identifiers | 0.2.2 | MIT | Other / optional |
| objc2-user-notifications | 0.2.2 | MIT | Other / optional |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | Windows / build |
| openssl-sys | 0.9.118 | MIT | Windows / build |
| orbclient | 0.3.55 | MIT | Other / optional |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | Other / optional |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | Windows / build |
| parking | 2.2.1 | Apache-2.0 OR MIT | Other / optional |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | Windows / build |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | Windows / build |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | Windows / build |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | Other / optional |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | Other / optional |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | Windows / build |
| piper | 0.2.5 | MIT OR Apache-2.0 | Other / optional |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | Windows / build |
| plain | 0.2.3 | MIT/Apache-2.0 | Other / optional |
| png | 0.18.1 | MIT OR Apache-2.0 | Windows / build |
| polling | 3.11.0 | Apache-2.0 OR MIT | Other / optional |
| pollster | 0.4.0 | Apache-2.0/MIT | Other / optional |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | Other / optional |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | Other / optional |
| potential_utf | 0.1.6 | Unicode-3.0 | Windows / build |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | Other / optional |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | Other / optional |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | Windows / build |
| profiling | 1.0.18 | MIT OR Apache-2.0 | Windows / build |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | Windows / build |
| quick-error | 2.0.1 | MIT/Apache-2.0 | Other / optional |
| quick-xml | 0.41.0 | MIT | Other / optional |
| quote | 1.0.47 | MIT OR Apache-2.0 | Windows / build |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | Other / optional |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | Other / optional |
| rand | 0.9.5 | MIT OR Apache-2.0 | Other / optional |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | Other / optional |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | Other / optional |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Windows / build |
| redox_syscall | 0.4.1 | MIT | Other / optional |
| redox_syscall | 0.5.18 | MIT | Other / optional |
| redox_syscall | 0.9.5 | MIT | Other / optional |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | Other / optional |
| rfd | 0.15.4 | MIT | Windows / build |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | Other / optional |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | Other / optional |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | Other / optional |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | Other / optional |
| same-file | 1.0.6 | Unlicense/MIT | Other / optional |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | Other / optional |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | Windows / build |
| semver | 1.0.28 | MIT OR Apache-2.0 | Windows / build |
| serde | 1.0.229 | MIT OR Apache-2.0 | Windows / build |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | Windows / build |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | Windows / build |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | Windows / build |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | Other / optional |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 | Windows / build |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | Windows / build |
| shlex | 2.0.1 | MIT OR Apache-2.0 | Windows / build |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | Other / optional |
| simd-adler32 | 0.3.10 | MIT | Windows / build |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | Other / optional |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | Other / optional |
| slab | 0.4.12 | MIT | Other / optional |
| slotmap | 1.1.1 | Zlib | Other / optional |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | Windows / build |
| smithay-client-toolkit | 0.19.2 | MIT | Other / optional |
| smithay-client-toolkit | 0.20.0 | MIT | Other / optional |
| smithay-clipboard | 0.7.3 | MIT | Other / optional |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | Windows / build |
| ssh2 | 0.9.7 | MIT OR Apache-2.0 | Windows / build |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | Windows / build |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | Windows / build |
| syn | 2.0.119 | MIT OR Apache-2.0 | Windows / build |
| syn | 3.0.10 | MIT OR Apache-2.0 | Windows / build |
| synstructure | 0.14.0 | MIT | Windows / build |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Other / optional |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | Other / optional |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | Other / optional |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | Other / optional |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | Other / optional |
| tiff | 0.11.3 | MIT | Other / optional |
| tinystr | 0.8.5 | Unicode-3.0 | Windows / build |
| tokio | 1.54.1 | MIT | Other / optional |
| toml | 1.2.1+spec-1.1.0 | MIT OR Apache-2.0 | Windows / build |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | Windows / build |
| toml_edit | 0.25.19+spec-1.1.0 | MIT OR Apache-2.0 | Other / optional |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | Windows / build |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 | Other / optional |
| tracing | 0.1.44 | MIT | Windows / build |
| tracing-attributes | 0.1.31 | MIT | Other / optional |
| tracing-core | 0.1.36 | MIT | Windows / build |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | Windows / build |
| type-map | 0.5.1 | MIT/Apache-2.0 | Other / optional |
| typenum | 1.20.1 | MIT OR Apache-2.0 | Windows / build |
| uds_windows | 1.2.1 | MIT | Other / optional |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | Windows / build |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | Windows / build |
| unicode-width | 0.1.14 | MIT OR Apache-2.0 | Windows / build |
| unicode-width | 0.2.3 | MIT OR Apache-2.0 | Other / optional |
| url | 2.5.8 | MIT OR Apache-2.0 | Windows / build |
| urlencoding | 2.1.3 | MIT | Other / optional |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | Windows / build |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | Windows / build |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | Windows / build |
| uuid | 1.29.2 | Apache-2.0 OR MIT | Windows / build |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | Windows / build |
| version_check | 0.9.5 | MIT/Apache-2.0 | Windows / build |
| vt100 | 0.15.2 | MIT | Windows / build |
| vte | 0.11.1 | Apache-2.0 OR MIT | Windows / build |
| vte_generate_state_changes | 0.1.2 | Apache-2.0 OR MIT | Windows / build |
| walkdir | 2.5.0 | Unlicense/MIT | Other / optional |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | Other / optional |
| wasm-bindgen-futures | 0.4.80 | MIT OR Apache-2.0 | Other / optional |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 | Other / optional |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 | Other / optional |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 | Other / optional |
| wayland-backend | 0.3.17 | MIT | Other / optional |
| wayland-client | 0.31.15 | MIT | Other / optional |
| wayland-csd-frame | 0.3.0 | MIT | Other / optional |
| wayland-cursor | 0.31.14 | MIT | Other / optional |
| wayland-protocols | 0.32.13 | MIT | Other / optional |
| wayland-protocols-experimental | 20250721.0.1 | MIT | Other / optional |
| wayland-protocols-misc | 0.3.12 | MIT | Other / optional |
| wayland-protocols-plasma | 0.3.12 | MIT | Other / optional |
| wayland-protocols-wlr | 0.3.12 | MIT | Other / optional |
| wayland-scanner | 0.31.11 | MIT | Other / optional |
| wayland-sys | 0.31.11 | MIT | Other / optional |
| web-sys | 0.3.107 | MIT OR Apache-2.0 | Other / optional |
| web-time | 1.1.0 | MIT OR Apache-2.0 | Windows / build |
| webbrowser | 1.2.7 | MIT OR Apache-2.0 | Windows / build |
| weezl | 0.1.12 | MIT OR Apache-2.0 | Other / optional |
| wgpu | 27.0.1 | MIT OR Apache-2.0 | Other / optional |
| wgpu-core | 27.0.3 | MIT OR Apache-2.0 | Other / optional |
| wgpu-core-deps-windows-linux-android | 27.0.0 | MIT OR Apache-2.0 | Other / optional |
| wgpu-hal | 27.0.4 | MIT OR Apache-2.0 | Other / optional |
| wgpu-types | 27.0.1 | MIT OR Apache-2.0 | Other / optional |
| winapi-util | 0.1.11 | Unlicense OR MIT | Other / optional |
| windows | 0.62.2 | MIT OR Apache-2.0 | Windows / build |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | Windows / build |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | Windows / build |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | Windows / build |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | Windows / build |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | Windows / build |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | Windows / build |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | Windows / build |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | Windows / build |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | Windows / build |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | Windows / build |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | Windows / build |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | Windows / build |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | Windows / build |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | Windows / build |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | Other / optional |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | Windows / build |
| winit | 0.30.13 | Apache-2.0 | Windows / build |
| winnow | 1.0.5 | MIT | Windows / build |
| winresource | 0.1.34 | MIT | Windows / build |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Other / optional |
| writeable | 0.6.4 | Unicode-3.0 | Windows / build |
| x11-dl | 2.21.0 | MIT | Other / optional |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | Other / optional |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | Other / optional |
| xcursor | 0.3.11 | MIT | Other / optional |
| xkbcommon-dl | 0.4.2 | MIT | Other / optional |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | Other / optional |
| xml-rs | 0.8.29 | MIT | Windows / build |
| yoke | 0.8.4 | Unicode-3.0 | Windows / build |
| yoke-derive | 0.8.4 | Unicode-3.0 | Windows / build |
| zbus | 5.19.0 | MIT | Other / optional |
| zbus_macros | 5.19.0 | MIT | Other / optional |
| zbus_names | 4.3.5 | MIT | Other / optional |
| zcheapstr | 1.2.0 | MIT | Other / optional |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Windows / build |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | Other / optional |
| zerofrom | 0.1.8 | Unicode-3.0 | Windows / build |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | Windows / build |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | Windows / build |
| zerotrie | 0.2.5 | Unicode-3.0 | Windows / build |
| zerovec | 0.11.9 | Unicode-3.0 | Windows / build |
| zerovec-derive | 0.11.7 | Unicode-3.0 | Windows / build |
| zlib-rs | 0.6.8 | Zlib | Other / optional |
| zmij | 1.0.23 | MIT | Windows / build |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | Other / optional |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | Other / optional |
| zvariant | 5.15.1 | MIT | Other / optional |
| zvariant_derive | 5.15.1 | MIT | Other / optional |
| zvariant_utils | 4.2.0 | MIT | Other / optional |

## Source record

The full-text file reproduces 647 standalone license/notice files, plus the native-source and embedded-font notices. Files are labeled using package-relative paths; original hashes and exact upstream commit URLs are retained. Package source archives are available through the versioned crates.io links recorded in that file. The Nexus Remote source archive includes Cargo.lock to reproduce dependency resolution.