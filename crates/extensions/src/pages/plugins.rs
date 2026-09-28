//! 插件子页：已安装插件管理（启用 / 设置 / 卸载）+ 安装区（zip / 开发目录）
//! + 插件市场浏览与安装。

use std::collections::HashSet;

use fluxdown_protocol::{InstalledPlugin, MarketEntryDto, PluginDto};
use fluxdown_ui_components::{
    ControlExt as _, FluxIcon, IconControlExt as _, card, form, form_field, input_with_action,
    tabular_numbers,
};
use fluxdown_ui_theme::active_theme;
use gpui::{
    AppContext as _, Context, Div, Entity, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    Disableable as _, Icon, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    h_flex,
    input::{Input, InputEvent, InputState},
    link::Link,
    switch::Switch,
    tooltip::Tooltip,
    v_flex,
};

use super::Frame;
use crate::{
    ExtensionsTab, ExtensionsView,
    components::{
        plugin_auth::PluginAuthDialog,
        plugin_detail::{PluginDetail, open_plugin_detail, yanked_label},
        plugin_settings::PluginSettingsForm,
    },
    controller::{COMPONENT_KINDS, component_wire_name},
    error_text, ui,
};

/// 市场列表每次展开的条数。
pub const MARKET_PAGE_SIZE: usize = 50;

/// 插件写操作类型；结果提示按此分流。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginOp {
    Install,
    Uninstall,
    SetEnabled,
}

pub(crate) struct PluginsUi {
    pub dev_mode: bool,
    pub dev_dir: String,
    pub installing_file: bool,
    pub installing_dir: bool,
    /// 有写操作在途的插件标识（禁用其卡片上的控件）。
    pub busy: HashSet<String>,
    pub market: MarketUi,
}

impl Default for PluginsUi {
    fn default() -> Self {
        Self {
            dev_mode: true,
            dev_dir: String::new(),
            installing_file: false,
            installing_dir: false,
            busy: HashSet::new(),
            market: MarketUi::default(),
        }
    }
}

pub(crate) struct MarketUi {
    pub requested: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub entries: Vec<MarketEntryDto>,
    pub search: Option<Entity<InputState>>,
    pub limit: usize,
    /// 安装在途的市场插件 id。
    pub pending: HashSet<String>,
}

impl Default for MarketUi {
    fn default() -> Self {
        Self {
            requested: false,
            loading: false,
            error: None,
            entries: Vec::new(),
            search: None,
            limit: MARKET_PAGE_SIZE,
            pending: HashSet::new(),
        }
    }
}

/// 市场条目关键字过滤：名称 / id / 描述 / 作者 / 标签任一命中（大小写不敏感）。
pub fn filter_market<'a>(entries: &'a [MarketEntryDto], query: &str) -> Vec<&'a MarketEntryDto> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return entries.iter().collect();
    }
    let hit = |value: &str| value.to_lowercase().contains(&query);
    entries
        .iter()
        .filter(|entry| {
            hit(&entry.name)
                || hit(&entry.plugin_id)
                || hit(&entry.description)
                || hit(&entry.author)
                || entry.tags.iter().any(|tag| hit(tag))
        })
        .collect()
}

impl ExtensionsView {
    pub(crate) fn render_plugins(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_market_loaded(window, cx);
        let search = self.ensure_market_search(window, cx);
        let query = search.read(cx).value().to_string();
        let theme = active_theme(cx);
        let frame = Frame {
            translator: self.translator.read(cx),
            tokens: theme.tokens(),
            extended: theme.extended(),
            components: theme.components(),
            stale: self.controller.is_stale(),
        };
        let Frame {
            translator,
            tokens,
            stale,
            ..
        } = frame;

        let plugins = self.controller.plugins();
        let installed = plugins
            .iter()
            .enumerate()
            .map(|(index, plugin)| {
                self.render_plugin_row(index, plugin, frame, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let installed_ids = plugins
            .iter()
            .map(|plugin| plugin.identity.as_str())
            .collect::<HashSet<_>>();

        v_flex()
            .w_full()
            .gap(tokens.spacing.md)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(ui::title_text(
                        translator.text("pluginsSectionTitle").to_owned(),
                        frame,
                    ))
                    .child(
                        h_flex()
                            .gap(tokens.spacing.sm)
                            .items_center()
                            .child(ui::meta_text(
                                translator.text("pluginDevModeSwitch").to_owned(),
                                frame,
                            ))
                            .child(
                                Switch::new("plugin-dev-mode")
                                    .checked(self.plugins.dev_mode)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.plugins.dev_mode = *checked;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(self.render_install_area(frame, cx))
            .child(if installed.is_empty() {
                ui::empty_state(
                    FluxIcon::Package,
                    translator.text("pluginsEmpty").to_owned(),
                    None,
                    frame,
                )
            } else {
                list_card(installed, frame, cx)
            })
            .child(
                v_flex()
                    .w_full()
                    .pt(tokens.spacing.lg)
                    .gap(tokens.spacing.xxs)
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(ui::title_text(
                                translator.text("marketSectionTitle").to_owned(),
                                frame,
                            ))
                            .child(
                                Button::new("market-refresh")
                                    .ghost()
                                    .control(cx)
                                    .label(translator.text("marketRefreshTooltip").to_owned())
                                    .loading(self.plugins.market.loading)
                                    .disabled(self.plugins.market.loading || stale)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.load_market(window, cx);
                                    })),
                            ),
                    )
                    .child(ui::meta_text(
                        translator.text("marketSectionDesc").to_owned(),
                        frame,
                    )),
            )
            .child(self.render_market(&search, &query, &installed_ids, frame, cx))
    }

    fn ensure_market_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(search) = &self.plugins.market.search {
            return search.clone();
        }
        let placeholder = self
            .translator
            .read(cx)
            .text("marketSearchPlaceholder")
            .to_owned();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.plugins.market.limit = MARKET_PAGE_SIZE;
                cx.notify();
            }
        })
        .detach();
        self.plugins.market.search = Some(search.clone());
        search
    }

    fn render_install_area(&self, frame: Frame<'_>, cx: &Context<Self>) -> impl IntoElement {
        let Frame {
            translator,
            tokens,
            stale,
            ..
        } = frame;
        let dev_dir = self.plugins.dev_dir.clone();
        let dev_dir_empty = dev_dir.is_empty();
        // 安装区：zip 安装按钮 + （开发模式下）「从目录安装」字段：只读路径框 +
        // 选择目录 / 安装 同行按钮，全部统一控件档。
        card(cx).w_full().p(tokens.spacing.md).child(
            form(cx)
                .child(
                    h_flex().child(
                        Button::new("plugin-install-zip")
                            .outline()
                            .icon(FluxIcon::FolderOpen)
                            .label(translator.text("pluginInstallZipButton").to_owned())
                            .control(cx)
                            .loading(self.plugins.installing_file)
                            .disabled(stale || self.plugins.installing_file)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.pick_plugin_zip(window, cx);
                            })),
                    ),
                )
                .when(self.plugins.dev_mode, |this| {
                    let display = if dev_dir_empty {
                        translator.text("pluginInstallDirPlaceholder").to_owned()
                    } else {
                        dev_dir
                    };
                    this.child(form_field(
                        translator.text("pluginInstallDirLabel").to_owned(),
                        input_with_action(
                            ui::path_box(display, dev_dir_empty, active_theme(cx)),
                            h_flex()
                                .gap(tokens.spacing.sm)
                                .items_center()
                                .child(
                                    Button::new("plugin-pick-dev-dir")
                                        .outline()
                                        .icon(FluxIcon::FolderOpen)
                                        .control_icon(cx)
                                        .tooltip(
                                            translator
                                                .text("pluginInstallDirPlaceholder")
                                                .to_owned(),
                                        )
                                        .disabled(self.plugins.installing_dir)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.pick_dev_dir(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("plugin-install-dev-dir")
                                        .primary()
                                        .label(translator.text("pluginInstallDirButton").to_owned())
                                        .control(cx)
                                        .loading(self.plugins.installing_dir)
                                        .disabled(
                                            stale || dev_dir_empty || self.plugins.installing_dir,
                                        )
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.install_dev_dir(window, cx);
                                        })),
                                ),
                            cx,
                        ),
                        None,
                        cx,
                    ))
                }),
        )
    }

    fn render_plugin_row(
        &self,
        index: usize,
        plugin: &PluginDto,
        frame: Frame<'_>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let Frame {
            translator,
            tokens,
            stale,
            ..
        } = frame;
        let destructive = tokens.colors.destructive;
        let identity = plugin.identity.clone();
        let busy = stale || self.plugins.busy.contains(&plugin.identity);
        let load_failed = plugin.load_status == "Failed";
        // 颜色是信号：只有加载失败 / 熔断停用着 destructive，其余状态中性。
        let badges = [
            plugin
                .dev_mode
                .then(|| ui::neutral_pill(translator.text("pluginDevModeBadge").to_owned(), frame)),
            Some(if load_failed {
                ui::tone_pill(
                    translator.text("pluginLoadStatusFailed").to_owned(),
                    destructive,
                    frame,
                )
            } else {
                ui::neutral_pill(translator.text("pluginLoadStatusLoaded").to_owned(), frame)
            }),
            (plugin.disabled_reason == "Manual").then(|| {
                ui::neutral_pill(translator.text("pluginDisabledManual").to_owned(), frame)
            }),
            (plugin.disabled_reason == "CircuitBreaker").then(|| {
                ui::tone_pill(
                    translator.text("pluginDisabledCircuitBreaker").to_owned(),
                    destructive,
                    frame,
                )
            }),
        ];
        let detail = PluginDetail::from_plugin(plugin);
        let detail_translator = translator.clone();
        list_row(frame)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(tokens.spacing.xxs)
                    .child(
                        h_flex()
                            .gap(tokens.spacing.sm)
                            .items_center()
                            .flex_wrap()
                            .child(ui::title_text(plugin.name.clone(), frame))
                            .child(
                                ui::meta_text(format!("v{}", plugin.version), frame)
                                    .font_features(tabular_numbers()),
                            )
                            .when(!plugin.homepage.is_empty(), |this| {
                                this.child(
                                    Link::new(("plugin-homepage", index))
                                        .href(plugin.homepage.clone())
                                        .text_size(tokens.typography.xs.size)
                                        .child(plugin.homepage.clone()),
                                )
                            })
                            .children(badges.into_iter().flatten()),
                    )
                    .when(!plugin.description.is_empty(), |this| {
                        this.child(
                            ui::meta_text(plugin.description.clone(), frame)
                                .w_full()
                                .truncate(),
                        )
                    })
                    .when(load_failed && !plugin.load_error.is_empty(), |this| {
                        let load_error = plugin.load_error.clone();
                        let load_error_tooltip = plugin.load_error.clone();
                        this.child(
                            ui::meta_text(load_error, frame)
                                .id(("plugin-load-error", index))
                                .w_full()
                                .truncate()
                                .text_color(destructive)
                                .tooltip(move |window, cx| {
                                    Tooltip::new(load_error_tooltip.clone()).build(window, cx)
                                }),
                        )
                    }),
            )
            .child(
                Button::new(("plugin-detail", index))
                    .ghost()
                    .control_icon(cx)
                    .icon(FluxIcon::Info)
                    .tooltip(translator.text("pluginDetailDescription").to_owned())
                    .on_click(move |_, window, cx| {
                        open_plugin_detail(detail.clone(), detail_translator.clone(), window, cx);
                    }),
            )
            .when(!load_failed && !plugin.settings.is_empty(), |this| {
                let identity = identity.clone();
                this.child(
                    Button::new(("plugin-settings", index))
                        .ghost()
                        .control_icon(cx)
                        .icon(FluxIcon::Settings)
                        .tooltip(translator.text("pluginSettingsTooltip").to_owned())
                        .disabled(busy || load_failed)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_plugin_settings(&identity, window, cx);
                        })),
                )
            })
            .when(!load_failed && plugin.auth_supported, |this| {
                let identity = identity.clone();
                this.child(
                    Button::new(("plugin-auth", index))
                        .outline()
                        .control(cx)
                        .label(translator.text("pluginAuthButton").to_owned())
                        .disabled(busy || load_failed)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_plugin_auth(&identity, window, cx);
                        })),
                )
            })
            .child(
                Button::new(("plugin-uninstall", index))
                    .ghost()
                    .control_icon(cx)
                    .icon(FluxIcon::Trash2)
                    .tooltip(translator.text("pluginUninstallTooltip").to_owned())
                    .disabled(busy)
                    .on_click({
                        let name = plugin.name.clone();
                        let identity = identity.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.confirm_uninstall_plugin(
                                identity.clone(),
                                name.clone(),
                                window,
                                cx,
                            );
                        })
                    }),
            )
            .child(
                Switch::new(("plugin-enabled", index))
                    .checked(plugin.enabled && !load_failed)
                    .disabled(busy || load_failed)
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        let future = this
                            .controller
                            .set_plugin_enabled(identity.clone(), *checked);
                        this.run_plugin_op(
                            identity.clone(),
                            PluginOp::SetEnabled,
                            future,
                            window,
                            cx,
                        );
                    })),
            )
    }

    fn render_market(
        &self,
        search: &Entity<InputState>,
        query: &str,
        installed_ids: &HashSet<&str>,
        frame: Frame<'_>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let Frame {
            translator, tokens, ..
        } = frame;
        let market = &self.plugins.market;
        let mut root = v_flex().w_full().gap(tokens.spacing.sm);
        if market.loading && market.entries.is_empty() {
            return root.child(ui::meta_text(
                translator.text("pluginCommonLoading").to_owned(),
                frame,
            ));
        }
        if let Some(error) = &market.error {
            return root.child(ui::status_line(
                FluxIcon::CircleAlert,
                tokens.colors.destructive,
                translator.text_with("marketLoadFailed", &[("message", error)]),
                frame,
            ));
        }
        if market.entries.is_empty() {
            return root.child(ui::empty_state(
                FluxIcon::Package,
                translator.text("marketEmpty").to_owned(),
                None,
                frame,
            ));
        }
        root = root.child(
            Input::new(search)
                .control(cx)
                .w_full()
                .prefix(
                    Icon::new(FluxIcon::Search)
                        .size(frame.extended.icon.md)
                        .text_color(tokens.colors.muted_foreground),
                )
                .cleanable(true),
        );
        let filtered = filter_market(&market.entries, query);
        if filtered.is_empty() {
            return root.child(ui::empty_state(
                FluxIcon::Search,
                translator.text("marketSearchNoResult").to_owned(),
                None,
                frame,
            ));
        }
        let remaining = filtered.len().saturating_sub(market.limit);
        let rows = filtered
            .iter()
            .take(market.limit)
            .enumerate()
            .map(|(index, entry)| {
                let installed = installed_ids.contains(entry.plugin_id.as_str());
                let pending = market.pending.contains(&entry.plugin_id);
                self.render_market_row(index, entry, installed, pending, frame, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        root.child(list_card(rows, frame, cx))
            .when(remaining > 0, |this| {
                this.child(
                    h_flex().justify_center().child(
                        Button::new("market-show-more")
                            .ghost()
                            .control(cx)
                            .label(
                                translator.text_with(
                                    "marketShowMore",
                                    &[("count", &remaining.to_string())],
                                ),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.plugins.market.limit += MARKET_PAGE_SIZE;
                                cx.notify();
                            })),
                    ),
                )
            })
    }

    fn render_market_row(
        &self,
        index: usize,
        entry: &MarketEntryDto,
        installed: bool,
        pending: bool,
        frame: Frame<'_>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let Frame {
            translator,
            tokens,
            stale,
            ..
        } = frame;
        let name = if entry.name.is_empty() {
            entry.plugin_id.clone()
        } else {
            entry.name.clone()
        };
        let yanked = yanked_label(translator, &entry.yanked);
        let detail = PluginDetail::from_market(entry, translator);
        let detail_translator = translator.clone();
        let plugin_id = entry.plugin_id.clone();
        let install_label = if installed {
            translator.text("marketInstalledButton")
        } else if pending {
            translator.text("marketInstallingButton")
        } else {
            translator.text("marketInstallButton")
        }
        .to_owned();
        list_row(frame)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(tokens.spacing.xxs)
                    .child(
                        h_flex()
                            .gap(tokens.spacing.sm)
                            .items_center()
                            .flex_wrap()
                            .child(ui::title_text(name, frame))
                            .child(
                                ui::meta_text(format!("v{}", entry.version), frame)
                                    .font_features(tabular_numbers()),
                            )
                            .when(!entry.author.is_empty(), |this| {
                                this.child(ui::meta_text(entry.author.clone(), frame))
                            })
                            .when(!entry.homepage.is_empty(), |this| {
                                this.child(
                                    Link::new(("market-homepage", index))
                                        .href(entry.homepage.clone())
                                        .text_size(tokens.typography.xs.size)
                                        .child(entry.homepage.clone()),
                                )
                            })
                            .children(yanked.map(|label| {
                                ui::tone_pill(label, tokens.colors.destructive, frame)
                            })),
                    )
                    .when(!entry.description.is_empty(), |this| {
                        this.child(
                            ui::meta_text(entry.description.clone(), frame)
                                .w_full()
                                .truncate(),
                        )
                    }),
            )
            .child(
                Button::new(("market-detail", index))
                    .ghost()
                    .control_icon(cx)
                    .icon(FluxIcon::Info)
                    .tooltip(translator.text("pluginDetailDescription").to_owned())
                    .on_click(move |_, window, cx| {
                        open_plugin_detail(detail.clone(), detail_translator.clone(), window, cx);
                    }),
            )
            .child(
                Button::new(("market-install", index))
                    .outline()
                    .control(cx)
                    .label(install_label)
                    .loading(pending)
                    .disabled(installed || pending || stale)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.install_market_plugin(plugin_id.clone(), window, cx);
                    })),
            )
    }

    fn ensure_market_loaded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.plugins.market.requested || self.controller.is_stale() {
            return;
        }
        self.load_market(window, cx);
    }

    fn load_market(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let market = &mut self.plugins.market;
        market.requested = true;
        market.loading = true;
        market.error = None;
        let future = self.controller.market_list();
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |this, cx| {
                let market = &mut this.plugins.market;
                market.loading = false;
                match result.and_then(|value| {
                    serde_json::from_value::<Vec<MarketEntryDto>>(value).map_err(|_| {
                        fluxdown_protocol::RpcErrorData::new(
                            fluxdown_protocol::ApplicationErrorCode::ProtocolIncompatible,
                            false,
                        )
                    })
                }) {
                    Ok(entries) => {
                        market.entries = entries;
                        market.error = None;
                        market.limit = MARKET_PAGE_SIZE;
                    }
                    Err(error) => {
                        market.error = Some(error_text(this.translator.read(cx), &error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn pick_plugin_zip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.plugins.installing_file {
            return;
        }
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.translator
                    .read(cx)
                    .text("pluginInstallZipButton")
                    .to_owned(),
            )),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.plugins.installing_file = true;
                let future = this
                    .controller
                    .install_plugin_file(path.display().to_string());
                cx.spawn_in(window, async move |this, cx| {
                    let result = future.await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.plugins.installing_file = false;
                        this.finish_plugin_op(PluginOp::Install, result, window, cx);
                        cx.notify();
                    });
                })
                .detach();
                cx.notify();
            });
        })
        .detach();
    }

    fn pick_dev_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let text = path.display().to_string();
                let _ = this.update(cx, |this, cx| {
                    this.plugins.dev_dir = text;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn install_dev_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.plugins.dev_dir.is_empty() || self.plugins.installing_dir {
            return;
        }
        self.plugins.installing_dir = true;
        let future = self
            .controller
            .install_plugin_dev(self.plugins.dev_dir.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.plugins.installing_dir = false;
                if result.is_ok() {
                    this.plugins.dev_dir.clear();
                }
                this.finish_plugin_op(PluginOp::Install, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn install_market_plugin(
        &mut self,
        plugin_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.plugins.market.pending.insert(plugin_id.clone()) {
            return;
        }
        let future = self.controller.market_install(plugin_id.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.plugins.market.pending.remove(&plugin_id);
                this.finish_plugin_op(PluginOp::Install, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn run_plugin_op(
        &mut self,
        identity: String,
        op: PluginOp,
        future: crate::PortFuture<serde_json::Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.plugins.busy.insert(identity.clone()) {
            return;
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = future.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.plugins.busy.remove(&identity);
                this.finish_plugin_op(op, result, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// 写操作结果的全局提示；安装成功且缺少基础组件时追加依赖提醒。
    fn finish_plugin_op(
        &mut self,
        op: PluginOp,
        result: Result<serde_json::Value, fluxdown_protocol::RpcErrorData>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let translator = self.translator.read(cx);
        match (op, result) {
            (PluginOp::Install, Ok(value)) => {
                let message = translator.text("pluginOpInstallSuccess").to_owned();
                let missing = serde_json::from_value::<InstalledPlugin>(value)
                    .map(|installed| installed.missing_components)
                    .unwrap_or_default();
                self.toast_success(message, window, cx);
                if !missing.is_empty() {
                    self.show_missing_components(&missing, window, cx);
                }
            }
            (PluginOp::Uninstall, Ok(_)) => {
                let message = translator.text("pluginOpUninstallSuccess").to_owned();
                self.toast_success(message, window, cx);
            }
            (PluginOp::SetEnabled, Ok(_)) => {}
            (op, Err(error)) => {
                let detail = error_text(translator, &error);
                let key = match op {
                    PluginOp::Install => "pluginOpInstallFailed",
                    PluginOp::Uninstall => "pluginOpUninstallFailed",
                    PluginOp::SetEnabled => "pluginOpEnabledFailed",
                };
                let message = translator.text_with(key, &[("message", &detail)]);
                self.toast_error(message, window, cx);
            }
        }
    }

    fn show_missing_components(
        &self,
        missing: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let translator = self.translator.read(cx);
        let names = missing
            .iter()
            .map(|component| {
                COMPONENT_KINDS
                    .into_iter()
                    .find(|kind| component_wire_name(*kind) == component)
                    .map(|kind| {
                        translator
                            .text(super::managed_components::title_key(kind))
                            .to_owned()
                    })
                    .unwrap_or_else(|| component.clone())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let title = SharedString::from(translator.text("pluginDepsMissingTitle").to_owned());
        let body = SharedString::from(
            translator.text_with("pluginDepsMissingBody", &[("components", &names)]),
        );
        let later = SharedString::from(translator.text("pluginDepsLater").to_owned());
        let go = SharedString::from(translator.text("pluginDepsGoToComponents").to_owned());
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let view = view.clone();
            alert
                .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
                .description(body.clone())
                .footer(fluxdown_ui_components::dialog_footer(
                    Some(later.clone()),
                    go.clone(),
                    fluxdown_ui_components::DialogIntent::Confirm,
                    cx,
                ))
                .on_ok(move |_, _, cx| {
                    let _ =
                        view.update(cx, |this, cx| this.show_tab(ExtensionsTab::Components, cx));
                    true
                })
        });
    }

    fn confirm_uninstall_plugin(
        &self,
        identity: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let translator = self.translator.read(cx);
        let title = SharedString::from(translator.text("pluginUninstallTitle").to_owned());
        let body =
            SharedString::from(translator.text_with("pluginUninstallMsg", &[("name", &name)]));
        let ok = SharedString::from(translator.text("pluginUninstallTooltip").to_owned());
        let cancel = SharedString::from(translator.text("cancel").to_owned());
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let view = view.clone();
            let identity = identity.clone();
            alert
                .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
                .description(body.clone())
                .footer(fluxdown_ui_components::dialog_footer(
                    Some(cancel.clone()),
                    ok.clone(),
                    fluxdown_ui_components::DialogIntent::Destructive,
                    cx,
                ))
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        let future = this.controller.uninstall_plugin(identity.clone());
                        this.run_plugin_op(
                            identity.clone(),
                            PluginOp::Uninstall,
                            future,
                            window,
                            cx,
                        );
                    });
                    true
                })
        });
    }

    fn open_plugin_settings(&self, identity: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(plugin) = self.controller.plugin(identity) else {
            return;
        };
        let translator = self.translator.read(cx);
        let title = SharedString::from(
            translator.text_with("pluginSettingsDialogTitle", &[("name", &plugin.name)]),
        );
        let save = SharedString::from(translator.text("pluginSettingsSaveButton").to_owned());
        let saving = SharedString::from(translator.text("pluginSettingsSaving").to_owned());
        let cancel = SharedString::from(translator.text("cancel").to_owned());
        let translator_entity = self.translator.clone();
        let port = self.controller.port().clone();
        let form =
            cx.new(|cx| PluginSettingsForm::new(translator_entity, port, plugin, window, cx));
        window.open_dialog(cx, move |dialog, _, cx| {
            let is_saving = form.read(cx).is_saving();
            let form_for_content = form.clone();
            let form_for_save = form.clone();
            dialog
                .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
                .w(px(560.))
                .overlay_closable(!is_saving)
                .content(move |content, _, _| content.child(form_for_content.clone()))
                .footer(
                    DialogFooter::new()
                        .gap(active_theme(cx).tokens().spacing.sm)
                        .child(
                            Button::new("plugin-settings-cancel")
                                .outline()
                                .control(cx)
                                .label(cancel.clone())
                                .disabled(is_saving)
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("plugin-settings-save")
                                .primary()
                                .control(cx)
                                .label(if is_saving {
                                    saving.clone()
                                } else {
                                    save.clone()
                                })
                                .loading(is_saving)
                                .disabled(is_saving)
                                .on_click(move |_, window, cx| {
                                    form_for_save.update(cx, |form, cx| form.submit(window, cx));
                                }),
                        ),
                )
        });
    }

    fn open_plugin_auth(&self, identity: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(plugin) = self.controller.plugin(identity) else {
            return;
        };
        let translator = self.translator.clone();
        let port = self.controller.port().clone();
        let identity = plugin.identity.clone();
        let title = SharedString::from(
            self.translator
                .read(cx)
                .text_with("pluginAuthDialogTitle", &[("name", &plugin.name)]),
        );
        let dialog = cx.new(|cx| PluginAuthDialog::new(translator, port, identity, window, cx));
        let dialog_for_cancel = dialog.clone();
        window.open_dialog(cx, move |dialog_view, _, cx| {
            let dialog_for_content = dialog.clone();
            let dialog_for_cancel = dialog_for_cancel.clone();
            dialog_view
                .title(fluxdown_ui_components::dialog_title(title.clone(), cx))
                .w(px(520.))
                .on_cancel(move |_, _, cx| {
                    dialog_for_cancel.update(cx, |this, cx| this.cancel_session(cx));
                    true
                })
                .content(move |content, _, _| content.child(dialog_for_content.clone()))
        });
    }
}

/// 列表卡片：一张 surface 卡片内纵向排列各行，行间 hairline 分隔（与下载表格同一「少线」风格）。
fn list_card(rows: Vec<gpui::AnyElement>, frame: Frame<'_>, cx: &gpui::App) -> Div {
    let count = rows.len();
    card(cx)
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .children(rows.into_iter().enumerate().flat_map(move |(index, row)| {
            let divider = (index + 1 < count).then(|| ui::divider(frame).into_any_element());
            std::iter::once(row).chain(divider)
        }))
}

/// 列表行：横向排布、行内控件间距 `spacing.sm`，内边距 md / sm，悬停 `row_hover`。
fn list_row(frame: Frame<'_>) -> Div {
    let hover = frame.extended.colors.row_hover;
    h_flex()
        .w_full()
        .gap(frame.tokens.spacing.sm)
        .items_center()
        .px(frame.tokens.spacing.md)
        .py(frame.tokens.spacing.sm)
        .hover(move |style| style.bg(hover))
}

#[cfg(test)]
mod tests {
    use fluxdown_protocol::MarketEntryDto;

    use super::filter_market;

    fn entry(
        id: &str,
        name: &str,
        description: &str,
        author: &str,
        tags: &[&str],
    ) -> MarketEntryDto {
        MarketEntryDto {
            plugin_id: id.to_owned(),
            version: "1.0.0".to_owned(),
            sequence: 1,
            content_hash: String::new(),
            min_app_version: String::new(),
            name: name.to_owned(),
            description: description.to_owned(),
            author: author.to_owned(),
            homepage: String::new(),
            mirrors: Vec::new(),
            publish_time: String::new(),
            yanked: String::new(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            permissions: Vec::new(),
        }
    }

    #[test]
    fn empty_query_keeps_everything() {
        let entries = [
            entry("a", "Alpha", "", "", &[]),
            entry("b", "Beta", "", "", &[]),
        ];
        assert_eq!(filter_market(&entries, "   ").len(), 2);
    }

    #[test]
    fn query_matches_any_field_case_insensitively() {
        let entries = [
            entry("video.dl", "Video", "grabs videos", "Ann", &["media"]),
            entry("other", "Other", "misc", "Bob", &["tools"]),
        ];
        let ids = |query: &str| {
            filter_market(&entries, query)
                .into_iter()
                .map(|entry| entry.plugin_id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("VIDEO"), vec!["video.dl"]);
        assert_eq!(ids("bob"), vec!["other"]);
        assert_eq!(ids("media"), vec!["video.dl"]);
        assert_eq!(ids("grabs"), vec!["video.dl"]);
        assert!(ids("nothing").is_empty());
    }
}
