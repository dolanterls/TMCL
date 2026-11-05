//! Instance workspace rendering.
//!
//! The body is shared: label/value rows, launch editors and content lists are
//! drawn once and coloured per preset. What differs is the frame around it —
//! `creeper` puts a category rail beside a rounded card, `classic` puts the
//! categories in a tab row above a plain panel, matching how those presets
//! render their settings pages.

use super::*;
use crate::core::instance::models::InstanceLoader;
use crate::core::instance::overrides::{EffectiveLaunch, JavaChoice};
use crate::tui::app::view::paper::{PaperPalette, ink};
use crate::tui::pages::workspace::{LaunchField, Workspace, WorkspaceCategory};
use crate::tui::palette;
use ratatui::symbols::border;
use ratatui::widgets::Borders;

/// How a launch row is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::app) enum LaunchFieldKind {
    Toggle,
    Number { step: u64, min: u64, max: u64 },
    Text,
    Picker,
}

/// One rendered, editable launch setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::tui::app) struct LaunchRow {
    pub field: LaunchField,
    pub label: String,
    /// Display value. Adjustable rows get `‹ ›` from the renderer, so the
    /// hit boxes can sit exactly on the glyphs they draw.
    pub value: String,
    pub kind: LaunchFieldKind,
    /// True when the instance follows the global value.
    pub following: bool,
    pub enabled: bool,
    pub disabled_reason: String,
}

const LABEL_WIDTH: usize = 16;
const RAIL_WIDTH: u16 = 22;

impl App {
    /// Rows of the current category as `(label, value)`.
    pub(in crate::tui::app) fn workspace_display_rows(&self) -> Vec<(String, String)> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Vec::new();
        };
        let text = |key: &str| self.i18n.text(key);
        match workspace.category {
            WorkspaceCategory::Overview => self.workspace_overview_rows(workspace),
            WorkspaceCategory::Launch => self
                .launch_rows()
                .into_iter()
                .map(|row| (row.label, row.value))
                .collect(),
            WorkspaceCategory::Install => self.workspace_install_rows(workspace),
            WorkspaceCategory::Manage => vec![
                (text("workspace.manage.rename"), String::new()),
                (text("workspace.manage.repair"), String::new()),
                (text("workspace.manage.export"), String::new()),
                (
                    format!("! {}", text("workspace.manage.purge_trash")),
                    String::new(),
                ),
                (
                    format!("! {}", text("workspace.manage.delete")),
                    String::new(),
                ),
            ],
            other => other
                .content_kind()
                .map(|kind| {
                    workspace
                        .content
                        .entries(kind)
                        .iter()
                        .map(|entry| {
                            let badge = entry
                                .badge
                                .clone()
                                .or_else(|| entry.meta.version())
                                .unwrap_or_default();
                            (entry.name.clone(), badge)
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Empty-state text for a content category: what is missing, and the one
    /// useful action (downloads land in this instance because `Enter` points the
    /// download centre at it before leaving).
    pub(in crate::tui::app) fn workspace_empty_text(
        &self,
        workspace: &Workspace,
        kind: crate::core::content::ContentKind,
    ) -> (String, String) {
        let message = if kind == crate::core::content::ContentKind::Shader
            && workspace.shader_loader.is_none()
        {
            self.i18n.text("workspace.shaders.need_loader")
        } else {
            self.i18n.text(kind.empty_key())
        };
        // A world download is saved wherever the player points the file
        // dialog, not into the instance, so the target promise would be wrong
        // there.
        let hint = if kind == crate::core::content::ContentKind::World {
            self.i18n.text("workspace.empty.open_downloads")
        } else {
            format!(
                "{} · {}",
                self.i18n.text("workspace.desc.target"),
                self.i18n.text("workspace.empty.open_downloads")
            )
        };
        (message, hint)
    }

    fn workspace_overview_rows(&self, workspace: &Workspace) -> Vec<(String, String)> {
        let text = |key: &str| self.i18n.text(key);
        let loader = match &workspace.draft.loader {
            InstanceLoader::Vanilla => "Vanilla".to_owned(),
            InstanceLoader::Fabric { version } => format!("Fabric {version}"),
            InstanceLoader::Quilt { version } => format!("Quilt {version}"),
            InstanceLoader::Forge { version } => format!("Forge {version}"),
            InstanceLoader::NeoForge { version } => format!("NeoForge {version}"),
        };
        let seconds = workspace.draft.stats.play_time_seconds;
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        let play_time = if hours > 0 {
            self.i18n.format(
                "workspace.play_time",
                &[
                    ("hours", &hours.to_string()),
                    ("minutes", &minutes.to_string()),
                ],
            )
        } else {
            self.i18n.format(
                "workspace.play_time_short",
                &[("minutes", &minutes.to_string())],
            )
        };
        let overrides = if workspace.overridden_count() == 0 {
            text("workspace.overview.following")
        } else {
            self.i18n.format(
                "workspace.overview.customized",
                &[("count", &workspace.overridden_count().to_string())],
            )
        };
        let contents = self.i18n.format(
            "workspace.overview.contents_value",
            &[
                (
                    "mods",
                    &workspace
                        .content
                        .count(crate::core::content::ContentKind::Mod)
                        .to_string(),
                ),
                (
                    "packs",
                    &workspace
                        .content
                        .count(crate::core::content::ContentKind::ResourcePack)
                        .to_string(),
                ),
                (
                    "shaders",
                    &workspace
                        .content
                        .count(crate::core::content::ContentKind::Shader)
                        .to_string(),
                ),
                (
                    "worlds",
                    &workspace
                        .content
                        .count(crate::core::content::ContentKind::World)
                        .to_string(),
                ),
            ],
        );
        // The walk runs in the background, so the row says so instead of
        // claiming zero bytes while the answer is still on its way.
        let size = workspace
            .size_bytes
            .map(format_size)
            .unwrap_or_else(|| text("workspace.overview.measuring"));
        vec![
            (text("workspace.overview.name"), workspace.name.clone()),
            (
                text("workspace.overview.version"),
                workspace.draft.minecraft_version.clone(),
            ),
            (text("workspace.overview.loader"), loader),
            (
                text("workspace.overview.path"),
                crate::core::instance::path::display_path(&workspace.root),
            ),
            (
                text("workspace.overview.launch_count"),
                workspace.draft.stats.launch_count.to_string(),
            ),
            (text("workspace.overview.play_time"), play_time),
            (text("workspace.overview.size"), size),
            (text("workspace.overview.contents"), contents),
            (text("workspace.overview.overrides"), overrides),
        ]
    }

    /// Nine adjustable rows of the launch category. Single source of truth for
    /// rendering and for the keyboard actions, so a value cannot be shown one
    /// way and edited another.
    pub(in crate::tui::app) fn launch_rows(&self) -> Vec<LaunchRow> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Vec::new();
        };
        let overrides = &workspace.draft.overrides;
        let effective = EffectiveLaunch::resolve(&self.config, overrides);
        let global = &self.config.launch;
        let text = |key: &str| self.i18n.text(key);
        let automatic = effective.memory_mode == crate::core::config::models::MemoryMode::Automatic;
        let fullscreen = effective.fullscreen;
        let total_mb = crate::core::launch::memory::detect().total_mb;
        let mode_label = |mode: crate::core::config::models::MemoryMode| match mode {
            crate::core::config::models::MemoryMode::Automatic => text("workspace.auto"),
            crate::core::config::models::MemoryMode::Manual => text("workspace.manual"),
        };

        let memory_mode_value = match overrides.memory_mode {
            Some(value) => format!("{}（{}）", text("workspace.custom"), mode_label(value)),
            None => format!(
                "{}（{}）",
                text("workspace.follow"),
                mode_label(global.memory_mode)
            ),
        };
        let window_mode_value = match overrides.fullscreen {
            Some(value) => format!(
                "{}（{}）",
                text("workspace.custom"),
                if value {
                    text("workspace.fullscreen")
                } else {
                    text("workspace.windowed")
                }
            ),
            None => format!(
                "{}（{}）",
                text("workspace.follow"),
                if effective.fullscreen {
                    text("workspace.fullscreen")
                } else {
                    text("workspace.windowed")
                }
            ),
        };
        let java_value = match &overrides.java_path {
            Some(path) => format!(
                "{} {}",
                text("workspace.custom"),
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
            ),
            None => match &effective.java {
                JavaChoice::Auto => text("workspace.auto"),
                JavaChoice::Fixed(path) => format!(
                    "{} {}",
                    text("workspace.follow"),
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                ),
            },
        };
        let args_value = |value: &Option<Vec<String>>, resolved: &[String]| match value {
            Some(items) => self.i18n.format(
                "workspace.custom_items",
                &[("count", &items.len().to_string())],
            ),
            None => self.i18n.format(
                "workspace.follow_items",
                &[("count", &resolved.len().to_string())],
            ),
        };
        let server_value = match &overrides.server_address {
            Some(value) if value.trim().is_empty() => text("workspace.not_auto_connect"),
            Some(value) => format!("{} {value}", text("workspace.custom")),
            None => match &effective.server_address {
                Some(value) => format!("{} {value}", text("workspace.follow")),
                None => format!(
                    "{}（{}）",
                    text("workspace.follow"),
                    text("workspace.not_auto_connect")
                ),
            },
        };

        let memory_reason = if automatic {
            text("settings.reason.memory_auto")
        } else {
            String::new()
        };
        let size_reason = if fullscreen {
            text("settings.reason.fullscreen")
        } else {
            String::new()
        };

        vec![
            LaunchRow {
                field: LaunchField::MemoryMode,
                label: text("workspace.launch.memory_mode"),
                value: memory_mode_value,
                kind: LaunchFieldKind::Toggle,
                following: overrides.memory_mode.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
            LaunchRow {
                field: LaunchField::MemoryMin,
                label: text("workspace.launch.memory_min"),
                value: match overrides.min_memory_mb {
                    Some(value) => format!("{} {value} MB", text("workspace.custom")),
                    None => format!(
                        "{} {} MB",
                        text("workspace.follow"),
                        effective.min_memory_mb
                    ),
                },
                kind: LaunchFieldKind::Number {
                    step: 256,
                    min: 256,
                    max: total_mb,
                },
                following: overrides.min_memory_mb.is_none(),
                enabled: !automatic,
                disabled_reason: memory_reason.clone(),
            },
            LaunchRow {
                field: LaunchField::MemoryMax,
                label: text("workspace.launch.memory_max"),
                value: match overrides.max_memory_mb {
                    Some(value) => format!("{} {value} MB", text("workspace.custom")),
                    None => format!(
                        "{} {} MB",
                        text("workspace.follow"),
                        effective.max_memory_mb
                    ),
                },
                kind: LaunchFieldKind::Number {
                    step: 256,
                    min: 256,
                    max: total_mb,
                },
                following: overrides.max_memory_mb.is_none(),
                enabled: !automatic,
                disabled_reason: memory_reason,
            },
            LaunchRow {
                field: LaunchField::WindowMode,
                label: text("workspace.launch.window_mode"),
                value: window_mode_value,
                kind: LaunchFieldKind::Toggle,
                following: overrides.fullscreen.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
            LaunchRow {
                field: LaunchField::WindowSize,
                label: text("workspace.launch.window_size"),
                value: match (overrides.width, overrides.height) {
                    (None, None) => format!(
                        "{} {}×{}",
                        text("workspace.follow"),
                        effective.width,
                        effective.height
                    ),
                    _ => format!(
                        "{} {}×{}",
                        text("workspace.custom"),
                        effective.width,
                        effective.height
                    ),
                },
                kind: LaunchFieldKind::Number {
                    step: 16,
                    min: 320,
                    max: 7680,
                },
                following: overrides.width.is_none() && overrides.height.is_none(),
                enabled: !fullscreen,
                disabled_reason: size_reason,
            },
            LaunchRow {
                field: LaunchField::Java,
                label: text("workspace.launch.java"),
                value: java_value,
                kind: LaunchFieldKind::Picker,
                following: overrides.java_path.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
            LaunchRow {
                field: LaunchField::JvmArgs,
                label: text("workspace.launch.jvm_args"),
                value: args_value(&overrides.jvm_args, &effective.jvm_args),
                kind: LaunchFieldKind::Text,
                following: overrides.jvm_args.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
            LaunchRow {
                field: LaunchField::GameArgs,
                label: text("workspace.launch.game_args"),
                value: args_value(&overrides.game_args, &effective.game_args),
                kind: LaunchFieldKind::Text,
                following: overrides.game_args.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
            LaunchRow {
                field: LaunchField::Server,
                label: text("workspace.launch.server"),
                value: server_value,
                kind: LaunchFieldKind::Text,
                following: overrides.server_address.is_none(),
                enabled: true,
                disabled_reason: String::new(),
            },
        ]
    }

    /// The six install rows, straight from the client json on disk.
    ///
    /// Reading the file (not `draft.loader`) is what makes the screen honest
    /// after an install: the task rewrites the json, and the cached loader in
    /// `tmcl.json` is only a copy of what it said a moment ago.
    pub(in crate::tui::app) fn workspace_install_rows(
        &self,
        workspace: &Workspace,
    ) -> Vec<(String, String)> {
        use crate::core::instance::config::{LoaderState, loader_status};
        let text = |key: &str| self.i18n.text(key);
        let installed = text("workspace.installed");
        let missing = text("workspace.not_installed");
        loader_status(
            &workspace.root,
            &workspace.draft.name,
            &workspace.draft.minecraft_version,
        )
        .into_iter()
        .map(|(loader, state)| {
            let value = match state {
                LoaderState::Installed(version) if !version.is_empty() => {
                    format!("{version} · {installed}")
                }
                LoaderState::Installed(_) => installed.clone(),
                LoaderState::NotInstalled => missing.clone(),
                LoaderState::Incompatible { current } => self
                    .i18n
                    .format("workspace.install.incompatible", &[("current", current)]),
                // OptiFine is downloadable, just not by us: its license
                // forbids redistribution. "Unsupported" would read as "this
                // launcher can never run it".
                LoaderState::Unsupported
                    if loader == crate::core::instance::config::InstallableLoader::OptiFine =>
                {
                    text("workspace.install.optifine_manual")
                }
                LoaderState::Unsupported => text("workspace.install.unsupported"),
            };
            (text(loader.row_key()), value)
        })
        .collect()
    }

    pub(in crate::tui::app) fn draw_workspace_page(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        t: f64,
    ) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if area.width < 30 || area.height < 4 {
            return;
        }
        match self.preset() {
            UiPreset::Creeper => self.draw_creeper_workspace(frame, area, workspace, t),
            UiPreset::Classic => self.draw_classic_workspace(frame, area, workspace),
            // `nightlight` draws this page from its own shell, where the
            // derived palette is already in hand; re-deriving it here would be
            // work for nothing. It still reads the row data from this module.
            UiPreset::Nightlight => {}
            UiPreset::Paper => self.draw_paper_workspace(frame, area),
            UiPreset::Shelf => self.draw_shelf_workspace(frame, area, workspace),
        }
    }

    /// `creeper`: a category rail beside a card, the same shape as its settings
    /// page. The accent is the Instances page accent, not the settings blue, so
    /// the workspace belongs to the screen it was opened from.
    fn draw_creeper_workspace(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        t: f64,
    ) {
        let ws = WsSkin::Creeper.frame(self);
        // The workspace folds its rail when the body would be too narrow to
        // read; the exact threshold is the workspace's own, not the shell's.
        let narrow = area.width < RAIL_WIDTH + 60;
        let (rail, body) = if narrow {
            (
                Rect::new(area.x, area.y, area.width, 1),
                Rect::new(
                    area.x,
                    area.y.saturating_add(1),
                    area.width,
                    area.height.saturating_sub(1),
                ),
            )
        } else {
            let width = RAIL_WIDTH.min(area.width / 3);
            (
                Rect::new(area.x, area.y, width, area.height),
                Rect::new(
                    area.x.saturating_add(width),
                    area.y,
                    area.width.saturating_sub(width),
                    area.height,
                ),
            )
        };
        self.draw_workspace_rail(frame, rail, workspace, &ws, narrow, t);
        self.draw_workspace_body(frame, body, workspace, &ws, t);
    }

    /// `classic`: no sidebar. The categories are a tab row under the page tabs,
    /// exactly like the instance filters and the settings categories, and the
    /// body is a plain panel rather than a rounded card.
    /// `paper`: a chapter directory beside the shared body.
    ///
    /// The body is the same one classic and creeper use; only the frame changes
    /// — no cards, no tabs, a margin bar for the selection and a printed
    /// counter on the bottom edge of the page block.
    pub(in crate::tui::app) fn draw_paper_workspace(&self, frame: &mut Frame<'_>, area: Rect) {
        let Some(workspace) = self.workspace.as_ref() else {
            return;
        };
        if area.width < 24 || area.height < 4 {
            return;
        }
        let ws = WsSkin::Paper.frame(self);
        let pal = self.paper_palette();
        let body = if area.width < 76 {
            self.draw_paper_workspace_strip(frame, area, workspace, &pal);
            Rect::new(
                area.x,
                area.y.saturating_add(2),
                area.width,
                area.height.saturating_sub(2),
            )
        } else {
            // Two of these cells belong to the margin bar; the narrow rail
            // keeps 14 for the label, which is what the longest category needs.
            let rail_width = if area.width >= 100 { 18 } else { 16 };
            let rail = Rect::new(area.x, area.y, rail_width, area.height);
            self.draw_paper_workspace_rail(frame, rail, workspace, &pal, &ws);
            let rule_x = rail.right().saturating_add(1);
            for y in rail.y..rail.bottom() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "│",
                        Style::default().fg(pal.rule).bg(pal.bg),
                    ))),
                    Rect::new(rule_x, y, 1, 1),
                );
            }
            let body_x = rule_x.saturating_add(2);
            Rect::new(
                body_x,
                area.y,
                area.right().saturating_sub(body_x),
                area.height,
            )
        };
        self.draw_workspace_body(frame, body, workspace, &ws, self.anim_time());
    }

    /// `shelf`: a category line above the shared body.
    ///
    /// No rail — the categories are a strip, the same one the settings page
    /// uses, so both second-level screens read alike. The overview keeps the
    /// instance's cover at the top: it is the one category where the instance
    /// itself is the subject.
    pub(in crate::tui::app) fn draw_shelf_workspace(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
    ) {
        if area.width < 24 || area.height < 4 {
            return;
        }
        let pal = self.shelf_palette();
        let labels = WorkspaceCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.title_key()))
            .collect::<Vec<_>>();
        let current = workspace.category.index();
        let strip = Rect::new(area.x, area.y, area.width, 1);
        for (index, rect) in crate::tui::app::view::shelf::widgets::fit_labels(
            strip,
            &labels,
            current,
            strip.right(),
        ) {
            let style = if index == current {
                Style::default()
                    .fg(pal.accent_light)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let Some(label) = labels.get(index) else {
                continue;
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    palette::truncate(label, usize::from(rect.width)),
                    style,
                ))),
                rect,
            );
            self.hitmap.add(rect, Action::workspace_category(index));
        }
        crate::tui::app::view::shelf::widgets::rule(
            frame,
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            pal,
        );

        let mut body = Rect::new(
            area.x,
            area.y.saturating_add(2),
            area.width,
            area.height.saturating_sub(2),
        );
        if workspace.category == WorkspaceCategory::Overview
            && let Some(below) = self.shelf_workspace_cover(frame, body, workspace)
        {
            body = below;
        }
        let ws = WsSkin::Shelf.frame(self);
        self.draw_workspace_body(frame, body, workspace, &ws, self.anim_time());
    }

    /// The 20×10 cover above the overview rows, when there is room for it.
    ///
    /// Returns the rectangle left for the card. The cover takes eleven rows
    /// (ten plus a blank), so it only appears when the page can still show the
    /// whole facts list under it — at 80×24 it would leave six rows and cut the
    /// card in half.
    fn shelf_workspace_cover(
        &self,
        frame: &mut Frame<'_>,
        body: Rect,
        workspace: &Workspace,
    ) -> Option<Rect> {
        const SIDE: u16 = 20;
        const ROWS: u16 = SIDE / 2;
        if body.width < 40 || body.height < 24 {
            return None;
        }
        let pal = self.shelf_palette();
        let cover = self
            .covers
            .borrow_mut()
            .ensure(&workspace.root)
            .unwrap_or_else(|| {
                let seed = workspace.instance.as_u128() as u64
                    ^ (workspace.instance.as_u128() >> 64) as u64;
                crate::tui::app::view::shelf::widgets::placeholder(seed, u32::from(SIDE), pal)
            });
        let rect = Rect::new(body.x, body.y, SIDE, ROWS);
        crate::tui::widgets::cover::render(frame.buffer_mut(), rect, &cover, pal.bg);
        Some(Rect::new(
            body.x,
            rect.bottom().saturating_add(1),
            body.width,
            body.height.saturating_sub(ROWS + 1),
        ))
    }

    /// `shelf`'s inline `search …` line: one row, the same one the downloads and
    /// logs pages print.
    fn draw_shelf_workspace_search(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        t: f64,
    ) {
        let pal = self.shelf_palette();
        let target = SearchTarget::Workspace;
        let field = Rect::new(area.x, area.y, area.width, 1);
        self.hitmap.add(field, Action::search(target));
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let label = self.i18n.text("common.search");
        let budget = area.width.saturating_sub(palette::width(&label) as u16 + 2);
        let query = &workspace.search;
        let style = if focused {
            Style::default()
                .fg(pal.text)
                .bg(pal.bg)
                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else if query.is_empty() {
            Style::default().fg(pal.faint).bg(pal.bg)
        } else {
            Style::default().fg(pal.text).bg(pal.bg)
        };
        let shown = if query.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(query, usize::from(budget.saturating_sub(1)))
        };
        let mut spans = vec![
            Span::styled(format!("{label} "), Style::default().fg(pal.dim).bg(pal.bg)),
            Span::styled(shown, style),
        ];
        if focused && crate::tui::text::blink_on(t) {
            spans.push(Span::styled(
                "▏",
                Style::default().fg(pal.accent).bg(pal.bg),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), field);
    }

    fn draw_paper_workspace_rail(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &PaperPalette,
        ws: &WsFrame,
    ) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                self.i18n.text("workspace.title"),
                Style::default()
                    .fg(pal.dim)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect::new(area.x, area.y, area.width, 1),
        );
        for (index, category) in WorkspaceCategory::ALL.iter().enumerate() {
            let y = area.y.saturating_add(1).saturating_add(index as u16);
            if y >= area.bottom() {
                break;
            }
            let active = *category == workspace.category;
            let style = if active {
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let rect = Rect::new(area.x, y, area.width, 1);
            // The bar owns the first two cells; the label starts after them on
            // every row, so turning the page does not shift the rail sideways.
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.i18n.text(category.title_key()),
                    style,
                ))),
                Rect::new(area.x.saturating_add(2), y, area.width.saturating_sub(2), 1),
            );
            ws.colors.selection(frame, rect, active);
            self.hitmap.add(rect, Action::workspace_category(index));
        }
    }

    fn draw_paper_workspace_strip(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        pal: &PaperPalette,
    ) {
        let mut x = area.x;
        for (index, category) in WorkspaceCategory::ALL.iter().enumerate() {
            let label = self.i18n.text(category.title_key());
            let width = palette::width(&label) as u16;
            if x.saturating_add(width) > area.right() {
                break;
            }
            let active = *category == workspace.category;
            let style = if active {
                Style::default()
                    .fg(pal.accent)
                    .bg(pal.bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(pal.dim).bg(pal.bg)
            };
            let rect = Rect::new(x, area.y, width, 1);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(label, style))), rect);
            self.hitmap.add(rect, Action::workspace_category(index));
            x = x.saturating_add(width);
            if index + 1 < WorkspaceCategory::ALL.len() && x < area.right() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "·",
                        Style::default().fg(pal.faint).bg(pal.bg),
                    ))),
                    Rect::new(x, area.y, 1, 1),
                );
                x = x.saturating_add(1);
            }
        }
    }

    fn draw_classic_workspace(&self, frame: &mut Frame<'_>, area: Rect, workspace: &Workspace) {
        let ws = WsSkin::Classic.frame(self);
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        self.draw_classic_workspace_tabs(frame, sections[0], workspace, &ws.colors);
        self.draw_workspace_body(frame, sections[1], workspace, &ws, 0.0);
    }

    fn draw_classic_workspace_tabs(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        colors: &WsColors,
    ) {
        let labels = WorkspaceCategory::ALL
            .iter()
            .map(|category| self.i18n.text(category.title_key()))
            .collect::<Vec<_>>();
        let tabs = crate::tui::widgets::tabs::TabBar::new(labels);
        let rects = tabs.rects(area);
        // Eight categories do not fit an 80-column terminal in English; fall
        // back to the same folded rail the creeper preset uses.
        if rects.last().is_none_or(|rect| rect.right() > area.right()) {
            self.draw_folded_workspace_category(frame, area, workspace, colors);
            return;
        }
        frame.render_widget(
            tabs.widget()
                .select(workspace.category.index())
                .highlight_style(self.theme.accent_style())
                .style(Style::default().fg(self.theme.label)),
            area,
        );
        for (index, rect) in rects.into_iter().enumerate() {
            self.hitmap.add(rect, Action::workspace_category(index));
        }
    }

    /// One line `‹ Category ›  n/m` with both arrows clickable. Used by the
    /// creeper rail when it folds and by classic tabs when they would overflow.
    fn draw_folded_workspace_category(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        colors: &WsColors,
    ) {
        let label = format!(
            " ‹ {} ›  {}/{}",
            self.i18n.text(workspace.category.title_key()),
            workspace.category.index() + 1,
            WorkspaceCategory::ALL.len()
        );
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(colors.accent).bg(colors.bg)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        let edge = (area.width / 2).clamp(1, 3);
        self.hitmap.add(
            Rect::new(area.x, area.y, edge, 1),
            Action::workspace_category_by(-1),
        );
        self.hitmap.add(
            Rect::new(area.x + area.width.saturating_sub(edge), area.y, edge, 1),
            Action::workspace_category_by(1),
        );
    }

    fn draw_workspace_rail(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        ws: &WsFrame,
        narrow: bool,
        t: f64,
    ) {
        let colors = &ws.colors;
        if narrow {
            self.draw_folded_workspace_category(frame, area, workspace, colors);
            return;
        }
        let block = ws.card(self, &self.i18n.text("workspace.title"), false, t);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        for (index, category) in WorkspaceCategory::ALL.iter().enumerate() {
            let y = inner.y.saturating_add(index as u16);
            if y >= inner.bottom() {
                break;
            }
            let active = *category == workspace.category;
            let bg = colors.row_bg(active);
            let count = category
                .content_kind()
                .map(|kind| workspace.content.count(kind))
                .filter(|count| *count > 0);
            let count = count.map(|count| count.to_string()).unwrap_or_default();
            let count_width = palette::width(&count);
            let label_width = usize::from(inner.width)
                .saturating_sub(count_width + 4)
                .max(4);
            let label = palette::truncate(&self.i18n.text(category.title_key()), label_width);
            let pad = " ".repeat(label_width.saturating_sub(palette::width(&label)));
            let row = Rect::new(inner.x, y, inner.width, 1);
            self.hitmap.add(row, Action::workspace_category(index));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        if active { "▌ " } else { "  " },
                        Style::default()
                            .fg(colors.accent)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{label}{pad} "),
                        Style::default()
                            .fg(colors.row_fg(active, colors.dim))
                            .bg(bg)
                            .add_modifier(colors.row_modifier(active)),
                    ),
                    Span::styled(count, Style::default().fg(colors.faint).bg(bg)),
                ]))
                .style(Style::default().bg(bg)),
                row,
            );
        }
    }

    /// `⌕ <query>` in a card of its own, mirroring the download page's search
    /// box: the field is always visible, `/` only puts the cursor in it.
    fn draw_workspace_search(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        ws: &WsFrame,
        t: f64,
    ) {
        let colors = &ws.colors;
        if area.is_empty() {
            return;
        }
        if ws.skin == WsSkin::Paper {
            self.draw_paper_workspace_search(frame, area, workspace, t);
            return;
        }
        if ws.skin == WsSkin::Shelf {
            self.draw_shelf_workspace_search(frame, area, workspace, t);
            return;
        }
        let target = SearchTarget::Workspace;
        self.hitmap.add(area, Action::search(target));
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let block = ws.card(self, &self.i18n.text("action.search"), focused, t);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let cursor = self
            .search
            .as_ref()
            .filter(|state| state.target == target)
            .map_or_else(|| workspace.search.chars().count(), |state| state.cursor);
        let style = crate::tui::text::FieldStyle {
            text: colors.accent,
            dim: colors.dim,
            bg: colors.bg,
            cursor_fg: colors.bg,
            cursor_bg: colors.accent,
        };
        let mut spans = vec![Span::styled(
            " ⌕  ",
            Style::default()
                .fg(if focused { colors.accent } else { colors.dim })
                .add_modifier(ratatui::style::Modifier::BOLD),
        )];
        spans.extend(
            crate::tui::text::render(
                &workspace.search,
                cursor,
                inner.width.saturating_sub(5),
                focused,
                crate::tui::text::blink_on(t),
                style,
                &self.i18n.text("workspace.search_placeholder"),
            )
            .spans,
        );
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(colors.bg)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    /// `paper`: the same inline `search …` line the downloads and logs pages
    /// print, instead of a card with a magnifier. One page should not have two
    /// kinds of text field.
    fn draw_paper_workspace_search(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        t: f64,
    ) {
        let pal = self.paper_palette();
        let target = SearchTarget::Workspace;
        // The hit box is the line itself, not a band: hovering it must tint one
        // row like every other search field, not a three-row slab.
        let field = Rect::new(area.x, area.y, area.width, 1);
        self.hitmap.add(field, Action::search(target));
        let focused = matches!(self.search.as_ref(), Some(state) if state.target == target);
        let label = self.i18n.text("common.search");
        let budget = area.width.saturating_sub(palette::width(&label) as u16 + 2);
        let query = &workspace.search;
        let style = if focused {
            ink(&pal, pal.ink)
                .add_modifier(ratatui::style::Modifier::UNDERLINED | ratatui::style::Modifier::BOLD)
        } else if query.is_empty() {
            ink(&pal, pal.faint)
        } else {
            ink(&pal, pal.soft)
        };
        let shown = if query.is_empty() {
            "…".to_owned()
        } else {
            palette::truncate(query, usize::from(budget.saturating_sub(1)))
        };
        let mut spans = vec![Span::styled(format!("{label} "), ink(&pal, pal.dim))];
        spans.push(Span::styled(shown, style));
        if focused && crate::tui::text::blink_on(t) {
            spans.push(Span::styled("▏", ink(&pal, pal.accent)));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), field);
    }

    fn draw_workspace_body(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        ws: &WsFrame,
        t: f64,
    ) {
        let colors = &ws.colors;
        // The filter sits in its own card above the list, the way the download
        // page puts its search box above the results. Only content categories
        // have something to filter.
        let area = match workspace.category.content_kind() {
            Some(_) if area.height >= 12 => {
                // The framed skins put the field inside a three-row card; the
                // paper field is one line, so it reserves one row.
                let field_rows = if matches!(ws.skin, WsSkin::Paper | WsSkin::Shelf) {
                    1
                } else {
                    3
                };
                let sections = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(field_rows), Constraint::Min(0)])
                    .split(area);
                self.draw_workspace_search(frame, sections[0], workspace, ws, t);
                sections[1]
            }
            _ => area,
        };
        let title = self.i18n.text(workspace.category.title_key());
        // An empty content category gets the same treatment as an empty
        // settings category: a panel that says so, not a silent blank body.
        if let Some(kind) = workspace.category.content_kind()
            && workspace.content.count(kind) == 0
            && !workspace.loading
            && workspace.search.trim().is_empty()
        {
            // Shaders without a loader get the reason, not just "empty": the
            // folder may be full and still do nothing.
            let (message, hint) = self.workspace_empty_text(workspace, kind);
            ws.empty(
                self,
                frame,
                area,
                WsEmpty {
                    title: &title,
                    message: &message,
                    hint: &hint,
                },
                t,
            );
            self.hitmap
                .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
            return;
        }
        let count = workspace.row_count();
        let mut block = ws.card(self, &title, true, t);
        if count > 0 {
            let position = workspace.selected.min(count - 1) + 1;
            block = block.title_bottom(ws.counter(self, position, count).right_aligned());
        }
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        self.hitmap
            .scroll(area, crate::tui::action::ScrollTarget::WorkspaceRows);
        // The note strip lives inside the panel: line 1 explains the selection,
        // line 2 lists the keys that work on it. Both lines are reserved
        // whenever there is an explanation to show — the old "two rows only
        // above 100 columns" rule meant every terminal with the rail open lost
        // the key line as soon as the selection had a description.
        let (message, is_warning) = self.workspace_note(workspace);
        let note_rows = if inner.width < 24 {
            0
        } else if inner.height >= 8 && !message.is_empty() {
            2
        } else if inner.height >= 5 {
            1
        } else {
            0
        };
        let body = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(note_rows),
        );
        let note = Rect::new(
            inner.x,
            inner.y.saturating_add(body.height),
            inner.width,
            note_rows,
        );
        if workspace.category == WorkspaceCategory::Launch {
            self.draw_launch_body(frame, body, workspace, colors);
        } else if let Some(kind) = workspace.category.content_kind() {
            self.draw_content_body(frame, body, workspace, kind, colors);
        } else {
            self.draw_plain_rows(frame, body, workspace, colors);
        }
        if note_rows > 0 {
            self.draw_workspace_notes(frame, note, workspace, colors, &message, is_warning);
        }
    }

    /// Overview / install / manage: label-value rows with no per-row control.
    fn draw_plain_rows(
        &self,
        frame: &mut Frame<'_>,
        inner: Rect,
        workspace: &Workspace,
        colors: &WsColors,
    ) {
        let rows = self.workspace_display_rows();
        if rows.is_empty() {
            return;
        }
        // The manage screen separates its destructive rows from the ordinary
        // ones; the rule is decoration, not a selectable row.
        let divider_after = (workspace.category == WorkspaceCategory::Manage).then_some(2);
        let extra_line = usize::from(divider_after.is_some());
        let visible = usize::from(inner.height.max(1))
            .saturating_sub(extra_line)
            .max(1);
        let selected = workspace.selected.min(rows.len().saturating_sub(1));
        let offset =
            self.row_windows
                .resolve_offset(RowWindow::Workspace, rows.len(), selected, visible);
        let mut y = inner.y;
        for (index, (label, value)) in rows.iter().enumerate().skip(offset) {
            if y >= inner.bottom() {
                break;
            }
            if divider_after.is_some_and(|after| index == after + 1) {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "─".repeat(usize::from(inner.width)),
                        Style::default().fg(colors.faint).bg(colors.bg),
                    ))),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y = y.saturating_add(1);
                if y >= inner.bottom() {
                    break;
                }
            }
            let active = index == selected;
            let background = colors.row_bg(active);
            let row = Rect::new(inner.x, y, inner.width, 1);
            paint_row_bg(frame, row, background);
            let label_style = Style::default()
                .fg(colors.row_fg(active, colors.dim))
                .bg(background)
                .add_modifier(colors.row_modifier(active));
            let value_style = Style::default()
                .fg(colors.row_fg(active, colors.dim))
                .bg(background)
                .add_modifier(colors.row_modifier(active));
            let pad = LABEL_WIDTH.saturating_sub(palette::width(label)).max(1);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!(" {label}{}", " ".repeat(pad)), label_style),
                    Span::styled(
                        palette::truncate(
                            value,
                            usize::from(inner.width).saturating_sub(LABEL_WIDTH + 2),
                        ),
                        value_style,
                    ),
                ])),
                row,
            );
            // Drawn after the text: the first cell of a shared row is padding,
            // and the paragraph would otherwise paint over the bar.
            colors.selection(frame, row, active);
            self.hitmap.add(row, Action::workspace_row(index));
            y = y.saturating_add(1);
        }
    }

    /// The note strip under the list: what the selection means, then the keys.
    ///
    /// Narrow layouts only get one row, so a real note wins over the key line
    /// instead of being silently dropped.
    fn draw_workspace_notes(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        colors: &WsColors,
        message: &str,
        is_warning: bool,
    ) {
        let keys = self.workspace_action_line(workspace);
        let width = usize::from(area.width).saturating_sub(4);
        // One row: the description wins, because a warning that gets cut off is
        // worse than missing shortcuts (the footer still lists them).
        let single = area.height < 2;
        if !message.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" ◆ {}", palette::truncate(message, width)),
                    Style::default()
                        .fg(if is_warning { colors.warn } else { colors.dim })
                        .bg(colors.bg),
                ))),
                Rect::new(area.x, area.y, area.width, 1),
            );
        }
        if single {
            if message.is_empty() && !keys.is_empty() {
                // Same style as the second row elsewhere: `◆` marks an
                // explanation, the indented faint row is the key list.
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("   {}", palette::truncate(&keys, width)),
                        Style::default().fg(colors.faint).bg(colors.bg),
                    ))),
                    Rect::new(area.x, area.y, area.width, 1),
                );
            }
            return;
        }
        // The key line moves up when there is no description, so the strip never
        // wastes a blank row — and it stays visible while a description exists.
        let key_row = area.y.saturating_add(u16::from(!message.is_empty()));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("   {}", palette::truncate(&keys, width)),
                Style::default().fg(colors.faint).bg(colors.bg),
            ))),
            Rect::new(area.x, key_row, area.width, 1),
        );
    }

    /// Line 1 of the strip: the selected row's description or warning.
    pub(in crate::tui::app) fn workspace_note(&self, workspace: &Workspace) -> (String, bool) {
        if let Some(kind) = workspace.category.content_kind() {
            let entries = workspace.visible_entries(kind);
            let Some(entry) = entries.get(workspace.selected) else {
                return (String::new(), false);
            };
            return self.content_note(workspace, entry);
        }
        match workspace.category {
            WorkspaceCategory::Launch => {
                let rows = self.launch_rows();
                let Some(row) = rows.get(workspace.selected) else {
                    return (String::new(), false);
                };
                if row.field == LaunchField::Java
                    && let Some((installed, required)) = self.workspace_java_mismatch()
                {
                    return (
                        self.i18n.format(
                            "workspace.launch.java_too_old",
                            &[
                                ("installed", &installed.to_string()),
                                ("required", &required.to_string()),
                            ],
                        ),
                        true,
                    );
                }
                if !row.enabled {
                    return (row.disabled_reason.clone(), true);
                }
                (String::new(), false)
            }
            WorkspaceCategory::Manage => (self.i18n.text("workspace.manage.danger_hint"), false),
            _ => (String::new(), false),
        }
    }

    /// Line 2 of the strip: what the keys do on this screen, and only the keys
    /// that actually work in the current state.
    pub(in crate::tui::app) fn workspace_action_line(&self, workspace: &Workspace) -> String {
        let text = |key: &str| self.i18n.text(key);
        let hint = |key: &str, label: &str| format!("{key} {}", text(label));
        let mut parts = Vec::new();
        match workspace.category {
            WorkspaceCategory::Overview => {
                parts.push(hint("L", "action.launch"));
                parts.push(hint("e", "action.rename"));
                parts.push(hint("o", "action.open"));
            }
            WorkspaceCategory::Launch => {
                parts.push(hint("←/→", "action.adjust"));
                parts.push(hint("Space", "action.toggle"));
                parts.push(hint("Enter", "action.edit"));
                parts.push(hint("R", "action.reset"));
            }
            WorkspaceCategory::Install => {
                parts.push(hint("Enter", "action.install"));
                parts.push(hint("d", "action.uninstall"));
            }
            WorkspaceCategory::Manage => {
                parts.push(hint("Enter", "action.run"));
                parts.push(hint("d", "action.delete"));
            }
            category => {
                if let Some(kind) = category.content_kind() {
                    let locked = self.workspace_instance_running();
                    let no_loader = kind == crate::core::content::ContentKind::Shader
                        && workspace.shader_loader.is_none();
                    let cache_empty = workspace.content.count(kind) == 0;
                    // A filter that hides every row leaves nothing for the
                    // per-row keys to act on, so they are not advertised.
                    let visible_empty = workspace.visible_entries(kind).is_empty();
                    if cache_empty {
                        parts.push(hint("Enter", "action.download"));
                    } else if !visible_empty {
                        if kind == crate::core::content::ContentKind::World {
                            parts.push(hint("b", "action.backup"));
                            parts.push(hint("r", "action.restore"));
                            parts.push(hint("c", "action.clone"));
                        }
                        if kind != crate::core::content::ContentKind::World && !locked && !no_loader
                        {
                            parts.push(hint("Space", "action.toggle"));
                        }
                        parts.push(hint("Enter", "action.details"));
                        parts.push(hint("d", "action.delete"));
                    }
                    if kind == crate::core::content::ContentKind::Mod {
                        parts.push(hint("s", "action.sort"));
                    }
                    if kind == crate::core::content::ContentKind::ResourcePack && !locked {
                        parts.push(hint("J/K", "action.reorder"));
                    }
                    parts.push(hint("F5", "action.refresh"));
                    parts.push(hint("i", "action.import"));
                    parts.push(hint("o", "action.open"));
                    parts.push(hint("/", "action.search"));
                }
            }
        }
        parts.join(" · ")
    }

    /// Description or warning of one content entry.
    fn content_note(
        &self,
        workspace: &Workspace,
        entry: &crate::core::content::ContentEntry,
    ) -> (String, bool) {
        if let Some(name) = entry.warning.as_deref() {
            return (
                self.i18n
                    .format("workspace.warning.duplicate", &[("name", name)]),
                true,
            );
        }
        // A pack built for another game version still loads, so this is a note.
        if let Some(note) = self.pack_format_note(workspace, entry) {
            return (note, false);
        }
        // A descriptor's description can hold newlines and tabs; the strip is
        // one line, so collapse its whitespace instead of drawing it raw.
        (
            entry
                .meta
                .description()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            false,
        )
    }

    fn pack_format_note(
        &self,
        workspace: &Workspace,
        entry: &crate::core::content::ContentEntry,
    ) -> Option<String> {
        let crate::core::content::ContentMeta::ResourcePack {
            pack_format: Some(format),
            ..
        } = &entry.meta
        else {
            return None;
        };
        let expected =
            crate::core::content::expected_pack_format(&workspace.draft.minecraft_version)?;
        (*format != expected).then(|| {
            self.i18n.format(
                "workspace.packs.format_mismatch",
                &[
                    ("pack", &format.to_string()),
                    ("expected", &expected.to_string()),
                    ("version", &workspace.draft.minecraft_version),
                ],
            )
        })
    }

    /// Launch rows carry more state than a label/value pair: a disabled row
    /// shows its reason instead of a value, and the selected row advertises the
    /// keys that work on it.
    fn draw_launch_body(
        &self,
        frame: &mut Frame<'_>,
        inner: Rect,
        workspace: &Workspace,
        colors: &WsColors,
    ) {
        let rows = self.launch_rows();
        if rows.is_empty() {
            return;
        }
        let visible = usize::from(inner.height.max(1));
        let selected = workspace.selected.min(rows.len().saturating_sub(1));
        let offset =
            self.row_windows
                .resolve_offset(RowWindow::Workspace, rows.len(), selected, visible);
        for (index, row) in rows.iter().enumerate().skip(offset).take(visible) {
            let y = inner.y.saturating_add((index - offset) as u16);
            if y >= inner.bottom() {
                break;
            }
            let active = index == selected;
            let background = colors.row_bg(active);
            let line = Rect::new(inner.x, y, inner.width, 1);
            paint_row_bg(frame, line, background);
            // A pinned runtime older than the game needs fails at launch, so
            // its value is marked while the row stays editable.
            let java_warning =
                row.field == LaunchField::Java && self.workspace_java_mismatch().is_some();
            let label_style = Style::default()
                .fg(colors.row_fg(active, colors.dim))
                .bg(background)
                .add_modifier(colors.row_modifier(active));
            let value_style = Style::default()
                .fg(if !row.enabled || java_warning {
                    if active && colors.selection_bg.is_none() {
                        colors.selection_fg
                    } else {
                        colors.warn
                    }
                } else {
                    colors.row_fg(active, colors.dim)
                })
                .bg(background)
                .add_modifier(colors.row_modifier(active));
            let hint = if active && row.enabled {
                self.i18n.text(launch_kind_hint(row.kind))
            } else {
                String::new()
            };
            let hint_width = palette::width(&hint);
            let value_text = if row.enabled {
                row.value.as_str()
            } else {
                row.disabled_reason.as_str()
            };
            let value_width = usize::from(inner.width)
                .saturating_sub(LABEL_WIDTH + hint_width + 3)
                .max(4);
            let pad = LABEL_WIDTH
                .saturating_sub(palette::width(&row.label))
                .max(1);
            // Only rows whose value `←/→` can actually change get arrows. A
            // text or picker row opens an editor, so arrows there would
            // advertise a control that does not exist.
            let adjustable = row.enabled && matches!(row.kind, LaunchFieldKind::Number { .. });
            let mut spans = vec![Span::styled(
                format!(" {}{}", row.label, " ".repeat(pad)),
                label_style,
            )];
            if adjustable {
                spans.push(Span::styled(
                    "\u{2039} ",
                    Style::default().fg(colors.faint).bg(background),
                ));
                spans.push(Span::styled(
                    palette::truncate(value_text, value_width.saturating_sub(4)),
                    value_style,
                ));
                spans.push(Span::styled(
                    " \u{203a}",
                    Style::default().fg(colors.faint).bg(background),
                ));
            } else {
                spans.push(Span::styled(
                    palette::truncate(value_text, value_width),
                    value_style,
                ));
            }
            if !hint.is_empty() {
                spans.push(Span::styled(
                    format!(" {hint}"),
                    Style::default().fg(colors.faint).bg(background),
                ));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), line);
            colors.selection(frame, line, active);
            // The whole row is the selection target; the arrows are two-cell
            // sub-targets registered after it, exactly like the settings page.
            self.hitmap.add(line, Action::workspace_row(index));
            if adjustable {
                let value_x = inner.x.saturating_add(1 + LABEL_WIDTH as u16);
                let text_width = palette::width(&palette::truncate(
                    value_text,
                    value_width.saturating_sub(4),
                ));
                let control_width = u16::try_from(4 + text_width).unwrap_or(u16::MAX);
                self.hitmap.add(
                    Rect::new(value_x, y, 2, 1),
                    Action::workspace_step(index, -1),
                );
                self.hitmap.add(
                    Rect::new(
                        value_x.saturating_add(control_width.saturating_sub(2)),
                        y,
                        2,
                        1,
                    ),
                    Action::workspace_step(index, 1),
                );
            }
        }
    }

    /// A content list: enable mark, name, version, size, warning.
    fn draw_content_body(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        workspace: &Workspace,
        kind: crate::core::content::ContentKind,
        colors: &WsColors,
    ) {
        // A shader list with no loader is the one case where the list itself is
        // the problem, so the reason goes above it instead of into an empty
        // state that would hide the files the player already has.
        // Two situations make the list itself misleading, and both say so in
        // a banner instead of a silent refusal when a key is pressed.
        let banner = if kind == crate::core::content::ContentKind::Shader
            && workspace.shader_loader.is_none()
        {
            Some(self.i18n.text("workspace.shaders.no_loader_banner"))
        } else if kind == crate::core::content::ContentKind::ResourcePack
            && self.workspace_instance_running()
        {
            Some(self.i18n.text("workspace.packs.running_banner"))
        } else {
            None
        };
        let inner = match banner {
            Some(message) => {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!(" ! {message}"),
                        Style::default().fg(colors.warn).bg(colors.bg),
                    ))),
                    Rect::new(area.x, area.y, area.width, 1),
                );
                Rect::new(
                    area.x,
                    area.y.saturating_add(1),
                    area.width,
                    area.height.saturating_sub(1),
                )
            }
            None => area,
        };
        let entries = workspace.visible_entries(kind);
        if entries.is_empty() {
            if workspace.loading {
                frame.render_widget(
                    Paragraph::new(vec![Line::from(format!(
                        " {}",
                        self.i18n.text("workspace.scanning")
                    ))])
                    .style(Style::default().fg(colors.faint).bg(colors.bg)),
                    inner,
                );
                return;
            }
            if !workspace.search.trim().is_empty() {
                frame.render_widget(
                    Paragraph::new(vec![Line::from(format!(
                        " {}",
                        self.i18n.text("workspace.no_match")
                    ))])
                    .style(Style::default().fg(colors.faint).bg(colors.bg)),
                    inner,
                );
                return;
            }
            let (message, hint) = self.workspace_empty_text(workspace, kind);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(format!(" {message}")),
                    Line::from(Span::styled(
                        format!(" {hint}"),
                        Style::default().fg(colors.faint).bg(colors.bg),
                    )),
                ])
                .style(Style::default().fg(colors.faint).bg(colors.bg)),
                inner,
            );
            return;
        }
        let width = usize::from(inner.width);
        let size_width = 9;
        let version_width = 12;
        // Leave room for the warning column instead of letting the name eat the
        // whole row; on narrow terminals the name wins and the warning truncates.
        // Reserve the trailing column before sizing the name: a note that does
        // not fit is clipped by the panel edge, and a clipped date is useless.
        let note_reserve = entries
            .iter()
            .filter_map(|entry| self.trailing_note(entry))
            .map(|(text, _)| palette::width(&text) + 1)
            .max()
            .unwrap_or(0)
            .min(width / 3);
        let name_width = width
            .saturating_sub(5 + version_width + size_width + note_reserve)
            .clamp(16, 34);
        let visible = usize::from(inner.height.max(1));
        let selected = workspace.selected.min(entries.len().saturating_sub(1));
        let offset =
            self.row_windows
                .resolve_offset(RowWindow::Workspace, entries.len(), selected, visible);
        for (index, entry) in entries.iter().enumerate().skip(offset).take(visible) {
            let y = inner.y.saturating_add((index - offset) as u16);
            if y >= inner.bottom() {
                break;
            }
            let active = index == selected;
            let background = colors.row_bg(active);
            let base = colors.row_fg(active, colors.dim);
            let dim = colors.row_fg(active, colors.dim);
            let warn = if active && colors.selection_bg.is_none() {
                colors.selection_fg
            } else {
                colors.warn
            };
            let bold = colors.row_modifier(active);
            let mark = if entry.enabled { "[x]" } else { "[ ]" };
            let size = format_size(entry.size);
            let row = Rect::new(inner.x, y, inner.width, 1);
            paint_row_bg(frame, row, background);
            let spans = vec![
                Span::styled(
                    format!(" {mark} "),
                    Style::default()
                        .fg(if entry.enabled {
                            colors.row_fg(active, colors.ok)
                        } else {
                            dim
                        })
                        .bg(background)
                        .add_modifier(bold),
                ),
                Span::styled(
                    format!(
                        "{}{}",
                        palette::truncate(&entry.name, name_width),
                        " ".repeat(name_width.saturating_sub(palette::width(&entry.name)))
                    ),
                    Style::default().fg(base).bg(background).add_modifier(bold),
                ),
                Span::styled(
                    format!(
                        "{:<version_width$}",
                        palette::truncate(&entry.version, version_width)
                    ),
                    Style::default().fg(dim).bg(background).add_modifier(bold),
                ),
                Span::styled(
                    format!("{size:>size_width$}"),
                    Style::default().fg(dim).bg(background).add_modifier(bold),
                ),
            ];
            // The trailing column is one slot with two jobs: a real problem wins
            // (gold), otherwise a world puts its date and hardcore flag there.
            //
            // It is drawn in its own rectangle instead of as another span, so the
            // available width is whatever the fixed columns left over — an extra
            // span would be clipped by the panel edge and lose its last glyph.
            let fixed = 5 + name_width + version_width + size_width;
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(inner.x, y, (fixed as u16).min(inner.width), 1),
            );
            if let Some((text, is_warning)) = self.trailing_note(entry) {
                let note_width = usize::from(inner.width).saturating_sub(fixed + 1);
                if note_width > 0 {
                    frame.render_widget(
                        Paragraph::new(Span::styled(
                            format!(" {}", palette::truncate(&text, note_width)),
                            Style::default()
                                .fg(if is_warning { warn } else { dim })
                                .bg(background)
                                .add_modifier(bold),
                        )),
                        Rect::new(
                            inner.x.saturating_add(fixed as u16),
                            y,
                            inner.width.saturating_sub(fixed as u16),
                            1,
                        ),
                    );
                }
            }
            // After the spans and the trailing note, so the padding cell at the
            // left edge is the only thing the bar covers.
            colors.selection(frame, row, active);
            // The whole row selects (double click opens the detail popup); the
            // three `[x]` cells are a smaller target registered afterwards, so
            // a click on the mark toggles and a click anywhere else selects.
            self.hitmap.add(row, Action::workspace_row(index));
            self.hitmap.add(
                Rect::new(inner.x.saturating_add(1), y, 3, 1),
                Action::workspace_toggle(index),
            );
        }
    }

    /// Entries of the category on screen, after filter and sort.
    pub(in crate::tui::app) fn workspace_content_entries(
        &self,
    ) -> Vec<crate::core::content::ContentEntry> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Vec::new();
        };
        let Some(kind) = workspace.category.content_kind() else {
            return Vec::new();
        };
        workspace
            .visible_entries(kind)
            .into_iter()
            .cloned()
            .collect()
    }
}

/// Paints the full row before its text, so a selected band and a hover tint
/// cover the same width the row's hit box reports.
fn paint_row_bg(frame: &mut Frame<'_>, row: Rect, bg: ratatui::style::Color) {
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(bg)),
        row,
    );
}

fn launch_kind_hint(kind: LaunchFieldKind) -> &'static str {
    match kind {
        LaunchFieldKind::Toggle => "hint.setting_toggle",
        LaunchFieldKind::Number { .. } => "hint.setting_step",
        LaunchFieldKind::Text => "hint.setting_edit",
        LaunchFieldKind::Picker => "hint.setting_pick",
    }
}

/// Colours one workspace surface draws with.
///
/// The two presets differ only here: `creeper` paints a `SURFACE` card with the
/// fixed palette, `classic` follows the terminal theme. Row semantics — marks,
/// warnings, selection — stay shared, so neither preset can drift in content.
#[derive(Clone, Copy)]
struct WsColors {
    bg: ratatui::style::Color,
    dim: ratatui::style::Color,
    faint: ratatui::style::Color,
    accent: ratatui::style::Color,
    /// Fill behind the selected row. `None` means the preset marks a selection
    /// with a bright foreground instead of a band, which is what the `classic`
    /// tables have always done.
    selection_bg: Option<ratatui::style::Color>,
    selection_fg: ratatui::style::Color,
    /// Mark the selection with a margin bar instead of a fill.
    selection_bar: bool,
    /// Underline the selected row's label. `paper` wants it; `shelf` marks the
    /// selection with the surface band alone and would read as a second kind of
    /// emphasis if it underlined too.
    underline: bool,
    warn: ratatui::style::Color,
    ok: ratatui::style::Color,
}

impl WsColors {
    fn row_bg(&self, active: bool) -> ratatui::style::Color {
        if active {
            self.selection_bg.unwrap_or(self.bg)
        } else {
            self.bg
        }
    }

    fn row_fg(&self, active: bool, resting: ratatui::style::Color) -> ratatui::style::Color {
        if active { self.selection_fg } else { resting }
    }

    /// Bold carries the selection in every preset: `creeper` pairs it with a
    /// tinted row, `classic` with the terminal green selection colour, `paper`
    /// with an underline and a margin bar.
    fn row_modifier(&self, active: bool) -> ratatui::style::Modifier {
        if active {
            ratatui::style::Modifier::BOLD
                | if self.underline {
                    ratatui::style::Modifier::UNDERLINED
                } else {
                    ratatui::style::Modifier::empty()
                }
        } else {
            ratatui::style::Modifier::empty()
        }
    }

    /// The margin bar, for presets that mark the selection without a fill.
    /// The first cell of every shared row is padding, so this never covers text.
    fn selection(&self, frame: &mut Frame<'_>, row: Rect, active: bool) {
        if !active || !self.selection_bar || row.width == 0 || row.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "▌",
                Style::default().fg(self.accent).bg(self.bg),
            ))),
            Rect::new(row.x, row.y, 1, 1),
        );
    }
}

/// The frame vocabulary of the workspace page.
///
/// The body — rows, editors, content lists, notes — is the same for `classic`
/// and `creeper`; only four things differ: the palette, the card border, the
/// empty state and the counter style. Funnelling them through one value keeps
/// the body from asking "am I creeper?" in four places, and a fourth preset
/// reuses this page by answering these four questions instead of adding a
/// fifth branch.
///
/// `nightlight` does not use it: its workspace has a different structure
/// (numbered rail, banner line, right-aligned value column), so it draws its
/// own page and only reads the row data from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WsSkin {
    Classic,
    Creeper,
    Paper,
    Shelf,
}

impl WsSkin {
    fn frame(self, app: &App) -> WsFrame {
        WsFrame {
            skin: self,
            colors: self.colors(app),
        }
    }

    fn colors(self, app: &App) -> WsColors {
        use crate::tui::palette;
        match self {
            // The workspace is a second-level screen of Instances, so it takes
            // that page's accent (green), not the settings page's blue. The
            // fills come from creeper's own sheet — reading the module
            // constants here left a dark panel on a light page.
            Self::Creeper => {
                let pal = app.creeper_palette();
                WsColors {
                    bg: pal.surface,
                    dim: pal.text_dim,
                    faint: pal.text_faint,
                    accent: app.page_accent(),
                    selection_bg: Some(palette::mix(pal.surface, app.page_accent(), 0.2)),
                    selection_fg: pal.text,
                    selection_bar: false,
                    underline: false,
                    warn: pal.gold,
                    ok: pal.green,
                }
            }
            Self::Paper => {
                let pal = app.paper_palette();
                WsColors {
                    bg: pal.bg,
                    dim: pal.dim,
                    faint: pal.faint,
                    accent: pal.accent,
                    selection_bg: None,
                    selection_fg: pal.ink,
                    selection_bar: true,
                    underline: true,
                    warn: pal.warn,
                    ok: pal.ok,
                }
            }
            Self::Shelf => {
                let pal = app.shelf_palette();
                WsColors {
                    bg: pal.bg,
                    dim: pal.dim,
                    faint: pal.faint,
                    accent: pal.accent,
                    // The same band the instance list uses: the workspace is a
                    // list of rows, not a stack of cards.
                    selection_bg: Some(pal.surface),
                    selection_fg: pal.text,
                    selection_bar: true,
                    underline: false,
                    warn: pal.warn,
                    ok: pal.ok,
                }
            }
            Self::Classic => {
                let theme = app.theme;
                WsColors {
                    bg: theme.bg,
                    // Classic tables print every cell in the terminal
                    // foreground and let the green selection style carry the
                    // emphasis.
                    dim: theme.fg,
                    faint: theme.dim,
                    accent: theme.accent,
                    selection_bg: None,
                    selection_fg: theme.selection,
                    selection_bar: false,
                    underline: false,
                    warn: theme.label,
                    ok: theme.success,
                }
            }
        }
    }
}

/// A border set made of spaces: `Block` keeps its geometry and titles, but
/// nothing is drawn on the edge rows. Used by the paper skin.
fn blank_border_set() -> border::Set {
    border::Set {
        top_left: " ",
        top_right: " ",
        bottom_left: " ",
        bottom_right: " ",
        vertical_left: " ",
        vertical_right: " ",
        horizontal_top: " ",
        horizontal_bottom: " ",
    }
}

/// A skin bound to the colours it resolved for this frame.
struct WsFrame {
    skin: WsSkin,
    colors: WsColors,
}

/// What an empty category says: the reason and the way out.
struct WsEmpty<'a> {
    title: &'a str,
    message: &'a str,
    hint: &'a str,
}

impl WsFrame {
    /// Rounded and palette-driven under `creeper`, the straight single-line box
    /// of `classic` — the same split every other page already makes.
    fn card(&self, app: &App, title: &str, focused: bool, t: f64) -> Block<'static> {
        match self.skin {
            WsSkin::Creeper => crate::tui::widgets::kit::card(
                title,
                self.colors.accent,
                focused,
                t,
                self.colors.bg,
            ),
            WsSkin::Classic => crate::tui::chrome::panel(title, focused, app.theme),
            WsSkin::Shelf => {
                // The same rounded slab the instance page puts around the
                // cover: no fill, a faint rule, a dim title.
                let pal = app.shelf_palette();
                Block::bordered()
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(pal.faint).bg(pal.bg))
                    .title(Line::from(Span::styled(
                        format!(" {title} "),
                        Style::default()
                            .fg(pal.dim)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    )))
                    .style(Style::default().bg(pal.bg))
            }
            WsSkin::Paper => {
                let pal = app.paper_palette();
                // A `Block` with space-drawn top and bottom edges: it still
                // owns the title row and `title_bottom` counter, but prints no
                // line at all. That is how the paper skin keeps a page head and
                // a page number without becoming a card.
                Block::default()
                    .borders(Borders::TOP | Borders::BOTTOM)
                    .border_set(blank_border_set())
                    .title(Line::from(Span::styled(
                        title.to_owned(),
                        Style::default()
                            .fg(pal.dim)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    )))
                    .style(Style::default().bg(pal.bg))
            }
        }
    }

    /// An empty category: a card that says why under `creeper`, a plain panel
    /// with the reason and the hint on one line under `classic`.
    fn empty(&self, app: &App, frame: &mut Frame<'_>, area: Rect, content: WsEmpty<'_>, t: f64) {
        match self.skin {
            WsSkin::Creeper => crate::tui::widgets::kit::empty_card_with_hint(
                frame,
                area,
                content.title,
                content.message,
                content.hint,
                self.colors.accent,
                t,
            ),
            WsSkin::Classic => crate::tui::widgets::empty::render(
                frame,
                area,
                content.title,
                &format!("{} · {}", content.message, content.hint),
                app.theme,
            ),
            WsSkin::Shelf => {
                let pal = app.shelf_palette();
                let mut lines = vec![
                    Line::from(Span::styled(
                        content.title.to_owned(),
                        Style::default()
                            .fg(pal.text)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        palette::truncate(content.message, usize::from(area.width)),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    )),
                ];
                if !content.hint.is_empty() {
                    lines.push(Line::from(Span::styled(
                        palette::truncate(content.hint, usize::from(area.width)),
                        Style::default().fg(pal.dim).bg(pal.bg),
                    )));
                }
                frame.render_widget(Paragraph::new(lines), area);
            }
            WsSkin::Paper => {
                let pal = app.paper_palette();
                let mut lines = vec![
                    Line::from(Span::styled(
                        content.title.to_owned(),
                        Style::default()
                            .fg(pal.ink)
                            .bg(pal.bg)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        palette::truncate(content.message, usize::from(area.width)),
                        Style::default().fg(pal.faint).bg(pal.bg),
                    )),
                ];
                if !content.hint.is_empty() {
                    lines.push(Line::from(Span::styled(
                        palette::truncate(content.hint, usize::from(area.width)),
                        Style::default().fg(pal.dim).bg(pal.bg),
                    )));
                }
                frame.render_widget(
                    Paragraph::new(lines),
                    Rect::new(
                        area.x,
                        area.y.saturating_add(1),
                        area.width,
                        area.height.saturating_sub(1),
                    ),
                );
            }
        }
    }

    /// The `n/m` counter on the card border: bare under `creeper`, wrapped in
    /// accent pipes under `classic`, whose tables always have them.
    fn counter(&self, app: &App, position: usize, total: usize) -> Line<'static> {
        match self.skin {
            WsSkin::Creeper => Line::from(format!("{position}/{total}")),
            WsSkin::Classic => Line::from(vec![
                Span::styled("|", app.theme.dim()),
                Span::styled(format!("{position}/{total}"), app.theme.accent_style()),
                Span::styled("|", app.theme.dim()),
            ]),
            WsSkin::Paper => {
                let pal = app.paper_palette();
                Line::from(Span::styled(
                    format!("{position} / {total}"),
                    Style::default().fg(pal.faint).bg(pal.bg),
                ))
            }
            WsSkin::Shelf => {
                let pal = app.shelf_palette();
                Line::from(Span::styled(
                    format!("{position} / {total}"),
                    Style::default().fg(pal.faint).bg(pal.bg),
                ))
            }
        }
    }
}

impl App {
    /// What the row prints after the size column, and whether it is a problem.
    ///
    /// A world has no version-vs-warning story to tell, so its date and hardcore
    /// flag live here; everything else keeps the slot for its warning.
    pub(in crate::tui::app) fn trailing_note(
        &self,
        entry: &crate::core::content::ContentEntry,
    ) -> Option<(String, bool)> {
        use crate::core::content::ContentMeta;
        if let Some(name) = entry.warning.as_deref() {
            return Some((
                self.i18n
                    .format("workspace.warning.duplicate", &[("name", name)]),
                true,
            ));
        }
        let ContentMeta::World {
            last_played,
            hardcore,
            parsed,
            ..
        } = &entry.meta
        else {
            return None;
        };
        if !parsed {
            return Some((self.i18n.text("workspace.world.unreadable"), true));
        }
        let mut parts = Vec::new();
        if *hardcore {
            parts.push(self.i18n.text("workspace.world.hardcore"));
        }
        if let Some(millis) = last_played {
            parts.push(self.relative_time(*millis));
        }
        (!parts.is_empty()).then(|| (parts.join(" · "), false))
    }

    /// `2 分钟前` / `3 天前`, from a millisecond timestamp written by the game.
    fn relative_time(&self, millis: i64) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0);
        let seconds = now.saturating_sub(millis).max(0) / 1000;
        let (key, count) = match seconds {
            0..=59 => return self.i18n.text("workspace.time.now"),
            60..=3_599 => ("workspace.time.minutes", seconds / 60),
            3_600..=86_399 => ("workspace.time.hours", seconds / 3_600),
            86_400..=2_591_999 => ("workspace.time.days", seconds / 86_400),
            _ => ("workspace.time.months", seconds / 2_592_000),
        };
        self.i18n.format(key, &[("count", &count.to_string())])
    }
}
