# TMCL command-line manual

English | [简体中文](cli.md)

Everything TMCL can do is on the command line; the TUI is another layer over the same core. This manual lists every command by group: what it does, its syntax, its options and examples.

- Background on installation, accounts and loaders is in [README.md](../README.md).
- Examples use Windows PowerShell; the same commands work in bash and cmd. The differences are in [§1.6](#16-shell-differences-powershell-bash-cmd).

---

## Contents

- [1. Common rules](#1-common-rules)
  - [1.1 Global options](#11-global-options)
  - [1.2 Output and `--json`](#12-output-and---json)
  - [1.3 Exit codes](#13-exit-codes)
  - [1.4 Confirmation and `--dry-run`](#14-confirmation-and---dry-run)
  - [1.5 Instance references `[REF]`](#15-instance-references-ref)
  - [1.6 Shell differences: PowerShell, bash, cmd](#16-shell-differences-powershell-bash-cmd)
  - [1.7 Matching content names `<NAME>`](#17-matching-content-names-name)
- [2. Instances: `tmcl instance`](#2-instances-tmcl-instance)
- [3. Accounts: `tmcl account`](#3-accounts-tmcl-account)
- [4. Java: `tmcl java`](#4-java-tmcl-java)
- [5. Game versions: `tmcl version`](#5-game-versions-tmcl-version)
- [6. Tasks: `tmcl task`](#6-tasks-tmcl-task)
- [7. Configuration: `tmcl config`](#7-configuration-tmcl-config)
- [8. Instance content: `tmcl content`](#8-instance-content-tmcl-content)
- [9. Trash: `tmcl trash`](#9-trash-tmcl-trash)
- [10. Worlds and datapacks: `tmcl world`](#10-worlds-and-datapacks-tmcl-world)
- [11. Mod updates: `tmcl mod`](#11-mod-updates-tmcl-mod)
- [12. Download center: `search` / `project` / `install`](#12-download-center-search--project--install)
- [13. Modpacks: `tmcl modpack`](#13-modpacks-tmcl-modpack)
- [14. Self-update: `tmcl update`](#14-self-update-tmcl-update)
- [15. Completions and the TUI: `completions` / `tui`](#15-completions-and-the-tui-completions--tui)
- [16. Adding tmcl to PATH: `tmcl path`](#16-adding-tmcl-to-path-tmcl-path)
- [17. Recipes](#17-recipes)

---

## 1. Common rules

### 1.1 Global options

These five options work on every command, wherever they appear:

| Option | Effect |
|---|---|
| `--json` | Print the result as JSON; progress becomes NDJSON on stderr |
| `--quiet` | No progress, only the final result |
| `--dry-run` | Print the plan, change nothing |
| `--no-color` | Disable colour (already off when stdout is not a terminal) |
| `--verbose` | Write HTTP requests and task events to stderr; same as `TMCL_LOG=debug` |

With both `--quiet` and `--json`, the result is still printed and progress is silent.

### 1.2 Output and `--json`

- **stdout carries the result**: lists, single objects, the outcome of a confirmation.
- **stderr carries progress, stages, warnings and errors.** `tmcl ... > out.txt` therefore never mixes progress into the file.
- With `--json`, progress on stderr is one JSON object per line (NDJSON):

  ```json
  {"event":"start","label":"Install sodium","done":0,"total":0}
  {"event":"progress","label":"Install sodium","done":42,"total":100}
  {"event":"done","label":"Install sodium","done":100,"total":100}
  ```

  Consume it like this:

  ```bash
  tmcl install AANobbMI --instance 14 --json 2> events.ndjson | jq .
  jq -r 'select(.event=="progress") | "\(.done)/\(.total)"' events.ndjson
  ```

- On failure stdout is empty and stderr carries `{"error": "...", "category": "Network"}` (with `--json`).

### 1.3 Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `2` | Bad argument or configuration (including "instance not found") |
| `3` | Network or download failure |
| `4` | Game version or Java error |
| `5` | Account error |
| `6` | Loader install or resolution error |
| `7` | Launch failure |
| `8` | Terminal error |
| `130` | Interrupted with Ctrl-C |

Scripts should test `0` versus non-zero; read the exact value only when the reason matters.

### 1.4 Confirmation and `--dry-run`

Commands that change the disk (`delete`, `remove`, `purge`, `restore`, `update`, `cancel`, `mod update`, `update apply`) pass three gates:

1. **Interactive confirmation**: `Continue? [y/N]` when stderr is a terminal.
2. **`--yes`**: skip the question, for scripts.
3. **`--dry-run`**: print the plan, do nothing.

With stderr not a terminal and neither `--yes` nor `--dry-run`, the command **refuses and exits 2** instead of acting silently.

`--dry-run --json` prints the plan itself, for example:

```json
{"plan": ["delete D:\\games\\versions\\old"], "applied": false}
```

### 1.5 Instance references `[REF]`

Every command that acts on an instance accepts the same reference syntax:

| Form | Meaning | Example |
|---|---|---|
| omitted | Same as `@last`: the instance marked current in the configuration | `tmcl content list` |
| `@last` | The same (in PowerShell, omit it or quote it — see 1.6) | `tmcl instance show @last` |
| number | Row n of `instance list` | `tmcl instance launch 3` |
| `+number` | Always row n (skips an instance with that name) | `tmcl instance launch +3` |
| name | Exact match, or an **unambiguous** prefix | `tmcl instance launch ArdaCraft` / `launch arda` |
| UUID | The `id` from `instance list --json` | `tmcl instance show 33333333-...` |
| path | The instance directory itself; no registration needed | `tmcl instance show "D:\games\versions\demo"` |

Details:

- **A number is a position, not an identity.** Deleting or installing an instance moves every number after it. Scripts should use a name, a UUID or a path.
- **A matching name wins**: if an instance is really named `123`, `tmcl instance launch 123` picks it; write `+123` to force the row number.
- **A prefix is only tried when nothing above matched**, and it must be unique. `farming` matching three instances is refused with the candidates listed:

  ```
  configuration error: instance reference is ambiguous: farming matches 3 instances;
  use a path or a number instead:
    FarmingTales_1.2  (D:\...\FarmingTales_1.2)
    FarmingTales_1.6.8  (D:\...\FarmingTales_1.6.8)
    FarmingTales_ForgeⅡ v1.5.2fix2  (D:\...\FarmingTales_ForgeⅡ v1.5.2fix2)
  ```

- Commands whose positional argument is taken (`install`, `modpack install/update`) use `--instance <REF>` instead.
- A name with spaces must be quoted: `tmcl instance launch "BattleArmory TACZ 26.0.5"`.
- The same name in two game directories is an **error**; TMCL will not pick one for you.

### 1.6 Shell differences: PowerShell, bash, cmd

| Case | Note |
|---|---|
| `@` in PowerShell | `@xxx` is treated as splatting and **silently dropped**. `instance launch @9` therefore has no reference (it launches the current instance), and `@last` behaves the same — omit it, or write `'@last'`. |
| Names with spaces | PowerShell needs quotes, or the rest is read as extra arguments. |
| `#` | A comment in PowerShell and bash; never use it as a prefix. |
| Numeric references | Safe in all three shells; the recommended form outside scripts. |

### 1.7 Matching content names `<NAME>`

`<NAME>` in `content enable/disable/remove`, `trash restore`, `world *` and `mod update`:

1. Match the **file name** first (`sodium-0.6.13.jar`), case-insensitively;
2. then the **display name** (`Sodium`), also case-insensitively;
3. several matches are an error that lists the candidates; zero matches suggests the closest name.

Lists print the display name, but both forms work.

---

## 2. Instances: `tmcl instance`

### `tmcl instance list`

Lists every instance; the first column is the **row number**, usable as `[REF]`.

```bash
tmcl instance list [--status ready|incomplete|all] [--game-dir PATH]
```

| Option | Description |
|---|---|
| `--status` | Only `ready` (all files present) or `incomplete` (missing files); default `all` |
| `--game-dir` | Only instances in this game directory |

```bash
tmcl instance list
tmcl instance list --status incomplete
tmcl instance list --json | jq -r '.[] | "\(.index) \(.name)"'
```

Text columns: `row name version loader status path`. In a narrow window the path is truncated from the left (the name is kept), then the column is dropped entirely; redirecting to a file always prints the full path. Numbers skip when `--status` filters the list — they are always computed over the full list, so the meaning of `+n` never changes.

### `tmcl instance show`

Prints everything about one instance.

```bash
tmcl instance show [REF]
```

```bash
tmcl instance show 14
```

```
id: 33333333-3333-4333-8333-333333333333
name: FarmingTales_ForgeⅡ v1.5.2fix2
version: 1.20.1
loader: Forge
path: D:\games\.minecraft\versions\FarmingTales_ForgeⅡ v1.5.2fix2
game directory: D:\games\.minecraft
status: ready
launch count: 8
play time: 19m
missing files: 0
```

When `missing files` reads `unknown (no readable version JSON)`, the version JSON could not be read.

### `tmcl instance create`

Creates an instance and installs its loader.

```bash
tmcl instance create <NAME> --version <MINECRAFT_VERSION> [--loader <LOADER>] [--loader-version <VERSION>] [--game-dir PATH]
```

| Option | Description |
|---|---|
| `--version` | Required, e.g. `1.21.1` |
| `--loader` | `vanilla` (default) / `fabric` / `quilt` / `forge` / `neoforge` |
| `--loader-version` | **Required** for every loader except Vanilla, with the exact version |
| `--game-dir` | Which game directory to create it in; the first configured one by default |

```bash
tmcl instance create "Fabric 1.21.1" --version 1.21.1 --loader fabric --loader-version 0.16.5
tmcl instance create "Forge 1.20.1" --version 1.20.1 --loader forge --loader-version 47.4.10
tmcl instance create demo --version 1.21.1 --dry-run      # only print the directory it would create
```

The installation is a persisted task; Ctrl-C cancels it and exits 130.

### `tmcl instance import`

Takes an existing version directory under management (from PCL2, HMCL or placed by hand).

```bash
tmcl instance import <INSTANCE_DIR> --version <MINECRAFT_VERSION> [--name <NAME>] [--game-directory-id <UUID>]
```

```bash
tmcl instance import "D:\games\versions\1.20.1-forge-47.4.10" --version 1.20.1 --name "Forge Survival"
```

Registration only; nothing is copied. `--name` renames the directory and `<name>.jar/.json` as well.

### `tmcl instance rename`

Renames the version directory, `<old>.jar/.json` and the metadata.

```bash
tmcl instance rename [REF] [NEW_NAME]
```

```bash
tmcl instance rename 14 "FarmingTales Pack"
```

The current-instance marker follows. **Note**: the version name shown in game and the directory name seen in PCL2 change too.

### `tmcl instance repair`

Restores missing files (client jar, libraries, asset indexes); existing files are not downloaded again.

```bash
tmcl instance repair [REF]
```

```bash
tmcl instance repair 3
tmcl instance repair --dry-run          # only list what would be downloaded
tmcl instance repair --json
```

JSON: `{"missing": 3, "downloaded": 3}`.

### `tmcl instance export`

Packs the instance directory into a zip.

```bash
tmcl instance export [REF] [--output <PATH>]
```

```bash
tmcl instance export 14 --output D:\backups/farming.zip
tmcl instance export 14                       # <instance name>.zip by default
```

An existing target file is **never overwritten**. `--output` is a zip path; the default is `<instance name>.zip` in the current directory.

Packing an instance of several hundred MB takes a while; `--dry-run` prints "Would pack … into …" and returns immediately.

### `tmcl instance verify`

Checks instance files against the version JSON (existence, plus sha1 with `--full`).

```bash
tmcl instance verify [REF] [--full] [--client-json <PATH>]
```

```bash
tmcl instance verify 3
tmcl instance verify 3 --full
tmcl instance verify 3 --json | jq '.missing[].path'
```

| Option | Description |
|---|---|
| `--full` | Compare sha1 as well as existence (slow) |
| `--client-json` | Use this version JSON as the baseline; the instance's own by default |

Only files **this machine would actually use** are checked: a library whose `rules` make it apply to another OS only (for example `java-objc-bridge` with `allow osx` in `1.20.1`) is not counted as missing, and natives resolve only to the entry for the current OS and bitness in the `natives` map. Libraries are located as `libraries/<path>`, the same rule the classpath uses at launch.

```
Checked 64 files; 0 missing or invalid.
```

`missing[].path` is relative to the game directory (for example `libraries/ca/weblite/...jar`); `url` and `sha1` are there so the file can be restored directly. Only the instance's own version JSON is read — a parent version named by `inheritsFrom` is not part of the check.

### `tmcl instance launch`

Starts the game and waits for the process to exit.

```bash
tmcl instance launch [REF] [--account-id <UUID>] [--java <PATH>] [--min-memory-mb N] [--max-memory-mb N] [--width N] [--height N] [--fullscreen] [--client-json <PATH>] [--export-script <PATH>]
```

| Option | Description |
|---|---|
| `--account-id` | Which account to use; the selected one by default |
| `--java` | The java executable; chosen by the `config java` policy by default |
| `--min-memory-mb` / `--max-memory-mb` | Heap floor and ceiling |
| `--width` / `--height` / `--fullscreen` | Window size and full screen |
| `--client-json` | Launch with another version JSON (debugging) |
| `--export-script` | **Do not launch**; write the command line to a `.bat`, `.ps1` or `.sh` |

```bash
tmcl instance launch 14
tmcl instance launch 14 --max-memory-mb 8192
tmcl instance launch 14 --export-script launch.bat
```

In an exported script the session token becomes an environment reference (`TMCL_ACCESS_TOKEN`), so the file can be shared; an offline account has no token and the script carries the literal `0`. Game output is also written to `%APPDATA%\TMCL\logs\game\<uuid>.log` (only stderr when `logs.game_debug` is off; a failed launch appends the tail of stdout).

### `tmcl instance unregister`

Removes the instance from the list (**files are kept**).

```bash
tmcl instance unregister [REF] [--yes]
```

```bash
tmcl instance unregister 14
```

### `tmcl instance delete`

Deletes the instance directory and everything in it.

```bash
tmcl instance delete [REF] [--yes]
```

```bash
tmcl instance delete "old test" --yes
tmcl instance delete 14 --dry-run          # only print the paths it would delete
```

If it was the current instance, the marker is cleared.

---

## 3. Accounts: `tmcl account`

### `tmcl account list`

```bash
tmcl account list [--json]
```

```
11111111-1111-4111-8111-111111111111  Offline    Alex
22222222-2222-4222-8222-222222222222  Microsoft  Steve
```

The first column is the ID `account remove` expects, not the user name.

### `tmcl account login-microsoft`

Runs the device-code flow: prints a URL and a code, saves the account once you confirm in the browser.

```bash
tmcl account login-microsoft
```

Tokens are encrypted with Windows DPAPI under `%APPDATA%\TMCL`.

### `tmcl account add-offline`

```bash
tmcl account add-offline <USERNAME> [--uuid <UUID>]
```

```bash
tmcl account add-offline Steve
tmcl account add-offline Steve --uuid 00000000-0000-3000-8000-000000000002
```

Without `--uuid` the UUID is derived from the name (the same name always gets the same UUID).

### `tmcl account remove`

```bash
tmcl account remove <ACCOUNT_ID>
```

`ACCOUNT_ID` is the first column of `account list`, not the user name.

---

## 4. Java: `tmcl java`

### `tmcl java list`

Lists the runtimes found (including the ones TMCL downloaded).

```bash
tmcl java list [--json]
```

```
Java 25  C:\Users\you\AppData\Roaming\TMCL\runtime\java-25\bin\java.exe
Java 21  C:\Users\you\AppData\Roaming\.minecraft\runtime\java-runtime-delta\bin\java.exe
Java 8   C:\Users\you\AppData\Roaming\.minecraft\runtime\jre-legacy\bin\java.exe
```

The first column is the major version, the second the executable.

### `tmcl java scan`

Rescans and reports how many it found (the TUI uses the same scan at startup).

```bash
tmcl java scan
```

```
Scanned 6 directories; found 3 runtimes.
```

### `tmcl java download`

Downloads a runtime provided by Mojang.

```bash
tmcl java download <MAJOR> [--manifest-url <URL>]
```

```bash
tmcl java download 21
tmcl java download 17 --dry-run
```

Installs into `%APPDATA%\TMCL\runtime\java-<major>`; `config java auto` picks it up from there.

---

## 5. Game versions: `tmcl version`

### `tmcl version list`

```bash
tmcl version list [--type release|snapshot|old_beta|old_alpha|all]
```

```bash
tmcl version list --type release
```

```
26.3  release
26.2  release
...
```

Two columns: version id and type. Needs the network (Mojang's version manifest).

---

## 6. Tasks: `tmcl task`

Every install, repair and download is persisted as a task, shared by both front ends.

### `tmcl task list`

```bash
tmcl task list [--state all|queued|running|done|failed|paused|cancelled]
```

```
44444444-...  succeeded  Download modpack rQiXwLhB (BattleArmory TACZ 26.0.5.mrpack)
d4bf514c-...  succeeded  Install BattleArmory TACZ 26.0.5 (1.20.1, forge)
```

```bash
tmcl task list --state failed
```

### `tmcl task watch`

Follows a task until it ends, printing progress to stderr.

```bash
tmcl task watch <TASK_ID>
```

```bash
tmcl task watch 44444444-4444-4444-8444-444444444444
tmcl task watch 44444444-... --json 2> events.ndjson
```

Tasks from another process are visible too. If the TMCL process that recorded the task has exited, `watch` first closes the stale record as `failed` (`failure` = `Interrupted when TMCL exited`, and `task retry` can run it again), then returns.

### `tmcl task retry`

Runs a failed task again.

```bash
tmcl task retry <TASK_ID>
```

### `tmcl task cancel`

Cancels the task and deletes its record.

```bash
tmcl task cancel <TASK_ID> [--yes]
```

---

## 7. Configuration: `tmcl config`

### `tmcl config path`

Prints the data directory (configuration, tasks, logs and runtimes all live under it).

```bash
tmcl config path
```

```
C:\Users\you\AppData\Roaming\TMCL
```

### `tmcl config list`

Prints every setting as `key = value`.

```bash
tmcl config list
```

```
downloads.auto_download_threads = false
downloads.cache_dir = null
downloads.cache_limit_mb = 2048
downloads.file_download_source = DEFAULT
downloads.retry_count = 5
game_directories = ["D:\\games\\.minecraft"]
java.auto_download = false
java.selection_mode = automatic
...
```

The values are written the way `config set` accepts them: booleans are `true`/`false`, an empty value is `null`, lists are JSON arrays, enums are **upper case** (`DEFAULT` / `OFFICIAL` / `MIRROR`).

### `tmcl config get`

```bash
tmcl config get <KEY>
```

```bash
tmcl config get downloads.retry_count
```

### `tmcl config set`

The value is parsed as the type of the target field.

```bash
tmcl config set <KEY> <VALUE>
```

```bash
tmcl config set downloads.retry_count 5
tmcl config set downloads.file_download_source MIRROR
tmcl config set logs.game_debug true
tmcl config set downloads.cache_dir "D:\cache\tmcl"
tmcl config set downloads.cache_dir null          # clear it
```

An unknown key is an error that names the closest valid key.

### `tmcl config game-dir`

```bash
tmcl config game-dir list
tmcl config game-dir add <PATH>
tmcl config game-dir remove <PATH>
```

```bash
tmcl config game-dir add "D:\games\.minecraft"
tmcl config game-dir remove "D:\old\.minecraft"
```

`remove` only stops managing the directory; **files are kept**. The first directory is where `create` and `modpack install` put new instances.

### `tmcl config java`

```bash
tmcl config java auto         # pick the best runtime automatically
tmcl config java path <PATH>  # always use this java
```

```bash
tmcl config java path "C:\Program Files\Java\jdk-21\bin\java.exe"
```

`auto` picks by the Java major version the game version needs; after `path`, every launch uses that executable.

---

## 8. Instance content: `tmcl content`

Manages `mods/`, `resourcepacks/` and `shaderpacks/`. Resource pack state lives in `options.txt`, shader state in the loader's config, and everything else uses the `.disabled` suffix.

### `tmcl content list`

```bash
tmcl content list [REF] [--kind mod|pack|shader] [--sort name|size|status] [--filter <TEXT>]
```

| Option | Description |
|---|---|
| `--kind` | `mod` (default) / `pack` / `shader` |
| `--sort` | `name` (default) / `size` / `status`; ignored for packs and shaders, whose order the game depends on |
| `--filter` | Only entries whose name contains this string |

```bash
tmcl content list 14 --kind mod
tmcl content list 14 --kind mod --sort size | head -20
tmcl content list 14 --kind mod --filter bakery
tmcl content list 14 --kind pack --json
```

```
[Let's Do Addon] Structures        1.7.2          322.7 KB  enabled
[Let's Do] API                     1.2.15           1.4 MB  enabled
Yes Steve Model                    2.6.5-forge…    60.3 MB  disabled
```

Each JSON entry carries `name`/`file`/`kind`/`version`/`size`/`enabled`/`order`/`warning`.

### `tmcl content enable` / `disable`

```bash
tmcl content enable  [REF] [NAME] --kind mod|pack|shader
tmcl content disable [REF] [NAME] --kind mod|pack|shader
```

```bash
tmcl content disable 14 sodium.jar --kind mod
tmcl content enable  14 "Stay True.zip" --kind pack
```

`--kind` is required (there is no guessing here). An entry already in that state is a no-op.

### `tmcl content remove`

```bash
tmcl content remove [REF] [NAME] --kind <KIND> [--purge] [--yes]
```

```bash
tmcl content remove 14 sodium.jar --kind mod          # into the instance trash
tmcl content remove 14 sodium.jar --kind mod --purge  # delete it
```

Removing a resource pack or shader also removes its entry from `options.txt` or the loader config, so no dangling reference is left. A mistake can be undone with `trash restore` (unless `--purge` was used).

### `tmcl content add`

Copies a local file, or downloads a URL, into the instance.

```bash
tmcl content add [REF] [FILE_OR_URL] [--kind mod|pack|shader] [--yes]
```

```bash
tmcl content add 14 "D:\Downloads\Stay True.zip" --kind pack
tmcl content add 14 https://example.com/mod.jar
```

Without `--kind` the extension decides: `.jar` is a mod, `.zip` is a resource pack (this command does not handle modpacks). Any other extension fails with `cannot tell what … is` and needs an explicit `--kind`.

### `tmcl content order`

Moves a resource pack within the stack (higher wins).

```bash
tmcl content order [REF] [NAME] --up|--down
```

```bash
tmcl content order 14 "Stay True.zip" --up
```

Only resource packs have an order; `--kind` for shaders or mods is refused.

---

## 9. Trash: `tmcl trash`

Every instance has its own trash; `content remove`, `world remove` and `world datapack remove` all put things there.

### `tmcl trash list`

```bash
tmcl trash list [REF]
```

```
2026-11-01-2312  sodium.jar                          2.1 MB
2026-11-01-2240  saves/old-world                    43.0 MB
```

Three columns: timestamp, entry name, size. When empty it prints `The trash is empty.`

### `tmcl trash restore`

```bash
tmcl trash restore [REF] [NAME]
```

```bash
tmcl trash restore 14 sodium.jar
```

An existing file at the target path is an error; nothing is overwritten.

### `tmcl trash purge`

```bash
tmcl trash purge [REF] [--yes]
```

```bash
tmcl trash purge 14 --yes
```

---

## 10. Worlds and datapacks: `tmcl world`

### `tmcl world list`

```bash
tmcl world list [REF]
```

```
World     1.20.1     2026-10-17 08:55     1.3 MB
World222  1.20.1     2026-10-20 15:17    43.0 MB
```

### `tmcl world info`

Reads `level.dat` (version, seed, game time and so on).

```bash
tmcl world info [REF] [NAME]
```

```bash
tmcl world info 14 "World"
```

```
name: World
folder: World
version: 1.20.1
last played: 2026-10-17 08:55
game mode: survival
hardcore: no
size: 1.3 MB
path: D:\...\saves\World
```

### `tmcl world backup`

Packs the world into the instance's backup directory (`<instance>/.tmcl-backups/worlds`).

```bash
tmcl world backup [REF] [NAME]
```

```bash
tmcl world backup 14 "World"
```

### `tmcl world backups`

```bash
tmcl world backups [REF] [NAME]
```

```
2026-11-01-2310     42.8 MB  D:\...\.tmcl-backups\worlds\World-2026-11-01-2310.zip
2026-10-30-1201     41.2 MB  D:\...\.tmcl-backups\worlds\World-2026-10-30-1201.zip
```

Three columns: timestamp, size, path. With no backups it prints `No backups of World.` The first column is the **timestamp** `world restore --from` accepts.

### `tmcl world restore`

Replaces the current world with a backup (confirmation first).

```bash
tmcl world restore [REF] [NAME] --from <ARCHIVE> [--yes]
```

```bash
tmcl world restore 14 "World" --from 2026-11-01-2310
tmcl world restore 14 "World" --from D:\backups\world.zip
```

### `tmcl world clone`

```bash
tmcl world clone [REF] [NAME] [--suffix <TEXT>]
```

```bash
tmcl world clone 14 "World"                 # produces "World (copy)"
tmcl world clone 14 "World" --suffix " survival"
```

### `tmcl world remove`

```bash
tmcl world remove [REF] [NAME] [--yes]
```

Goes into the instance trash and can be restored with `trash restore`.

### `tmcl world datapack list`

```bash
tmcl world datapack list [REF] [WORLD]
```

```
vanilla      built-in
dp1.zip      enabled
dp2.zip      disabled
```

### `tmcl world datapack enable` / `disable` / `remove`

```bash
tmcl world datapack enable  [REF] [WORLD] [NAME]
tmcl world datapack disable [REF] [WORLD] [NAME]
tmcl world datapack remove  [REF] [WORLD] [NAME] [--yes]
```

```bash
tmcl world datapack disable 14 "World" dp1.zip
tmcl world datapack remove  14 "World" dp2.zip
```

Enabling removes `.disabled`, disabling adds it, removing puts the file in the trash.

---

## 11. Mod updates: `tmcl mod`

Works by hash lookup on Modrinth, for jars in `mods/` only.

### `tmcl mod check-updates`

```bash
tmcl mod check-updates [REF]
```

```bash
tmcl mod check-updates 14
```

```
Sodium                              0.5.11 -> 0.6.13
Iris                                1.7.3 -> 1.8.1

364 checked, 12 unknown to Modrinth, 2 update(s).
```

Two version columns (current → newest) and a summary line. With nothing to update it prints `No updates (364 checked, 12 unknown to Modrinth).`

Needs the network.

### `tmcl mod update`

```bash
tmcl mod update [REF] [NAME] [--yes]
```

```bash
tmcl mod update 14 sodium.jar
tmcl mod update 14 Sodium --yes
```

The old file moves to the trash before the new one is downloaded; if the download fails, the old file is still in the trash and `trash restore` rolls back.

---

## 12. Download center: `search` / `project` / `install`

Modrinth is the default. CurseForge needs an API key compiled into the build; without one the command says so explicitly.

### `tmcl search`

```bash
tmcl search <QUERY> [--source modrinth|curseforge] [--kind mod|modpack|pack|shader|world] [--limit N]
```

```bash
tmcl search sodium --limit 5
tmcl search "stay true" --kind pack
tmcl search "better mc" --kind modpack --json
```

```
AANobbMI       258125185  Sodium  (by jellysquid3, 1.16.3, 1.16.4, 1.16.5)
```

Four fields: project ID, downloads (`-` when unknown), name, `(author, supported game versions)`. The project ID feeds `project` and `install` directly.

### `tmcl project`

Shows one project and its file list.

```bash
tmcl project <ID> [--source modrinth|curseforge] [--kind mod|modpack|pack|shader|world]
```

```bash
tmcl project AANobbMI
```

### `tmcl install`

```bash
tmcl install <PROJECT> [--instance <REF>] [--file <ID>] [--kind <KIND>] [--source <SOURCE>] [--with-deps]
```

| Option | Description |
|---|---|
| `--instance` | Which instance to install into; the current one by default |
| `--file` | Pick a file by ID; otherwise the newest compatible file is chosen (Release first, incompatible skipped) |
| `--kind` | `mod` (default) / `pack` / `shader` / `modpack` / `world` |
| `--with-deps` | Also install the required dependencies (one level) |

```bash
tmcl install AANobbMI --instance 14
tmcl install AANobbMI --instance 14 --with-deps --dry-run
tmcl install RphJSnEs --instance 14 --kind shader --file 5dwd2E9U
```

`--dry-run` prints the **exact file paths it would write** (dependencies included) without downloading.

---

## 13. Modpacks: `tmcl modpack`

### `tmcl modpack install`

Creates a new instance from a modpack, or applies the pack to an existing instance.

```bash
tmcl modpack install <PROJECT_OR_ARCHIVE> [--instance <REF>] [--name <NAME>] [--game-dir <PATH>] [--source <SOURCE>]
```

```bash
tmcl modpack install "D:\Downloads\pack.mrpack"          # new instance, named after the pack
tmcl modpack install "D:\Downloads\pack.mrpack" --name "My pack"
tmcl modpack install rQiXwLhB --instance 14              # apply to an existing instance
```

A local `.mrpack` / `.zip` or a project ID both work. Overrides, the loader version and the Java requirement are handled as part of the install.

### `tmcl modpack update`

```bash
tmcl modpack update [REF] --from <ARCHIVE> [--source <SOURCE>]
```

```bash
tmcl modpack update 14 --from "D:\Downloads\pack-1.1.mrpack"
```

`--from` is required: a manifest records versions, not a download address. Mods removed by the new version go to the trash.

---

## 14. Self-update: `tmcl update`

No endpoint is compiled into the binary, so both commands need `--url` or `TMCL_UPDATE_URL`.

### `tmcl update check`

```bash
tmcl update check [--url <URL>]
```

```bash
tmcl update check --url https://example.com/tmcl-release.json
```

```
0.3.1 is available (running 0.3.0)
https://example.com/tmcl-0.3.1-windows-x64.zip
```

### `tmcl update apply`

```bash
tmcl update apply [--url <URL>] [--yes]
```

Downloads the artifact built for this machine, replaces the current executable and keeps the old file beside it. It refuses when the release only offers a source archive.

---

## 15. Completions and the TUI: `completions` / `tui`

### `tmcl completions`

```bash
tmcl completions <bash|elvish|fish|powershell|zsh>
```

Loading it in PowerShell:

```powershell
tmcl completions powershell | Out-String | Invoke-Expression
```

Add that to `$PROFILE` to keep it. The script is generated from the parser's own command tree, so it always matches `--help`.

### `tmcl tui`

Running `tmcl` without arguments is the same as `tmcl tui` and opens the full-screen interface.

```bash
tmcl tui
```

`q` / `Esc` exits, `Tab` changes pages; rows, tabs and buttons are clickable and the wheel scrolls.

---

## 16. Adding tmcl to PATH: `tmcl path`

`tmcl` is a single executable, so it only runs from the directory it sits in; unless that directory is on `PATH`, you have to type the full path every time. The TUI's **Settings → General → Add tmcl to PATH** (with one confirmation) and the three commands below do the same thing: they change the **user** PATH only, and need no administrator rights.

### `tmcl path status`

```bash
tmcl path status
```

Three possible results:

```
D:\games is on PATH
D:\games is not on PATH; run `tmcl path add` to add it
D:\games is on PATH only as "\u{202a}D:\\games\\", which does not resolve; run `tmcl path add` to repair it
```

The third is "looks added, does nothing": an invisible character slipped into the entry (common when a path is copied from a chat window or a document) and Windows resolves it to a different directory. `path add` repairs it in place.

`--json` outputs `directory` / `present` / `broken_entry`.

### `tmcl path add`

```bash
tmcl path add
tmcl path add --dry-run
```

Only its own entry is appended; every other entry keeps its value (when a write is really needed the whole string is rejoined with `;` and surrounding whitespace is trimmed); `--dry-run` only says what it would do. Writes keep the `REG_EXPAND_SZ` type (entries such as `%USERPROFILE%\...` are not expanded into concrete paths), and `WM_SETTINGCHANGE` is broadcast afterwards, so terminals opened later pick the change up.

### `tmcl path remove`

```bash
tmcl path remove
```

Removes every spelling that points at this directory, including broken entries with invisible characters.

> **Open a new terminal after a PATH change**: a process copies its parent's environment block at startup, so windows that are already open see nothing. The TUI says the same after a successful change.

## 17. Recipes

**See what an instance has installed**

```bash
tmcl content list 14 --kind mod --sort size
tmcl content list 14 --kind pack
```

**Install a mod with its dependencies**

```bash
tmcl search "jei" --kind mod
tmcl install <ID> --instance 14 --with-deps --dry-run   # inspect the file list first
tmcl install <ID> --instance 14 --with-deps
```

**Disable a few mods to track down a crash**

```bash
tmcl content disable 14 "Sodium" --kind mod
tmcl content disable 14 "Iris" --kind mod
# put them back
tmcl content enable 14 "Sodium" --kind mod
```

**Back up a world before trying new mods**

```bash
tmcl world backup 14 "World"
tmcl world backups 14 "World"
tmcl world restore 14 "World" --from 2026-11-01-2310 --yes
```

**Upgrade every mod that can be upgraded**

```bash
tmcl mod check-updates 14
tmcl mod update 14 Sodium --yes
```

**Export a shareable launch script**

```bash
tmcl instance launch 14 --export-script launch.bat
```

**Stable output for scripts**

```bash
tmcl instance list --json | jq -r '.[] | select(.status=="incomplete") | .path'
tmcl content list 14 --kind mod --json | jq -r '.[] | select(.enabled==false) | .file'
```

**Move an entire instance**

```bash
tmcl instance export 14 --output D:\backups\farming.zip
# on the other machine
tmcl instance import "D:\backups\farming" --version 1.20.1 --name farming
```
