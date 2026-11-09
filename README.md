# TMCL

English | [简体中文](README.zh-CN.md)

A terminal launcher for Minecraft: Java Edition on Windows. One core, two front ends: a full-screen TUI and a scriptable CLI.

TMCL is not affiliated with Mojang Studios, Microsoft, Forge, Fabric, or NeoForge.

## Features

- **Instances** — create, import, verify, repair, export and launch, for Vanilla, Fabric, Quilt, Forge and NeoForge.
- **Content** — mods, modpacks, resource packs, shaders and worlds, with a per-instance trash.
- **Downloads** — Modrinth and CurseForge, including modpack install and update.
- **Accounts** — Microsoft device-code sign-in, or offline profiles.
- **Tasks** — installs, repairs and Java downloads persist across restarts and can be watched or cancelled.
- **Five TUI presets** — the same pages, keys and data; only the drawing differs.

## Requirements

Windows 10/11 x86_64, an ANSI terminal (Windows Terminal), and a Java runtime for the version you launch. TMCL scans for installed runtimes and can download Mojang's.

## Build and run

~~~bash
cargo run                    # TUI
cargo run -- --help          # CLI
cargo run -- --json instance list
~~~

`cargo build --release` builds `tmcl.exe`; `./scripts/package.ps1` writes the release archive.

## Quick start

~~~bash
tmcl instance create fabric-1.21.1 --version 1.21.1 --loader fabric --loader-version 0.16.5
tmcl instance launch @last
tmcl search sodium --kind mod
tmcl install AANobbMI --instance @last --with-deps
~~~

Any command that acts on an instance accepts `@last`, a name, a UUID, a list number or a path. Options, exit codes, JSON output and shell quirks live in **[docs/cli.en.md](docs/cli.en.md)**.

## Where things live

| Path | Contents |
| --- | --- |
| `%APPDATA%\TMCL` | configuration, accounts, tasks, logs, downloaded runtimes |
| the folder containing `tmcl.exe` | default game directory (portable layout) |

The two are separate: launcher data is per-user, the game directory can be anywhere. Settings → Game directories switches, adds or unregisters one.

## TUI presets

| Preset | Look |
| --- | --- |
| Classic | framed shell, top tabs, bordered panels (default) |
| Nightlight | one accent colour, eight themes, no animation |
| Paper | printed page, ivory / ash / kraft stock, day or night |
| Shelf | cover art, 56/44 split, one fixed palette |

Switch in Settings → Appearance → TUI preset. It applies immediately.

## Accounts

Microsoft sign-in uses the device-code flow: the launcher never sees a password, and the refresh token is stored with Windows DPAPI. Offline profiles use the standard `OfflinePlayer:<name>` UUID; if `%APPDATA%\TMCL\authlib-injector.jar` exists, offline launches use it for a local Yggdrasil service, and TMCL never downloads that file itself. No command prints a token.

## Development

~~~bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
~~~

Network tests are `#[ignore]`d by default: `cargo test --test cli_network -- --ignored`.

## Third-party notices

Parts of the TUI layout come from [binsider](https://github.com/orhun/binsider) (MIT OR Apache-2.0). The `nightlight` and `shelf` presets study other terminal UIs without copying code, constants or assets. Revisions, licences and the exact boundary are in [`NOTICE`](NOTICE).
