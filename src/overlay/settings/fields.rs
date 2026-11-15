//! The static config-field table for the settings overlay: every editable
//! [`crate::config::AppConfig`] field as a [`Field`] row. A [`Kind`] bundles the
//! field's presentation with its typed get/set accessors into the config, so
//! adding a setting is adding one row to [`FIELDS`]. The binding list is not
//! here — it's dynamic, behind a [`Kind::Door`] row (see [`super::controls`]).

use super::SettingsSection;
use crate::config::{
    bounds, AppConfig, Channel, CursorMode, ExperimentalPreset, HomeStyle, MemoryProfile, OskStyle,
    PadLayout, PageTheme, Scaling, ToolbarPosition,
};
use crate::overlay::quick_access::Modes;
use crate::platform::render::shaders;

/// How a field is displayed, edited, and reached in a config. `Choice` carries
/// `(label, stored value)` pairs; `Int`/`Float` carry the bounds dpad steps
/// within (so the renderer and the adjust logic share one source of truth for
/// the range). The `get`/`set` fn pointers are the field's only binding to
/// [`AppConfig`].
pub enum Kind {
    Bool {
        get: fn(&AppConfig) -> bool,
        set: fn(&mut AppConfig, bool),
    },
    /// A row that runs something instead of holding a value: the first A arms
    /// it, the second runs it (see [`super::Settings::confirm_action`]).
    Action { task: Task },
    /// A row that opens another screen.
    Door { door: Door },
    /// Free text, typed via the on-screen keyboard (Left/Right does nothing; A
    /// opens it). `get_mut` hands the OSK the draft's own buffer.
    Text {
        get: fn(&AppConfig) -> String,
        get_mut: fn(&mut AppConfig) -> &mut String,
    },
    Choice {
        opts: Opts,
        get: fn(&AppConfig) -> String,
        set: fn(&mut AppConfig, &str),
    },
    Int {
        min: i64,
        max: i64,
        step: i64,
        /// Label shown instead of a bare `0` (e.g. "不限").
        zero: Option<&'static str>,
        get: fn(&AppConfig) -> i64,
        set: fn(&mut AppConfig, i64),
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
        decimals: usize,
        get: fn(&AppConfig) -> f64,
        set: fn(&mut AppConfig, f64),
    },
}

/// A choice row's `(label, stored value)` pairs.
pub enum Opts {
    Fixed(&'static [(&'static str, &'static str)]),
    /// Listed when stepped; `label` names a value without listing.
    Listed {
        list: fn() -> Vec<(String, String)>,
        label: fn(&str) -> String,
    },
}

impl Opts {
    fn list(&self) -> Vec<(String, String)> {
        match self {
            Opts::Fixed(opts) => opts
                .iter()
                .map(|(l, v)| (l.to_string(), v.to_string()))
                .collect(),
            Opts::Listed { list, .. } => list(),
        }
    }

    fn label(&self, value: &str) -> String {
        match self {
            Opts::Fixed(opts) => opts
                .iter()
                .find(|(_, v)| *v == value)
                .map_or_else(|| value.to_string(), |(l, _)| l.to_string()),
            Opts::Listed { label, .. } => label(value),
        }
    }
}

/// `Kind::Bool` over a config path.
macro_rules! flag {
    ($($seg:ident).+) => {
        Kind::Bool {
            get: |c| c.$($seg).+,
            set: |c, v| c.$($seg).+ = v,
        }
    };
}

/// `Kind::Text` over a config `String` path.
macro_rules! text {
    ($($seg:ident).+) => {
        Kind::Text {
            get: |c| c.$($seg).+.clone(),
            get_mut: |c| &mut c.$($seg).+,
        }
    };
}

/// `Kind::Choice` over a `token_enum!` config path (`CHOICES` / `as_str` /
/// `from_value`).
macro_rules! choice {
    ($($seg:ident).+: $ty:ty) => {
        Kind::Choice {
            opts: Opts::Fixed(<$ty>::CHOICES),
            get: |c| c.$($seg).+.as_str().to_string(),
            set: |c, v| c.$($seg).+ = <$ty>::from_value(v),
        }
    };
}

/// `Kind::Int` over a config path (cast to/from its native `$ty`), ranged by a
/// shared [`bounds::IntBounds`]; `step` is the GUI dpad step, the optional
/// trailing label replaces a bare `0`.
macro_rules! int {
    ($($seg:ident).+ as $ty:ty, $b:expr, $step:expr) => {
        int!($($seg).+ as $ty, $b, $step, None)
    };
    ($($seg:ident).+ as $ty:ty, $b:expr, $step:expr, $zero:expr) => {
        Kind::Int {
            min: $b.min,
            max: $b.max,
            step: $step,
            zero: $zero,
            get: |c| c.$($seg).+ as i64,
            set: |c, v| c.$($seg).+ = v as $ty,
        }
    };
}

/// `Kind::Float` over a config path (cast to/from its native `$ty`), ranged by
/// a shared [`bounds::FloatBounds`]; `step` is the GUI dpad step, `decimals`
/// the display precision.
macro_rules! float {
    ($($seg:ident).+ as $ty:ty, $b:expr, $step:expr, $decimals:expr) => {
        Kind::Float {
            min: $b.min,
            max: $b.max,
            step: $step,
            decimals: $decimals,
            get: |c| c.$($seg).+ as f64,
            set: |c, v| c.$($seg).+ = v as $ty,
        }
    };
}

/// The screen a [`Kind::Door`] row opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    /// The binding list (see [`super::controls`]).
    Bindings,
    /// Game Mode's input-map screens, which [`super::Settings::open_door`]
    /// leaves to its caller.
    InputMaps,
}

/// What a [`Kind::Action`] row runs; the app does the work (see
/// [`crate::app`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    /// Wipe history, site data, the HTTP cache, the saved session and the open
    /// tabs. Bookmarks, pins and the settings themselves stay.
    ClearData,
    /// Every settings row, the speed-dial pins and the control bindings back to
    /// how they ship. The user's own content is [`Task::ClearData`]'s business.
    RestoreDefaults,
    /// Hand the log files to the system share sheet.
    #[cfg(target_os = "android")]
    ShareLogs,
}

impl Task {
    /// The verb shown as the row's value — what the confirmed press will do.
    pub fn verb(self) -> &'static str {
        match self {
            Task::ClearData => "清除",
            Task::RestoreDefaults => "恢复",
            #[cfg(target_os = "android")]
            Task::ShareLogs => "分享",
        }
    }

    /// Whether the task destroys something and so waits for a second press.
    pub fn needs_confirm(self) -> bool {
        match self {
            Task::ClearData | Task::RestoreDefaults => true,
            #[cfg(target_os = "android")]
            Task::ShareLogs => false,
        }
    }
}

/// A row in a settings tab. `cat` is its sub-header, drawn only in a tab with
/// several (empty: none); `restart` marks a field the running app can't apply
/// live, flagged with `*`.
pub struct Field {
    pub section: SettingsSection,
    pub cat: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub restart: bool,
    /// Also a quick row, in these modes.
    pub quick: Option<Modes>,
    /// The quick row's label, where `label` leans on its sub-header.
    quick_label: Option<&'static str>,
    /// Shown only where the page draws through shaders.
    shaders: bool,
}

impl Field {
    const fn quick(mut self, modes: Modes) -> Self {
        self.quick = Some(modes);
        self
    }

    const fn shaders(mut self) -> Self {
        self.shaders = true;
        self
    }

    pub fn shown(&self) -> bool {
        !self.shaders || crate::platform::window::page_shaders()
    }

    const fn quick_as(mut self, modes: Modes, label: &'static str) -> Self {
        self.quick_label = Some(label);
        self.quick(modes)
    }

    /// What the row reads as out of its tab.
    pub fn quick_label(&self) -> &'static str {
        self.quick_label.unwrap_or(self.label)
    }
}

/// A switch's state as a row shows it.
pub fn on_off(on: bool) -> &'static str {
    if on {
        "开启"
    } else {
        "关闭"
    }
}

/// `kind`'s value in `config` as a row shows it; empty for a row with none.
pub fn value_of(kind: &Kind, config: &AppConfig) -> String {
    match kind {
        Kind::Action { task } => task.verb().to_string(),
        Kind::Door { .. } => String::new(),
        Kind::Bool { get, .. } => on_off(get(config)).to_string(),
        Kind::Text { get, .. } => {
            let t = get(config);
            if t.is_empty() {
                "(default)".to_string()
            } else {
                t
            }
        }
        Kind::Choice { opts, get, .. } => opts.label(&get(config)),
        Kind::Int { zero, get, .. } => {
            let v = get(config);
            match zero {
                Some(label) if v == 0 => label.to_string(),
                _ => format!("{v}"),
            }
        }
        Kind::Float { decimals, get, .. } => format!("{:.*}", decimals, get(config)),
    }
}

/// Step `kind` in `config` by `dx` (-1 left, +1 right): toggle a bool, cycle a
/// choice, or step a number within its bounds. Rows holding no value ignore it.
pub fn step(kind: &Kind, config: &mut AppConfig, dx: i32) {
    match kind {
        Kind::Text { .. } | Kind::Action { .. } | Kind::Door { .. } => {}
        Kind::Bool { get, set } => {
            let v = !get(config);
            set(config, v);
        }
        Kind::Choice { opts, get, set } => {
            let cur = get(config);
            let opts = opts.list();
            let n = opts.len() as i32;
            let idx = opts.iter().position(|(_, v)| *v == cur).unwrap_or(0) as i32;
            let next = (idx + dx).rem_euclid(n) as usize;
            set(config, &opts[next].1);
        }
        Kind::Int {
            min,
            max,
            step,
            get,
            set,
            ..
        } => {
            let v = (get(config) + dx as i64 * step).clamp(*min, *max);
            set(config, v);
        }
        Kind::Float {
            min,
            max,
            step,
            get,
            set,
            ..
        } => {
            let v = (get(config) + dx as f64 * step).clamp(*min, *max);
            set(config, v);
        }
    }
}

/// User-Agent presets: the keywords [`crate::config::BrowserConfig::user_agent`]
/// understands (empty keeps Servo's platform default).
const UA_CHOICES: &[(&str, &str)] = &[
    ("默认", ""),
    ("桌面", "desktop"),
    ("移动设备", "mobile"),
    ("iOS", "ios"),
];

/// The User-Agent choice — a free `String` in the config, not a `token_enum!`:
/// a value outside [`UA_CHOICES`] shows verbatim and cycles back into the list
/// when adjusted.
const fn ua_kind() -> Kind {
    Kind::Choice {
        opts: Opts::Fixed(UA_CHOICES),
        get: |c| c.browser.user_agent.clone(),
        set: |c, v| c.browser.user_agent = v.to_string(),
    }
}

/// The page shader; its options include the user's files.
const SHADER: Kind = Kind::Choice {
    opts: Opts::Listed {
        list: shaders::list,
        label: shaders::label,
    },
    get: |c| c.game_mode.view.shader.clone(),
    set: |c, v| c.game_mode.view.shader = v.to_string(),
};

/// The Web-features preset choice — derived from the experimental bools (shows
/// "自定义" when they match no preset); picking a preset rewrites all of them
/// (the bools are the source of truth, see [`ExperimentalPreset`]).
const fn web_features_kind() -> Kind {
    Kind::Choice {
        opts: Opts::Fixed(ExperimentalPreset::CHOICES),
        get: |c| {
            ExperimentalPreset::detect(&c.experimental)
                .as_str()
                .to_string()
        },
        set: |c, v| c.experimental = ExperimentalPreset::from_value(v).features(),
    }
}

/// Compact constructor for the [`FIELDS`] table — without it `rustfmt` explodes
/// each `Field` literal across six lines and drowns the table.
const fn f(
    section: SettingsSection,
    cat: &'static str,
    label: &'static str,
    kind: Kind,
    restart: bool,
) -> Field {
    Field {
        section,
        cat,
        label,
        kind,
        restart,
        quick: None,
        quick_label: None,
        shaders: false,
    }
}

use SettingsSection as S;

/// Every editable config field, in display order (grouped by [`SettingsSection`]).
/// The binding list is not here: it is built dynamically behind a door row.
#[rustfmt::skip]
pub(super) static FIELDS: &[Field] = &[
    f(S::Browser,  "浏览器",     "主页",              text!(browser.home_page), false),
    f(S::Browser,  "浏览器",     "搜索网址模板",             text!(browser.search_page), false),
    f(S::Browser,  "浏览器",     "用户代理",             ua_kind(), false).quick(Modes::Browser),
    f(S::Browser,  "浏览器",     "网页缩放",              float!(browser.page_zoom as f32, bounds::PAGE_ZOOM, 0.05, 2), false),
    f(S::Browser,  "浏览器",     "网页主题",             choice!(browser.page_theme: PageTheme), false).quick(Modes::Browser),
    f(S::Browser,  "浏览器",     "恢复标签页",           flag!(browser.restore_tabs), false),
    f(S::Browser,  "浏览器",     "标签页数量上限",               int!(browser.max_tabs as u32, bounds::MAX_TABS, 1, Some("不限")), false),
    f(S::Browser,  "浏览器",     "保留网站数据",         flag!(browser.persist_site_data), true),

    f(S::Browser,  "实验性功能", "网页功能",          web_features_kind(), false),
    f(S::Browser,  "实验性功能", "WebGL 2",               flag!(experimental.webgl2), false),
    f(S::Browser,  "实验性功能", "WebGPU",                flag!(experimental.webgpu), false),
    f(S::Browser,  "实验性功能", "OffscreenCanvas",       flag!(experimental.offscreen_canvas), false),
    f(S::Browser,  "实验性功能", "CSS 多列布局",           flag!(experimental.columns), false),
    f(S::Browser,  "实验性功能", "容器查询",     flag!(experimental.container_queries), false),
    f(S::Browser,  "实验性功能", "网页字体",              flag!(experimental.fontface), false),
    f(S::Browser,  "实验性功能", "IntersectionObserver",  flag!(experimental.intersection_observer), false),
    f(S::Browser,  "实验性功能", "ResizeObserver",        flag!(experimental.resize_observer), false),
    f(S::Browser,  "实验性功能", "IndexedDB",             flag!(experimental.indexeddb), false),
    f(S::Browser,  "实验性功能", "StorageManager",        flag!(experimental.storage_manager), false),
    f(S::Browser,  "实验性功能", "通知",         flag!(experimental.notification), false),
    f(S::Browser,  "实验性功能", "异步剪贴板",       flag!(experimental.async_clipboard), false),
    f(S::Browser,  "实验性功能", "权限",           flag!(experimental.permissions), false),

    f(S::Gaming, "游戏模式",   "按键映射",              Kind::Door { door: Door::InputMaps }, false),
    f(S::Gaming, "游戏模式",   "画面缩放",                choice!(game_mode.view.scaling: Scaling), false).quick(Modes::Game),
    f(S::Gaming, "游戏模式",   "着色器",                 SHADER, false).shaders().quick(Modes::Game),

    f(S::Interface, "界面",  "界面缩放",        float!(interface.scale as f32, bounds::SCALE, bounds::SCALE_STEP, 2), false),
    f(S::Interface, "界面",  "工具栏位置",       choice!(interface.toolbar_position: ToolbarPosition), false),
    f(S::Interface, "界面",  "自动隐藏工具栏",      flag!(interface.toolbar_autohide), false),
    f(S::Interface, "界面",  "主页样式",             choice!(interface.home_style: HomeStyle), false),
    f(S::Interface, "界面",  "网页图标",             flag!(interface.page_icons), false),
    f(S::Interface, "界面",  "光标停留时间（毫秒）",     int!(interface.cursor_linger_ms as u64, bounds::CURSOR_LINGER_MS, 100), false),

    // No sub-header: the door belongs to no group.
    f(S::Controls, "",            "Button bindings",        Kind::Door { door: Door::Bindings }, false),

    f(S::Controls, "手柄",     "手柄布局",         choice!(controls.pad_layout: PadLayout), false),
    f(S::Controls, "手柄",     "交换 A/B 和 X/Y",       flag!(controls.swap_face_buttons), false),
    f(S::Controls, "手柄",     "摇杆死区",        float!(controls.deadzone as f32, bounds::DEADZONE, 0.05, 2), false),
    f(S::Controls, "手柄",     "扳机阈值",      float!(controls.trigger_threshold as f32, bounds::TRIGGER_THRESHOLD, 0.05, 2), false),
    f(S::Controls, "手柄",     "长按时间（毫秒）",      int!(controls.hold_ms as u64, bounds::HOLD_MS, 50), false),
    f(S::Controls, "手柄",     "手柄振动",         flag!(controls.haptics), false),

    f(S::Controls, "光标和滚动", "光标模式",        choice!(controls.cursor_mode: CursorMode), true),
    f(S::Controls, "光标和滚动", "光标速度",       float!(controls.cursor_speed as f32, bounds::CURSOR_SPEED, 50.0, 0), false),
    f(S::Controls, "光标和滚动", "滚动速度",       float!(controls.scroll_speed as f32, bounds::SCROLL_SPEED, 100.0, 0), false),
    f(S::Controls, "光标和滚动", "边缘滚动",     flag!(controls.edge_scroll), false),
    f(S::Controls, "光标和滚动", "链接提示标记",        flag!(controls.hint_badges), false),

    f(S::Controls, "键盘",    "屏幕键盘",     choice!(osk.style: OskStyle), false),
    #[cfg(target_os = "android")]
    f(S::Controls, "键盘",    "系统键盘",        flag!(controls.system_keyboard), false),
    f(S::Controls, "键盘",    "摇杆阈值",        float!(controls.osk_nav_threshold as f32, bounds::OSK_NAV_THRESHOLD, 0.05, 2), false),
    f(S::Controls, "键盘",    "连发延迟（毫秒）",      int!(controls.osk_nav_initial_delay_ms as u64, bounds::OSK_NAV_INITIAL_DELAY_MS, 50), false),
    f(S::Controls, "键盘",    "连发间隔（毫秒）",       int!(controls.osk_nav_repeat_ms as u64, bounds::OSK_NAV_REPEAT_MS, 10), false),

    f(S::Content,  "历史记录",     "记录浏览历史",         flag!(history.enabled), false),
    f(S::Content,  "历史记录",     "记录条数上限",            int!(history.max_entries as usize, bounds::HISTORY_MAX, 5), false),
    f(S::Content,  "广告拦截",  "已启用",                flag!(adblock.enabled), false).quick_as(Modes::Browser, "广告拦截"),
    f(S::Content,  "广告拦截",  "更新间隔（天）",    int!(adblock.update_days as u64, bounds::ADBLOCK_UPDATE_DAYS, 1), false),

    f(S::Content, "省流量", "屏蔽图片",         flag!(data_saving.block_images), false),
    f(S::Content, "省流量", "屏蔽音视频",    flag!(data_saving.block_media), false),
    f(S::Content, "省流量", "屏蔽网页字体",      flag!(data_saving.block_fonts), false),
    f(S::Content, "省流量", "每页图片上限",      int!(data_saving.max_images_per_page as usize, bounds::IMAGES_PER_PAGE, 8, Some("不限")), false),

    f(S::Content, "音频",       "音频输出",         flag!(audio.enabled), true),
    f(S::Content, "音频",       "最长解码时长（秒）",   int!(audio.max_decode_seconds as u32, bounds::DECODE_SECONDS, 30, Some("不限")), true),
    f(S::Content, "视频",       "视频播放",       flag!(video.enabled), true),

    f(S::System,   "性能", "内存配置",          choice!(performance.memory_profile: MemoryProfile), true),
    f(S::System,   "性能", "布局线程数（0 为自动）", int!(performance.layout_threads as u32, bounds::LAYOUT_THREADS, 1), true),
    f(S::System,   "性能", "工作线程上限（0 为自动）", int!(performance.worker_pool_max as u32, bounds::WORKER_POOL_MAX, 1), true),
    f(S::System,   "性能", "加载时提高 CPU 性能",       flag!(performance.cpu_boost_on_load), false),
    f(S::System,   "性能", "交换空间调优",             flag!(performance.swap_tuning), true),
    f(S::System,   "性能", "内存保护",            flag!(performance.memory_guard), false),
    f(S::System,   "性能", "内存保护下限（MB）", int!(performance.memory_guard_floor_mb as u32, bounds::MEMORY_GUARD_FLOOR_MB, 16, Some("自动")), true),
    f(S::System,   "性能", "HTTP 磁盘缓存（MB）",    int!(performance.http_disk_cache_mb as u32, bounds::HTTP_DISK_CACHE_MB, 8, Some("关闭")), true),
    f(S::System,   "性能", "帧率上限（fps）",        int!(performance.max_fps as u32, bounds::MAX_FPS, 5, Some("不限")), false),
    f(S::System,   "显示",     "窗口宽度",           int!(display.width as u32, bounds::WIDTH, 16), true),
    f(S::System,   "显示",     "窗口高度",          int!(display.height as u32, bounds::HEIGHT, 16), true),
    f(S::System,   "显示",     "使用 OpenGL ES",          flag!(display.use_gles), true),
    f(S::System,   "下载",   "保存目录",            text!(downloads.dir), true),
    f(S::System,   "数据",        "清除浏览数据",    Kind::Action { task: Task::ClearData }, false),
    f(S::System,   "更新",     "更新渠道",         choice!(update.channel: Channel), false),
    f(S::System,   "更新",     "启动时自动检查更新",  flag!(update.auto_check), false),
    f(S::System,   "诊断", "显示内存监视",         flag!(debug.memory_overlay), false),
    f(S::System,   "诊断", "记录内存用量",          flag!(debug.memory_log), false),
    #[cfg(target_os = "android")]
    f(S::System,   "诊断", "日志",                   Kind::Action { task: Task::ShareLogs }, false),
    // Last row: it rewrites every other one.
    f(S::System,   "重置",       "恢复所有默认设置",   Kind::Action { task: Task::RestoreDefaults }, true),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A label names one row per tab; a repeat is a row listed twice.
    #[test]
    fn labels_are_unique_within_a_tab() {
        for (i, a) in FIELDS.iter().enumerate() {
            for b in &FIELDS[i + 1..] {
                let same_tab = a.section == b.section;
                assert!(!(same_tab && a.label == b.label), "{}", a.label);
            }
        }
    }

    /// Every accessor pair reads back what it wrote — catches a `get`/`set`
    /// wired to different config spots.
    #[test]
    fn accessors_roundtrip() {
        for field in FIELDS {
            let mut c = AppConfig::default();
            match &field.kind {
                // No accessors to check — these rows hold no config value.
                Kind::Action { .. } | Kind::Door { .. } => {}
                Kind::Bool { get, set } => {
                    for v in [true, false] {
                        set(&mut c, v);
                        assert_eq!(get(&c), v, "{}", field.label);
                    }
                }
                Kind::Text { get, get_mut } => {
                    *get_mut(&mut c) = "roundtrip".to_string();
                    assert_eq!(get(&c), "roundtrip", "{}", field.label);
                }
                Kind::Choice { opts, get, set } => {
                    let opts = opts.list();
                    assert!(!opts.is_empty(), "{}", field.label);
                    for (_, token) in &opts {
                        set(&mut c, token);
                        assert_eq!(get(&c), *token, "{}", field.label);
                    }
                }
                Kind::Int {
                    min, max, get, set, ..
                } => {
                    for v in [*min, *max] {
                        set(&mut c, v);
                        assert_eq!(get(&c), v, "{}", field.label);
                    }
                }
                Kind::Float {
                    min, max, get, set, ..
                } => {
                    for v in [*min, *max] {
                        set(&mut c, v);
                        let got = get(&c);
                        assert!((got - v).abs() < 1e-3, "{}: {got} != {v}", field.label);
                    }
                }
            }
        }
    }
}
