# TMCL 命令行手册

[English](cli.en.md) | 简体中文

TMCL 的所有能力都在命令行里，TUI 只是同一套内核的另一层界面。这份手册按命令组列出**每一条命令**的用途、语法、选项和示例。

- 安装、账号、加载器的背景知识在 [README.zh-CN.md](../README.zh-CN.md)。
- 命令之间的设计取舍在 `docs/superpowers/specs/`。
- 本文以 Windows PowerShell 为例；bash / cmd 里同样的命令都能跑，差异见 [§1.6](#16-在-powershell-bash-cmd-里不一样的地方)。

---

## 目录

- [1. 通用规则](#1-通用规则)
  - [1.1 全局选项](#11-全局选项)
  - [1.2 输出与 `--json`](#12-输出与---json)
  - [1.3 退出码](#13-退出码)
  - [1.4 确认与 `--dry-run`](#14-确认与---dry-run)
  - [1.5 实例引用 `[REF]`](#15-实例引用-ref)
  - [1.6 在 PowerShell / bash / cmd 里不一样的地方](#16-在-powershell--bash--cmd-里不一样的地方)
  - [1.7 内容名 `<NAME>` 怎么匹配](#17-内容名-name-怎么匹配)
- [2. 实例：`tmcl instance`](#2-实例tmcl-instance)
- [3. 账号：`tmcl account`](#3-账号tmcl-account)
- [4. Java：`tmcl java`](#4-javatmcl-java)
- [5. 游戏版本：`tmcl version`](#5-游戏版本tmcl-version)
- [6. 任务：`tmcl task`](#6-任务tmcl-task)
- [7. 配置：`tmcl config`](#7-配置tmcl-config)
- [8. 实例内容：`tmcl content`](#8-实例内容tmcl-content)
- [9. 回收站：`tmcl trash`](#9-回收站tmcl-trash)
- [10. 世界与数据包：`tmcl world`](#10-世界与数据包tmcl-world)
- [11. 模组更新：`tmcl mod`](#11-模组更新tmcl-mod)
- [12. 下载中心：`search` / `project` / `install`](#12-下载中心search--project--install)
- [13. 整合包：`tmcl modpack`](#13-整合包tmcl-modpack)
- [14. 自更新：`tmcl update`](#14-自更新tmcl-update)
- [15. 补全与界面：`completions` / `tui`](#15-补全与界面completions--tui)
- [16. 把 tmcl 加入 PATH：`tmcl path`](#16-把-tmcl-加入-pathtmcl-path)
- [17. 常见任务配方](#17-常见任务配方)

---

## 1. 通用规则

### 1.1 全局选项

下面五个选项对**每一条命令**都有效，写在命令的任意位置都可以：

| 选项 | 作用 |
|---|---|
| `--json` | 结果输出为 JSON；进度改为 stderr 上的逐行 NDJSON |
| `--quiet` | 不打印进度，只打印最终结果 |
| `--dry-run` | 只打印计划，不改动任何东西 |
| `--no-color` | 关闭颜色（stdout 不是终端时本来就不上色） |
| `--verbose` | 把 HTTP 请求、任务事件写到 stderr；等价于环境变量 `TMCL_LOG=debug` |

`--quiet` 与 `--json` 同时给时，`--json` 的结果照常输出，进度完全静默。

### 1.2 输出与 `--json`

- **stdout 只放结果**：列表、单个对象、确认后的结论。
- **stderr 放进度、阶段、警告和错误**。所以 `tmcl ... > out.txt` 永远不会把进度混进文件。
- 加 `--json` 后，stderr 的进度是一条一条独立的 JSON（NDJSON），每行形如：

  ```json
  {"event":"start","label":"Install sodium","done":0,"total":0}
  {"event":"progress","label":"Install sodium","done":42,"total":100}
  {"event":"done","label":"Install sodium","done":100,"total":100}
  ```

  可以这样消费：

  ```bash
  tmcl install AANobbMI --instance 14 --json 2> events.ndjson | jq .
  jq -r 'select(.event=="progress") | "\(.done)/\(.total)"' events.ndjson
  ```

- 出错时 stdout 为空，stderr 打出 `{"error": "...", "category": "Network"}`（带 `--json` 时）。

### 1.3 退出码

| 退出码 | 含义 |
|---|---|
| `0` | 成功 |
| `2` | 参数或配置错误（含"找不到实例"） |
| `3` | 网络或下载失败 |
| `4` | 游戏版本或 Java 相关错误 |
| `5` | 账号错误 |
| `6` | 加载器安装/解析错误 |
| `7` | 启动失败 |
| `8` | 终端相关错误 |
| `130` | 被 Ctrl-C 中断 |

脚本里判断成功与否只看 `0` 或非 `0`；要区分原因再看具体值。

### 1.4 确认与 `--dry-run`

会改变磁盘的命令（`delete`、`remove`、`purge`、`restore`、`update`、`cancel`、`mod update`、`update apply`）走三道闸门：

1. **交互确认**：stderr 是终端时问 `Continue? [y/N]`。
2. **`--yes`**：跳过询问，适合脚本。
3. **`--dry-run`**：只打印计划，什么都不做。

stderr 不是终端、又没给 `--yes` 或 `--dry-run` 时，命令**直接拒绝并以 2 退出**，而不是默默执行。

`--dry-run --json` 会输出计划本身，例如：

```json
{"plan": ["delete D:\\games\\versions\\old"], "applied": false}
```

### 1.5 实例引用 `[REF]`

凡是针对实例的命令都接受同一个引用语法：

| 写法 | 含义 | 例子 |
|---|---|---|
| 省略 | 等于 `@last`：配置里记录的当前实例 | `tmcl content list` |
| `@last` | 同上（PowerShell 里请省略或加引号，见 1.6） | `tmcl instance show @last` |
| 数字 | `instance list` 的第 n 行 | `tmcl instance launch 3` |
| `+数字` | 一定是第 n 行（跳过同名实例） | `tmcl instance launch +3` |
| 名字 | 精确匹配，或**唯一**前缀 | `tmcl instance launch ArdaCraft` / `launch arda` |
| UUID | `instance list --json` 里的 `id` | `tmcl instance show 33333333-...` |
| 路径 | 实例目录本身，不需要预先注册 | `tmcl instance show "D:\games\versions\demo"` |

规则细节：

- **数字是位置，不是身份**。删掉或新装一个实例，后面的编号都会变。脚本里请用名字、UUID 或路径。
- **同名实例优先**：如果真有一个实例叫 `123`，`tmcl instance launch 123` 命中的是它；要按行号取就写 `+123`。
- **前缀只在前述规则都没命中时才用**，且必须唯一。`farming` 命中三个时会被拒绝并列出候选：

  ```
  configuration error: instance reference is ambiguous: farming matches 3 instances;
  use a path or a number instead:
    FarmingTales_1.2  (D:\...\FarmingTales_1.2)
    FarmingTales_1.6.8  (D:\...\FarmingTales_1.6.8)
    FarmingTales_ForgeⅡ v1.5.2fix2  (D:\...\FarmingTales_ForgeⅡ v1.5.2fix2)
  ```

- 位置参数被占用的命令（`install`、`modpack install/update`）改用 `--instance <REF>`。
- 名字含空格要加引号：`tmcl instance launch "BattleArmory TACZ 26.0.5"`。
- 同名实例出现在两个游戏目录里是**错误**，不会替你挑一个。

### 1.6 在 PowerShell / bash / cmd 里不一样的地方

| 场景 | 注意 |
|---|---|
| PowerShell 的 `@` | `@xxx` 会被当成 splatting **静默丢掉**。所以 `instance launch @9` 等于没写引用（会启动当前实例），`@last` 也一样——省略它，或写成 `'@last'`。 |
| 名字含空格 | PowerShell 里必须加引号，否则后半截被当成多余参数。 |
| `#` | PowerShell / bash 里是注释，不要用它做前缀。 |
| 数字引用 | 三种 shell 都安全，推荐脚本外使用。 |

### 1.7 内容名 `<NAME>` 怎么匹配

`content enable/disable/remove`、`trash restore`、`world *`、`mod update` 里的 `<NAME>`：

1. 先按**文件名**匹配（如 `sodium-0.6.13.jar`），忽略大小写；
2. 没命中再按**显示名**匹配（如 `Sodium`），同样忽略大小写；
3. 命中多个会报错并列出候选，命中零个会给出最接近的名字。

列表里打印的是显示名，但两种都能用。

---

## 2. 实例：`tmcl instance`

### `tmcl instance list`

列出所有实例，每行第一列是**行号**，可直接用于 `[REF]`。

```bash
tmcl instance list [--status ready|incomplete|all] [--game-dir PATH]
```

| 选项 | 说明 |
|---|---|
| `--status` | 只列 `ready`（文件齐）或 `incomplete`（缺文件），默认 `all` |
| `--game-dir` | 只列这个游戏目录里的实例 |

```bash
tmcl instance list
tmcl instance list --status incomplete
tmcl instance list --json | jq -r '.[] | "\(.index) \(.name)"'
```

文本列：`行号 名称 版本 加载器 状态 路径`。窗口窄时路径会从左截断（保留实例名），再窄会整列省略；重定向到文件时永远输出完整路径。加了 `--status` 过滤后行号会跳号——因为编号始终按全量列表计算，`+n` 的含义才不随过滤变化。

### `tmcl instance show`

显示一个实例的全部信息。

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

`missing files` 为 `unknown (no readable version JSON)` 时说明版本 JSON 读不出来。

### `tmcl instance create`

新建实例并安装对应加载器。

```bash
tmcl instance create <NAME> --version <MINECRAFT_VERSION> [--loader <LOADER>] [--loader-version <VERSION>] [--game-dir PATH]
```

| 选项 | 说明 |
|---|---|
| `--version` | 必填，如 `1.21.1` |
| `--loader` | `vanilla`（默认）/ `fabric` / `quilt` / `forge` / `neoforge` |
| `--loader-version` | 非 Vanilla 加载器**必须**给出确切版本 |
| `--game-dir` | 建在哪个游戏目录，默认第一个已配置的 |

```bash
tmcl instance create "Fabric 1.21.1" --version 1.21.1 --loader fabric --loader-version 0.16.5
tmcl instance create "Forge 1.20.1" --version 1.20.1 --loader forge --loader-version 47.4.10
tmcl instance create demo --version 1.21.1 --dry-run      # 只打印将创建的目录
```

安装过程是持久化任务，Ctrl-C 会取消任务并以 130 退出。

### `tmcl instance import`

把一个已存在的版本目录纳入管理（PCL2、HMCL 或手动放的都行）。

```bash
tmcl instance import <INSTANCE_DIR> --version <MINECRAFT_VERSION> [--name <NAME>] [--game-directory-id <UUID>]
```

```bash
tmcl instance import "D:\games\versions\1.20.1-forge-47.4.10" --version 1.20.1 --name "Forge 生存"
```

只登记，不复制文件；`--name` 会同时重命名目录和 `<name>.jar/.json`。

### `tmcl instance rename`

改名，同时重命名版本目录、`<old>.jar/.json` 和元数据。

```bash
tmcl instance rename [REF] [NEW_NAME]
```

```bash
tmcl instance rename 14 "FarmingTales 整合包"
```

当前实例标记会跟着改。**注意**：游戏内显示的版本名、PCL2 里看到的目录名都会变。

### `tmcl instance repair`

补齐缺失文件（客户端 jar、库、资源索引），已存在的不重下。

```bash
tmcl instance repair [REF]
```

```bash
tmcl instance repair 3
tmcl instance repair --dry-run          # 只列出会下载什么
tmcl instance repair --json
```

JSON：`{"missing": 3, "downloaded": 3}`。

### `tmcl instance export`

把实例目录打包成 zip。

```bash
tmcl instance export [REF] [--output <PATH>]
```

```bash
tmcl instance export 14 --output D:\backups\farming.zip
tmcl instance export 14                       # 默认 <实例名>.zip
```

目标文件已存在时**拒绝覆盖**。`--output` 是 zip 文件路径，默认落在当前目录的 `<实例名>.zip`。

打包几百 MB 的实例会明显耗时；`--dry-run` 只打印 "Would pack … into …" 并立即返回。

### `tmcl instance verify`

对照版本 JSON 检查实例文件（是否存在，`--full` 时还校验 sha1）。

```bash
tmcl instance verify [REF] [--full] [--client-json <PATH>]
```

```bash
tmcl instance verify 3
tmcl instance verify 3 --full
tmcl instance verify 3 --json | jq '.missing[].path'
```

| 选项 | 说明 |
|---|---|
| `--full` | 除存在性外还比对 sha1（慢） |
| `--client-json` | 用指定的版本 JSON 当基准，默认用实例自己的 |

只会核对**本机真的会用到的**文件：版本 JSON 里带 `rules` 且只对别的系统生效的库（例如
`1.20.1` 里 `allow osx` 的 `java-objc-bridge`）不算缺失；natives 也只取 `natives` 映射里
当前系统 + 位数对应的那一个。库文件按 `libraries/<path>` 定位，和启动时拼 classpath 的规则一致。

```
Checked 64 files; 0 missing or invalid.
```

`missing[].path` 是相对游戏目录的路径（如 `libraries/ca/weblite/...jar`），`url` 与 `sha1`
可以直接拿去补文件。注意它只读实例自己的版本 JSON，`inheritsFrom` 的父版本不在核对范围内。

### `tmcl instance launch`

启动游戏，并等待进程结束。

```bash
tmcl instance launch [REF] [--account-id <UUID>] [--java <PATH>] [--min-memory-mb N] [--max-memory-mb N] [--width N] [--height N] [--fullscreen] [--client-json <PATH>] [--export-script <PATH>]
```

| 选项 | 说明 |
|---|---|
| `--account-id` | 用哪个账号；默认当前选中的账号 |
| `--java` | 指定 java 可执行文件；默认按 `config java` 的策略选 |
| `--min-memory-mb` / `--max-memory-mb` | 堆内存下限/上限 |
| `--width` / `--height` / `--fullscreen` | 窗口尺寸与全屏 |
| `--client-json` | 用别的版本 JSON 启动（调试用） |
| `--export-script` | **不启动游戏**，把命令行写成 `.bat` / `.ps1` / `.sh` |

```bash
tmcl instance launch 14
tmcl instance launch 14 --max-memory-mb 8192
tmcl instance launch 14 --export-script launch.bat
```

导出的脚本里，会话令牌会替换成环境变量引用（`TMCL_ACCESS_TOKEN`），可以直接发给别人；离线账号没有令牌，脚本里就是字面量 `0`。游戏输出同时写进 `%APPDATA%\TMCL\logs\game\<uuid>.log`（`logs.game_debug` 关时只记 stderr，失败启动会补上 stdout 尾部）。

### `tmcl instance unregister`

从列表里移除（**保留文件**）。

```bash
tmcl instance unregister [REF] [--yes]
```

```bash
tmcl instance unregister 14
```

### `tmcl instance delete`

删除实例目录及其中所有文件。

```bash
tmcl instance delete [REF] [--yes]
```

```bash
tmcl instance delete "old test" --yes
tmcl instance delete 14 --dry-run          # 只打印将删除的路径
```

如果它是当前实例，标记会被清空。

---

## 3. 账号：`tmcl account`

### `tmcl account list`

```bash
tmcl account list [--json]
```

```
11111111-1111-4111-8111-111111111111  Offline    Alex
22222222-2222-4222-8222-222222222222  Microsoft  Steve
```

第一列是 `account remove` 要的 ID，不是用户名。

### `tmcl account login-microsoft`

走设备码流程：打印一个网址和代码，浏览器里确认后自动保存。

```bash
tmcl account login-microsoft
```

令牌用 Windows DPAPI 加密存在 `%APPDATA%\TMCL`。

### `tmcl account add-offline`

```bash
tmcl account add-offline <USERNAME> [--uuid <UUID>]
```

```bash
tmcl account add-offline Steve
tmcl account add-offline Steve --uuid 00000000-0000-3000-8000-000000000002
```

不给 `--uuid` 时由用户名派生（同一名字永远得到同一个 UUID）。

### `tmcl account remove`

```bash
tmcl account remove <ACCOUNT_ID>
```

`ACCOUNT_ID` 是 `account list` 第一列，不是用户名。

---

## 4. Java：`tmcl java`

### `tmcl java list`

列出探测到的运行时（含 TMCL 自己下载的）。

```bash
tmcl java list [--json]
```

```
Java 25  C:\Users\you\AppData\Roaming\TMCL\runtime\java-25\bin\java.exe
Java 21  C:\Users\you\AppData\Roaming\.minecraft\runtime\java-runtime-delta\bin\java.exe
Java 8   C:\Users\you\AppData\Roaming\.minecraft\runtime\jre-legacy\bin\java.exe
```

第一列是主版本号，第二列是可执行文件路径。

### `tmcl java scan`

重新扫描并报告结果数量（TUI 启动时会用同一份扫描）。

```bash
tmcl java scan
```

```
Scanned 6 directories; found 3 runtimes.
```

### `tmcl java download`

下载 Mojang 提供的运行时。

```bash
tmcl java download <MAJOR> [--manifest-url <URL>]
```

```bash
tmcl java download 21
tmcl java download 17 --dry-run
```

下载到 `%APPDATA%\TMCL\runtime\java-<major>`，之后 `config java auto` 就会选到它。

---

## 5. 游戏版本：`tmcl version`

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

两列：版本 id 与类型。需要联网（Mojang 版本清单）。

---

## 6. 任务：`tmcl task`

所有安装、修复、下载都会落盘成任务，两个前端共享。

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

跟随一个任务直到结束，进度打印到 stderr。

```bash
tmcl task watch <TASK_ID>
```

```bash
tmcl task watch 44444444-4444-4444-8444-444444444444
tmcl task watch 44444444-... --json 2> events.ndjson
```

别的进程里的任务也能看。如果记录它那个 TMCL 进程已经退出，watch 会先把这条残留任务收尾成 `failed`（`failure` 为 `Interrupted when TMCL exited`，可 `task retry` 重跑），然后再返回。

### `tmcl task retry`

重跑一个失败的任务。

```bash
tmcl task retry <TASK_ID>
```

### `tmcl task cancel`

取消并删除任务记录。

```bash
tmcl task cancel <TASK_ID> [--yes]
```

---

## 7. 配置：`tmcl config`

### `tmcl config path`

打印数据目录（配置、任务、日志、运行时都在它下面）。

```bash
tmcl config path
```

```
C:\Users\you\AppData\Roaming\TMCL
```

### `tmcl config list`

按 `key = value` 打印所有设置。

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

值的写法就是 `config set` 接受的写法：布尔是 `true`/`false`，空值是 `null`，列表是 JSON 数组，枚举是**大写**（`DEFAULT` / `OFFICIAL` / `MIRROR`）。

### `tmcl config get`

```bash
tmcl config get <KEY>
```

```bash
tmcl config get downloads.retry_count
```

### `tmcl config set`

值会按目标字段的类型解析。

```bash
tmcl config set <KEY> <VALUE>
```

```bash
tmcl config set downloads.retry_count 5
tmcl config set downloads.file_download_source MIRROR
tmcl config set logs.game_debug true
tmcl config set downloads.cache_dir "D:/cache/tmcl"
tmcl config set downloads.cache_dir null          # 清空
```

未知键会报错并给出最接近的合法键。

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

`remove` 只是不再管理，**不删文件**。第一个目录是 `create` / `modpack install` 的默认落点。

### `tmcl config java`

```bash
tmcl config java auto         # 自动挑最合适的运行时
tmcl config java path <PATH>  # 固定使用某个 java
```

```bash
tmcl config java path "C:\Program Files\Java\jdk-21\bin\java.exe"
```

`auto` 会按游戏版本需要的 Java 大版本挑；`path` 之后所有启动都用它。

---

## 8. 实例内容：`tmcl content`

管理 `mods/`、`resourcepacks/`、`shaderpacks/`。资源包的启用状态写在 `options.txt`，光影写在对应 loader 的配置，其它用 `.disabled` 后缀。

### `tmcl content list`

```bash
tmcl content list [REF] [--kind mod|pack|shader] [--sort name|size|status] [--filter <TEXT>]
```

| 选项 | 说明 |
|---|---|
| `--kind` | `mod`（默认）/ `pack` / `shader` |
| `--sort` | `name`（默认）/ `size` / `status`；资源包和光影的排序无效（必须保持游戏读取顺序） |
| `--filter` | 只列名字包含该串的条目 |

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

JSON 每条含 `name`/`file`/`kind`/`version`/`size`/`enabled`/`order`/`warning`。

### `tmcl content enable` / `disable`

```bash
tmcl content enable  [REF] [NAME] --kind mod|pack|shader
tmcl content disable [REF] [NAME] --kind mod|pack|shader
```

```bash
tmcl content disable 14 sodium.jar --kind mod
tmcl content enable  14 "Stay True.zip" --kind pack
```

`--kind` 必填（这里不做猜测）。对已经处于该状态的条目是无操作。

### `tmcl content remove`

```bash
tmcl content remove [REF] [NAME] --kind <KIND> [--purge] [--yes]
```

```bash
tmcl content remove 14 sodium.jar --kind mod          # 进实例回收站
tmcl content remove 14 sodium.jar --kind mod --purge  # 直接删
```

移除资源包/光影时会同步清理 `options.txt` 或 loader 配置里的条目，不留悬空引用。误删可以用 `trash restore` 找回（`--purge` 除外）。

### `tmcl content add`

把本地文件或 URL 复制/下载进实例。

```bash
tmcl content add [REF] [FILE_OR_URL] [--kind mod|pack|shader] [--yes]
```

```bash
tmcl content add 14 "D:\Downloads\Stay True.zip" --kind pack
tmcl content add 14 https://example.com/mod.jar
```

不给 `--kind` 时按扩展名推断：`.jar` → 模组，`.zip` → 资源包（整合包不适用这条命令）。其它扩展名会直接报 `cannot tell what … is`，需要显式给 `--kind`。

### `tmcl content order`

调整资源包在栈里的位置（越靠上优先级越高）。

```bash
tmcl content order [REF] [NAME] --up|--down
```

```bash
tmcl content order 14 "Stay True.zip" --up
```

只有资源包有顺序；光影和模组的 `--kind` 会被拒绝。

---

## 9. 回收站：`tmcl trash`

每个实例有自己的回收站，`content remove`、`world remove`、`world datapack remove` 都往这里放。

### `tmcl trash list`

```bash
tmcl trash list [REF]
```

```
2026-11-01-2312  sodium.jar                          2.1 MB
2026-11-01-2240  saves/旧存档                        43.0 MB
```

三列：时间戳、条目名、大小。空的时候打印 `The trash is empty.`。

### `tmcl trash restore`

```bash
tmcl trash restore [REF] [NAME]
```

```bash
tmcl trash restore 14 sodium.jar
```

目标位置已有同名文件时会报错，不会覆盖。

### `tmcl trash purge`

```bash
tmcl trash purge [REF] [--yes]
```

```bash
tmcl trash purge 14 --yes
```

---

## 10. 世界与数据包：`tmcl world`

### `tmcl world list`

```bash
tmcl world list [REF]
```

```
世界     1.20.1     2026-10-17 08:55     1.3 MB
世界222  1.20.1     2026-10-20 15:17    43.0 MB
```

### `tmcl world info`

读 `level.dat` 里的信息（版本、种子、游戏时间等）。

```bash
tmcl world info [REF] [NAME]
```

```bash
tmcl world info 14 "世界"
```

```
name: 世界
folder: 世界
version: 1.20.1
last played: 2026-10-17 08:55
game mode: survival
hardcore: no
size: 1.3 MB
path: D:\...\saves\世界
```

### `tmcl world backup`

打包到实例的备份目录（`<实例>/.tmcl-backups/worlds`）。

```bash
tmcl world backup [REF] [NAME]
```

```bash
tmcl world backup 14 "世界"
```

### `tmcl world backups`

```bash
tmcl world backups [REF] [NAME]
```

```
2026-11-01-2310     42.8 MB  D:\...\.tmcl-backups\worlds\世界-2026-11-01-2310.zip
2026-10-30-1201     41.2 MB  D:\...\.tmcl-backups\worlds\世界-2026-10-30-1201.zip
```

三列：时间戳、大小、备份文件路径。没有备份时打印 `No backups of 世界.`。第一列是 `world restore --from` 接受的**时间戳**。

### `tmcl world restore`

用备份替换当前世界（先确认）。

```bash
tmcl world restore [REF] [NAME] --from <ARCHIVE> [--yes]
```

```bash
tmcl world restore 14 "世界" --from 2026-11-01-2310
tmcl world restore 14 "世界" --from D:\backups\world.zip
```

### `tmcl world clone`

```bash
tmcl world clone [REF] [NAME] [--suffix <TEXT>]
```

```bash
tmcl world clone 14 "世界"                 # 得到 "世界 (copy)"
tmcl world clone 14 "世界" --suffix " 生存"
```

### `tmcl world remove`

```bash
tmcl world remove [REF] [NAME] [--yes]
```

进实例回收站，可用 `trash restore` 找回。

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
tmcl world datapack disable 14 "世界" dp1.zip
tmcl world datapack remove  14 "世界" dp2.zip
```

启用=去掉 `.disabled`，禁用=加 `.disabled`，移除=进回收站。

---

## 11. 模组更新：`tmcl mod`

基于 Modrinth 的哈希查找，只对 `mods/` 里的 jar 生效。

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

两列版本（当前 → 最新），末尾一行汇总。没有可更新的会打印 `No updates (364 checked, 12 unknown to Modrinth).`

需要联网。

### `tmcl mod update`

```bash
tmcl mod update [REF] [NAME] [--yes]
```

```bash
tmcl mod update 14 sodium.jar
tmcl mod update 14 Sodium --yes
```

旧版本先移到回收站再下载新文件；下载失败时旧文件还在回收站里，`trash restore` 可回滚。

---

## 12. 下载中心：`search` / `project` / `install`

默认走 Modrinth。CurseForge 需要编译期带 API key，否则会明确报"不支持"。

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

四段：项目 ID、下载量（未知时是 `-`）、名称、`(作者, 支持的游戏版本)`。项目 ID 直接喂给 `project` / `install`。

### `tmcl project`

看一个项目及其文件列表。

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

| 选项 | 说明 |
|---|---|
| `--instance` | 装进哪个实例；不写用当前实例 |
| `--file` | 指定文件 ID，不写则自动挑（优先 Release、跳过不兼容） |
| `--kind` | `mod`（默认）/ `pack` / `shader` / `modpack` / `world` |
| `--with-deps` | 一并安装声明为 required 的依赖（一层） |

```bash
tmcl install AANobbMI --instance 14
tmcl install AANobbMI --instance 14 --with-deps --dry-run
tmcl install RphJSnEs --instance 14 --kind shader --file 5dwd2E9U
```

`--dry-run` 会打印**确切要写的文件路径**（含依赖），不下载。

---

## 13. 整合包：`tmcl modpack`

### `tmcl modpack install`

从整合包新建实例，或把包套用到已有实例。

```bash
tmcl modpack install <PROJECT_OR_ARCHIVE> [--instance <REF>] [--name <NAME>] [--game-dir <PATH>] [--source <SOURCE>]
```

```bash
tmcl modpack install "D:\Downloads\pack.mrpack"          # 新建实例，名字取自包
tmcl modpack install "D:\Downloads\pack.mrpack" --name "我的整合"
tmcl modpack install rQiXwLhB --instance 14              # 套用到已有实例
```

`.mrpack` / `.zip` 本地文件或项目 ID 都行。会一并处理包里的 overrides、加载器版本和 Java 版本要求。

### `tmcl modpack update`

```bash
tmcl modpack update [REF] --from <ARCHIVE> [--source <SOURCE>]
```

```bash
tmcl modpack update 14 --from "D:\Downloads\pack-1.1.mrpack"
```

必须给 `--from`：清单只记录版本号，不记录下载地址。新版本删掉的模组会移入回收站。

---

## 14. 自更新：`tmcl update`

端点没有编译进二进制，所以两条命令都需要 `--url` 或 `TMCL_UPDATE_URL`。

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

下载对应平台的产物，替换当前可执行文件，旧文件留在旁边。只提供源码包时会拒绝执行。

---

## 15. 补全与界面：`completions` / `tui`

### `tmcl completions`

```bash
tmcl completions <bash|elvish|fish|powershell|zsh>
```

PowerShell 里加载：

```powershell
tmcl completions powershell | Out-String | Invoke-Expression
```

写进 `$PROFILE` 可以长期生效。补全脚本由解析器本身的命令树生成，和 `--help` 永远一致。

### `tmcl tui`

不带任何参数运行 `tmcl` 等价于 `tmcl tui`，打开全屏界面。

```bash
tmcl tui
```

界面里 `q`/`Esc` 退出、`Tab` 切页；鼠标可点行、标签页、按钮、滚轮滚动。

---

## 16. 把 tmcl 加入 PATH：`tmcl path`

`tmcl` 只有一个可执行文件，放在哪个目录就只能从哪里运行；除非那个目录在 `PATH` 里，否则每次都得敲完整路径。TUI 的 **设置 → 常规 → 把 tmcl 加入 PATH**（回车后有一次确认）和下面这三条命令做的是同一件事：都只改**用户** PATH，不需要管理员权限。

### `tmcl path status`

```bash
tmcl path status
```

三种结果：

```
D:\games is on PATH
D:\games is not on PATH; run `tmcl path add` to add it
D:\games is on PATH only as "\u{202a}D:\\games\\", which does not resolve; run `tmcl path add` to repair it
```

第三种是"看着加了、其实没用"：条目里混进了不可见字符（从聊天窗口或文档复制路径时很常见），Windows 按字面解析成另一个目录。`path add` 会就地修正它。

`--json` 输出 `directory` / `present` / `broken_entry`。

### `tmcl path add`

```bash
tmcl path add
tmcl path add --dry-run
```

只追加自己这一条，其它条目的值保持不变（真的需要写入时，整串会按 `;` 重新拼接，条目首尾空白会被去掉）；`--dry-run` 只说明会做什么。写入时保持 `REG_EXPAND_SZ` 类型（`%USERPROFILE%\...` 这类条目不会被展开成具体路径），写完广播 `WM_SETTINGCHANGE`，所以之后新开的终端能直接生效。

### `tmcl path remove`

```bash
tmcl path remove
```

删掉所有指向本目录的写法，包括带不可见字符的坏条目。

> **改完 PATH 必须新开终端**：进程启动时复制父进程的环境块，已经开着的窗口看不到变化。TUI 里执行成功后也会提示这一句。

## 17. 常见任务配方

**看这个实例装了什么**

```bash
tmcl content list 14 --kind mod --sort size
tmcl content list 14 --kind pack
```

**装一个模组并带依赖**

```bash
tmcl search "jei" --kind mod
tmcl install <ID> --instance 14 --with-deps --dry-run   # 先看文件清单
tmcl install <ID> --instance 14 --with-deps
```

**关掉几个模组排查崩溃**

```bash
tmcl content disable 14 "Sodium" --kind mod
tmcl content disable 14 "Iris" --kind mod
# 复原
tmcl content enable 14 "Sodium" --kind mod
```

**备份世界再试新模组**

```bash
tmcl world backup 14 "世界"
tmcl world backups 14 "世界"
tmcl world restore 14 "世界" --from 2026-11-01-2310 --yes
```

**升级所有能升级的模组**

```bash
tmcl mod check-updates 14
tmcl mod update 14 Sodium --yes
```

**导出可分享的启动脚本**

```bash
tmcl instance launch 14 --export-script launch.bat
```

**给脚本用的稳定输出**

```bash
tmcl instance list --json | jq -r '.[] | select(.status=="incomplete") | .path'
tmcl content list 14 --kind mod --json | jq -r '.[] | select(.enabled==false) | .file'
```

**转移整个实例**

```bash
tmcl instance export 14 --output D:\backups\farming.zip
# 另一台机器
tmcl instance import "D:\backups\farming" --version 1.20.1 --name farming
```
