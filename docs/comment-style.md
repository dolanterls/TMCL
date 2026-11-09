# TMCL 源码注释准则

- 生效：2026-11-09
- 语体基准：`0.0.1`（`bf25de2`）—— 33 行注释 / 34,936 行代码
- 英文摘要见 `CODEBUDDY.md` 的 Comments 一节，两份同步改
- 逐条清理清单由 `scripts/comment-audit.py --md` 只读生成，改写全部人工完成

## 1. 基调

注释是工程文档。用陈述句和祈使句写，术语直接用，不加修辞。禁止比喻、拟人、玩笑、双关和抒情。

基准是 0.0.1 的写法：

```
/// Extract files under `prefix/` to a dedicated destination, stripping the prefix.
/// Entry names are validated before any file is created; symlink entries and
/// writes through existing symlink ancestors are rejected.
```

作为对照，2026-11-09 清理前后的指标（354 个文件，8,993 行注释）：

| 特征 | 0.0.1 | 清理前 | 清理后 |
|---|---|---|---|
| 破折号 `—`（悬空） | 0 | 349 | **0** |
| `really / actually / just / simply` | 0 | 116 | **0** |
| `used to` / `originally` | 0 | 112 | **0** |
| 比喻（floating / swallow / leak / wallpaper / soup） | 0 | 37 | **0** |
| 点题句（`which is why` / `that is where`） | 0 | 20 | **0** |

清理覆盖 158 个文件，注释净减 222 行。审计脚本现在报 0 块，新代码靠这份准则约束。

## 2. 五种错误写法

### 2.1 用比喻替代术语

术语能准确指代机制，比喻不能，而且每个读者要重新建立一次映射。

| 不写 | 写 |
|---|---|
| the card's scrolling ghosts | the cells the next frame does not repaint |
| the URL soup it used to print | the raw URL chain |
| dependency tables leak into the parsed identity | dependency tables end up in the parsed identity |
| a modpack install can seed it from the project's artwork | a modpack install can set it from the project's artwork |

不属于比喻、不要改：`paper`、`sheet`、`slab`、`hairline` 是 paper 预设的设计词汇（见 specs）；`paint`、`row`、`cell`、`frame` 是 ratatui 的既有术语。

代码标识符里的比喻不在本次范围：`punch_through_terminal`、`seed: Option<Color>`。注释引用它们时用词准确，要改只能重命名，会动 API，单独排期。

### 2.2 把静态机制写成时间序列

讲清机制就够了，不需要还原当时的现场。四个动词串起来的因果链是叙事，不是说明。

不写：

```
a tab from a game log became a cell whose symbol was `\t`, the backend printed
a real tab, and the terminal jumped to the next tab stop — carrying the rest
of the line past the widget's area and past the diagnosis card's border
```

写：

```
`Buffer::set_stringn` filters them, but `Paragraph` writes graphemes directly
into cells, so a tab is stored as a cell symbol and printing it moves the
cursor to the next tab stop. The rest of the line is then drawn outside the
widget, and no later frame repaints those cells.
```

判断方法：句子里有三个以上隐含"然后"串联的动作，改为说明谁存放了什么、由此导致什么。

### 2.3 点题收尾

给出结论之后停止。段末再补一句总结是文章写法，注释里没有位置。

- `That is where the card's scrolling ghosts came from.`
- `Which is why only classic used to show it.`
- `This is the whole point of the trait.`

### 2.4 破折号转折

`—` 在注释里只有一种用途：制造"前面铺垫、后面揭晓"的节奏。技术说明不需要这个节奏，改为冒号、句号或直接拆句。

例外：并列项之间成对包住（`the decisions — which provider, which file — live in core`）可以保留，它们不构成转折。

### 2.5 拟人

- `the terminal jumped to the next tab stop` → `printing moves the cursor to the next tab stop`
- `the field prints the first half` → `the field shows the first half`
- `a same-named instance wins` → `a same-named instance takes precedence`

## 3. 三种注释，三个位置

| 形态 | 位置 | 上限 | 内容 |
|---|---|---|---|
| `//!` | 文件头 | 3 行，硬上限 6 | 这个模块负责什么；为什么它单独存在 |
| `///` | 项上 | 首行 ≤100 字符，正文 ≤3 行 | 契约；不显然的约束 |
| `//` | 行内 | 2 行 | 只有代码看起来像有别的解释时才写 |

`//!` 超过 6 行就是设计文档，搬去 `docs/superpowers/specs/`，源码留一句加路径。

## 4. 三个准入测试

动笔前问：

- **删除测试**：删掉它，读者看代码会不会误解？不会 → 不写
- **复述测试**：是否只是把代码翻译成英文？是 → 不写
- **过期测试**：这段代码改动后，注释会不会立刻变假？会 → 写成规则，不要描述现状

## 5. 历史信息

git 已经记录了"以前是什么样"。源码只留会再犯的坑，压成一句禁令：

保留：`Do not scope a shared row by reading only nightlight/; check the paint path.`

删除：`Reading only nightlight/ when this table was first written put the three shared rows behind nightlight and took them away from classic.`

判断方法：这条历史在下次重构时会不会再犯？会 → 写成禁令；不会 → 删。

## 6. 保留不动

- **界面字面量**：`跟随终端`、`需重新登录` 这类原样保留。测试按界面原文匹配，翻成英文等于让注释撒谎。
- **安全与兼容禁令**：破坏性操作、DPAPI、路径改写的约束，一条都不删。
- **外部规范 URL**。

## 7. 预算

- 单文件注释行 / 总行数 ≤ 20%（文件不足 150 行时不看密度）
- 单个 `///` 块 ≤ 6 行
- 单个函数超过 200 行时，先怀疑函数本身，而不是给它加注释

存量：全库 7.0%，15 个文件超过 20%，`view/sheet.rs` 最高。清理顺序按标记数倒排，先 tui 后 core。

## 8. 两条边界

密度不是验收标准。`view/sheet.rs` 是纯 trait，代码只有 10 行，压到 20% 以下只能删有效信息。

不要为了达标删掉有效信息。一条说明约束的注释，价值高于三条正确的废话。
