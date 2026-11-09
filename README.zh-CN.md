# TMCL

[English](README.md) | 简体中文

面向 Windows 的 Minecraft Java 版终端启动器。一套内核，两套前端：全屏 TUI 与可脚本化的 CLI。

TMCL 与 Mojang Studios、Microsoft、Forge、Fabric、NeoForge 均无隶属关系。

## 功能

- **实例** —— 创建、导入、校验、修复、导出、启动；支持 Vanilla、Fabric、Quilt、Forge、NeoForge。
- **内容** —— 模组、整合包、资源包、光影与世界，删除先进实例回收站。
- **下载** —— Modrinth 与 CurseForge，支持整合包安装与更新。
- **账号** —— 微软设备码登录，或离线账号。
- **任务** —— 安装、修复与 Java 下载会持久化，可查看、可取消，重启不丢。
- **五套 TUI 预设** —— 页面、键位与数据完全相同，只有绘制层不同。

## 环境要求

Windows 10/11 x86_64、支持 ANSI 的终端（Windows Terminal），以及与所启动版本兼容的 Java 运行时。TMCL 会扫描已安装的运行时，也能下载 Mojang 运行时。

## 构建与运行

~~~bash
cargo run                    # TUI
cargo run -- --help          # CLI
cargo run -- --json instance list
~~~

`cargo build --release` 生成 `tmcl.exe`；`./scripts/package.ps1` 打出发布压缩包。

## 快速上手

~~~bash
tmcl instance create fabric-1.21.1 --version 1.21.1 --loader fabric --loader-version 0.16.5
tmcl instance launch @last
tmcl search sodium --kind mod
tmcl install AANobbMI --instance @last --with-deps
~~~

针对实例的命令都接受 `@last`、名字、UUID、列表编号或路径。完整选项、退出码、JSON 输出与各 shell 的差异见 **[docs/cli.md](docs/cli.md)**（[English](docs/cli.en.md)）。

## 文件位置

| 路径 | 内容 |
| --- | --- |
| `%APPDATA%\TMCL` | 配置、账号、任务、日志、已下载的运行时 |
| `tmcl.exe` 所在目录 | 默认游戏目录（便携布局） |

两者是分开的：启动器数据按用户存放，游戏目录可以在任何位置。「设置 → 游戏目录」可以切换、添加或取消登记。

## TUI 预设

| 预设 | 样子 |
| --- | --- |
| Classic | 带框外壳、顶部标签页、带边框面板（默认） |
| Nightlight | 单一强调色、八套主题、无动画 |
| Paper | 纸面排版，象牙 / 灰白 / 牛皮，白天纸或夜间印张 |
| Shelf | 封面驱动、56/44 分栏、固定配色 |

在「设置 → 外观 → TUI 预设」切换，立即生效。

## 账号

微软登录使用设备码流程：启动器不接触密码，刷新令牌以 Windows DPAPI 存储。离线账号使用标准的 `OfflinePlayer:<名字>` UUID；若 `%APPDATA%\TMCL\authlib-injector.jar` 存在，离线启动会用它提供本地 Yggdrasil 服务，TMCL 不会自行下载该文件。任何命令都不会打印令牌。

## 开发

~~~bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
~~~

联网测试默认标了 `#[ignore]`：`cargo test --test cli_network -- --ignored`。

## 第三方声明

TUI 的部分布局取自 [binsider](https://github.com/orhun/binsider)（MIT OR Apache-2.0）。`nightlight`、`shelf` 两套预设研究过其他终端界面，但未复制代码、常量或素材。参考版本、许可证与确切边界见 [`NOTICE`](NOTICE)。
