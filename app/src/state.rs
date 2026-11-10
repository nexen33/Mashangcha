use crate::platform;
use crate::{MainWindow, UiConflict, UiEntry};
use mashangcha_core::crypto::{open_value, seal_value, CryptoError};
use mashangcha_core::export::{export_to_csv_bytes, export_to_xlsx_bytes};
use mashangcha_core::import::{import_from_path, ImportError};
use mashangcha_core::model::{
    AppConfig, Dataset, DuplicateConflict, FontSizeTier, KeyMode, TabCategory,
};
use mashangcha_core::search::search_dataset;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 复制动作同步防抖锁间隔（300ms）
const COPY_DEBOUNCE_MS: u64 = 300;

/// 状态胶囊显示时长（2000ms / 2s）
const TOAST_DURATION_MS: u64 = 2000;

/// 查重错误面板最多展示前 10 条冲突摘要（一键复制按钮仍复制全量冲突明细）
const MAX_DISPLAY_CONFLICTS: usize = 10;

/// 计算指定字号档位下的窗口物理/逻辑尺寸边界 (min_w, max_w, min_h, max_h)
/// 软件自绘阴影单边预留外边距（逻辑像素）
pub const SHADOW_MARGIN: u32 = 6;

/// 最小且默认高度为 3 组信息，最大可拉伸至 10 组信息；宽度最小为默认值，最大可拉伸至 150%（含四周 6px 阴影外边距）
pub fn get_window_size_bounds(font_tier: FontSizeTier) -> (u32, u32, u32, u32) {
    let (base_w, row_h) = match font_tier {
        FontSizeTier::Small => (425, 60),
        FontSizeTier::Medium => (440, 66),
        FontSizeTier::Large => (470, 72),
    };
    let min_content_w = base_w;
    let max_content_w = (base_w as f32 * 1.5).round() as u32;
    let min_content_h = 42 + 54 + (3 * row_h) + 26; // 3 组信息默认/最小内容高度
    let max_content_h = 42 + 54 + (10 * row_h) + 26; // 10 组信息最大拉伸内容高度

    let min_w = min_content_w + 2 * SHADOW_MARGIN;
    let max_w = max_content_w + 2 * SHADOW_MARGIN;
    let min_h = min_content_h + 2 * SHADOW_MARGIN;
    let max_h = max_content_h + 2 * SHADOW_MARGIN;

    (min_w, max_w, min_h, max_h)
}

/// 供 Win32 WM_GETMINMAXINFO 获取当前激活控制器的物理像素边界
pub fn get_active_window_bounds() -> Option<(u32, u32, u32, u32)> {
    ACTIVE_CONTROLLER.with(|slot| {
        let opt = slot.borrow();
        let ctrl = opt.as_ref()?;
        let scale = ctrl.get_scale_factor();
        let (min_w, max_w, min_h, max_h) = get_window_size_bounds(ctrl.config.borrow().font_tier);
        Some((
            (min_w as f32 * scale).round() as u32,
            (max_w as f32 * scale).round() as u32,
            (min_h as f32 * scale).round() as u32,
            (max_h as f32 * scale).round() as u32,
        ))
    })
}

/// 供 Win32 WM_EXITSIZEMOVE 触发尺寸持久化
pub fn on_window_size_changed(hwnd: isize) {
    ACTIVE_CONTROLLER.with(|slot| {
        if let Some(ctrl) = slot.borrow().as_ref() {
            ctrl.handle_window_resized(hwnd);
        }
    });
}

thread_local! {
    static ACTIVE_CONTROLLER: RefCell<Option<Rc<AppController>>> = const { RefCell::new(None) };
}

/// 主线程响应式状态控制器
pub struct AppController {
    storage_dir: PathBuf,
    key_mode: KeyMode,
    encryption_key: [u8; 32],
    hwnd: Cell<isize>,
    jinxiang_data: RefCell<Dataset>,
    xiaoxiang_data: RefCell<Dataset>,
    config: RefCell<AppConfig>,
    last_copy_time: RefCell<Option<Instant>>,
    full_conflict_report: RefCell<String>,
    toast_generation: Cell<u64>,
    io_busy: Arc<AtomicBool>,
    overlay_open_flag: Arc<AtomicBool>,
    weak_window: RefCell<Option<slint::Weak<MainWindow>>>,
}

impl AppController {
    /// 初始化状态控制器、加载并解密本地数据、绑定全部 Slint 界面回调
    pub fn new_and_bind(
        window: &MainWindow,
        storage_dir: PathBuf,
        key_mode: KeyMode,
        encryption_key: [u8; 32],
    ) -> Rc<Self> {
        let mut reset_notice = false;

        let config: AppConfig = Self::load_encrypted_file(
            &storage_dir,
            "config.dat",
            &encryption_key,
            key_mode,
            &mut reset_notice,
        )
        .unwrap_or_default();
        let jinxiang_data: Dataset = Self::load_encrypted_file(
            &storage_dir,
            TabCategory::Jinxiang.storage_filename(),
            &encryption_key,
            key_mode,
            &mut reset_notice,
        )
        .unwrap_or_default();
        let xiaoxiang_data: Dataset = Self::load_encrypted_file(
            &storage_dir,
            TabCategory::Xiaoxiang.storage_filename(),
            &encryption_key,
            key_mode,
            &mut reset_notice,
        )
        .unwrap_or_default();

        let controller = Rc::new(Self {
            storage_dir,
            key_mode,
            encryption_key,
            hwnd: Cell::new(0),
            jinxiang_data: RefCell::new(jinxiang_data),
            xiaoxiang_data: RefCell::new(xiaoxiang_data),
            config: RefCell::new(config.clone()),
            last_copy_time: RefCell::new(None),
            full_conflict_report: RefCell::new(String::new()),
            toast_generation: Cell::new(0),
            io_busy: Arc::new(AtomicBool::new(false)),
            overlay_open_flag: Arc::new(AtomicBool::new(false)),
            weak_window: RefCell::new(Some(window.as_weak())),
        });

        ACTIVE_CONTROLLER.with(|slot| {
            *slot.borrow_mut() = Some(Rc::clone(&controller));
        });

        // 初始化 UI 属性
        window.set_is_portable(key_mode == KeyMode::Portable);
        window.set_default_tab(config.default_tab.as_index());
        window.set_active_tab(config.default_tab.as_index());
        window.set_is_topmost(config.is_topmost);
        window.set_font_tier(config.font_tier.as_index());
        window.set_allow_copy_name(config.allow_copy_name);
        window.set_show_all_enterprises(config.show_all_enterprises);

        if reset_notice {
            controller.show_toast(window, "设备环境变更，原加密缓存已重置，请重新导入");
        }

        // 渲染首屏检索结果
        controller.refresh_search_results(window, "");

        // 绑定全部 UI 回调
        Self::bind_callbacks(&controller, window);

        controller
    }

    /// 注入底层 Win32 窗口句柄
    pub fn set_hwnd(&self, hwnd: isize) {
        self.hwnd.set(hwnd);
    }

    /// 获取当前配置是否开启窗口置顶
    pub fn is_topmost(&self) -> bool {
        self.config.borrow().is_topmost
    }

    /// 获取浮层打开标志原子引用（供 Win32 焦点子类化过程读取）
    pub fn overlay_open_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.overlay_open_flag)
    }

    /// 获取当前窗口的 DPI 缩放比例
    pub fn get_scale_factor(&self) -> f32 {
        if let Some(weak) = self.weak_window.borrow().as_ref() {
            if let Some(win) = weak.upgrade() {
                let scale = win.window().scale_factor();
                if scale > 0.0 {
                    return scale;
                }
            }
        }
        1.0
    }

    /// 应用启动时恢复持久化的自定义窗口尺寸
    pub fn apply_initial_window_size(&self) {
        let hwnd = self.hwnd.get();
        if hwnd == 0 {
            return;
        }
        let cfg = self.config.borrow();
        let (min_w, max_w, min_h, max_h) = get_window_size_bounds(cfg.font_tier);
        let target_w = cfg.custom_width.unwrap_or(min_w).clamp(min_w, max_w);
        let target_h = cfg.custom_height.unwrap_or(min_h).clamp(min_h, max_h);
        drop(cfg);

        let scale = self.get_scale_factor();
        let phys_w = (target_w as f32 * scale).round() as u32;
        let phys_h = (target_h as f32 * scale).round() as u32;
        platform::set_window_size(hwnd, phys_w, phys_h);
    }

    /// 切换字号档位时自适应约束当前窗口尺寸
    pub fn adjust_window_size_for_font_tier(&self, tier: FontSizeTier) {
        let hwnd = self.hwnd.get();
        if hwnd == 0 {
            return;
        }
        let (min_w, max_w, min_h, max_h) = get_window_size_bounds(tier);
        let (phys_w, phys_h) = platform::get_window_size(hwnd);
        let scale = self.get_scale_factor();
        let current_log_w = if phys_w > 0 { (phys_w as f32 / scale).round() as u32 } else { min_w };
        let current_log_h = if phys_h > 0 { (phys_h as f32 / scale).round() as u32 } else { min_h };

        let clamped_w = current_log_w.clamp(min_w, max_w);
        let clamped_h = current_log_h.clamp(min_h, max_h);

        let phys_clamped_w = (clamped_w as f32 * scale).round() as u32;
        let phys_clamped_h = (clamped_h as f32 * scale).round() as u32;
        platform::set_window_size(hwnd, phys_clamped_w, phys_clamped_h);

        let mut cfg = self.config.borrow_mut();
        cfg.custom_width = Some(clamped_w);
        cfg.custom_height = Some(clamped_h);
    }

    /// 窗口缩放释放后持久化当前尺寸
    pub fn handle_window_resized(&self, hwnd: isize) {
        let (phys_w, phys_h) = platform::get_window_size(hwnd);
        if phys_w == 0 || phys_h == 0 {
            return;
        }
        let scale = self.get_scale_factor();
        let log_w = (phys_w as f32 / scale).round() as u32;
        let log_h = (phys_h as f32 / scale).round() as u32;
        let (min_w, max_w, min_h, max_h) = get_window_size_bounds(self.config.borrow().font_tier);
        let clamped_w = log_w.clamp(min_w, max_w);
        let clamped_h = log_h.clamp(min_h, max_h);

        let mut cfg = self.config.borrow_mut();
        if cfg.custom_width != Some(clamped_w) || cfg.custom_height != Some(clamped_h) {
            cfg.custom_width = Some(clamped_w);
            cfg.custom_height = Some(clamped_h);
            drop(cfg);
            let _ = self.save_config();
        }
    }

    /// 读取并解密单个持久化文件；若硬件指纹变更或校验失败，自动备份首个 `.bak` 并优雅降级
    fn load_encrypted_file<T: serde::de::DeserializeOwned>(
        storage_dir: &PathBuf,
        filename: &str,
        key: &[u8; 32],
        expected_mode: KeyMode,
        reset_notice: &mut bool,
    ) -> Option<T> {
        let path = storage_dir.join(filename);
        if !path.exists() {
            return None;
        }

        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => return None,
        };

        match open_value::<T>(&bytes, key, expected_mode) {
            Ok(val) => Some(val),
            Err(
                CryptoError::AuthenticationFailed
                | CryptoError::ModeMismatch { .. }
                | CryptoError::InvalidHeader
                | CryptoError::SerializationError(_),
            ) => {
                let bak_path = storage_dir.join(format!("{}.bak", filename));
                if !bak_path.exists() {
                    let _ = fs::rename(&path, &bak_path);
                } else {
                    let _ = fs::remove_file(&path);
                }
                *reset_notice = true;
                None
            }
            Err(_) => None,
        }
    }

    /// 加密并原子持久化当前配置
    fn save_config(&self) -> Result<(), String> {
        let cfg = self.config.borrow().clone();
        let sealed = seal_value(&cfg, &self.encryption_key, self.key_mode)
            .map_err(|e| format!("加密配置失败: {:?}", e))?;
        let target = self.storage_dir.join("config.dat");
        platform::atomic_write_file(&target, &sealed)
    }

    /// 根据当前选中的分类和搜索词执行 `< 1ms` 同步过滤并刷新 Slint 列表视图
    pub fn refresh_search_results(&self, window: &MainWindow, query: &str) {
        self.overlay_open_flag.store(
            window.get_show_settings_menu() || window.get_show_error_overlay(),
            Ordering::Relaxed,
        );
        let category = TabCategory::from_index(window.get_active_tab());
        let dataset_ref = match category {
            TabCategory::Jinxiang => self.jinxiang_data.borrow(),
            TabCategory::Xiaoxiang => self.xiaoxiang_data.borrow(),
        };

        let is_query_empty = query.trim().is_empty();
        let show_all = self.config.borrow().show_all_enterprises;

        let (ui_items, matched_count) = if is_query_empty && !show_all {
            (Vec::new(), 0)
        } else {
            let search_res = search_dataset(&dataset_ref, query);
            let items: Vec<UiEntry> = search_res
                .items
                .into_iter()
                .map(|entry| UiEntry {
                    name: SharedString::from(entry.name.as_str()),
                    code: SharedString::from(entry.code.as_str()),
                    tax_id: SharedString::from(entry.tax_id.as_str()),
                })
                .collect();
            (items, search_res.total_matched)
        };

        window.set_total_dataset_count(dataset_ref.entries.len() as i32);
        window.set_total_matched(matched_count as i32);
        window.set_results(ModelRc::from(Rc::new(VecModel::from(ui_items))));
    }

    /// 显示顶栏轻量状态胶囊并在 1200ms 后自动消隐（带代数校验防止覆盖新消息）
    pub fn show_toast(&self, window: &MainWindow, message: &str) {
        let next_gen = self.toast_generation.get().wrapping_add(1);
        self.toast_generation.set(next_gen);
        window.set_status_toast(SharedString::from(message));

        let weak_win = window.as_weak();
        Timer::single_shot(Duration::from_millis(TOAST_DURATION_MS), move || {
            ACTIVE_CONTROLLER.with(|slot| {
                if let Some(ctrl) = slot.borrow().as_ref() {
                    if ctrl.toast_generation.get() == next_gen {
                        if let Some(win) = weak_win.upgrade() {
                            win.set_status_toast(SharedString::default());
                        }
                    }
                }
            });
        });
    }

    /// 执行带 300ms 防抖锁的剪贴板复制动作
    pub fn perform_copy(&self, window: &MainWindow, field_label: &str, value: &str) {
        if value.is_empty() {
            return;
        }

        let now = Instant::now();
        {
            let mut last_ref = self.last_copy_time.borrow_mut();
            if let Some(prev) = *last_ref {
                if now.duration_since(prev) < Duration::from_millis(COPY_DEBOUNCE_MS) {
                    return;
                }
            }
            *last_ref = Some(now);
        }

        match platform::set_clipboard_text_with_retry(value) {
            Ok(()) => {
                let msg = format!("已复制{}: {}", field_label, value);
                self.show_toast(window, &msg);
            }
            Err(err_msg) => {
                self.show_toast(window, &err_msg);
            }
        }
    }

    /// 格式化单条查重冲突信息
    fn format_conflict_line(c: &DuplicateConflict) -> String {
        format!(
            "第 {} 行 与 第 {} 行 [{}] 重复: \"{}\"",
            c.duplicate_row,
            c.first_row,
            c.field.label(),
            c.conflicting_value
        )
    }

    /// 处理后台导入结果并更新 UI 状态
    fn apply_import_outcome(
        &self,
        window: &MainWindow,
        category: TabCategory,
        outcome: Result<Dataset, ImportError>,
    ) {
        match outcome {
            Ok(dataset) => {
                let count = dataset.entries.len();
                match category {
                    TabCategory::Jinxiang => *self.jinxiang_data.borrow_mut() = dataset,
                    TabCategory::Xiaoxiang => *self.xiaoxiang_data.borrow_mut() = dataset,
                }
                window.set_show_error_overlay(false);
                self.overlay_open_flag.store(false, Ordering::Relaxed);
                let query = window.get_search_text().to_string();
                self.refresh_search_results(window, &query);
                self.show_toast(
                    window,
                    &format!("{}导入成功 ({} 条)", category.display_name(), count),
                );
            }
            Err(ImportError::DuplicateRecords {
                total_conflicts,
                conflicts,
            }) => {
                let all_lines: Vec<String> =
                    conflicts.iter().map(Self::format_conflict_line).collect();
                *self.full_conflict_report.borrow_mut() = all_lines.join("\r\n");

                let preview_items: Vec<UiConflict> = all_lines
                    .into_iter()
                    .take(MAX_DISPLAY_CONFLICTS)
                    .map(|line| UiConflict {
                        text: SharedString::from(line),
                    })
                    .collect();

                window.set_error_headline(SharedString::from(format!(
                    "导入已中止：发现 {} 处重复记录（请去重后重新导入）",
                    total_conflicts
                )));
                window.set_conflict_total_count(total_conflicts as i32);
                window.set_conflict_items(ModelRc::from(Rc::new(VecModel::from(preview_items))));
                window.set_show_error_overlay(true);
                self.overlay_open_flag.store(true, Ordering::Relaxed);
                window.set_status_toast(SharedString::default());
            }
            Err(other_err) => {
                let (headline, detail) = match other_err {
                    ImportError::IoError(msg) => ("文件读写失败".to_string(), msg),
                    ImportError::ParseError(msg) => ("表格格式解析失败".to_string(), msg),
                    ImportError::EmptyDataset => (
                        "导入失败：表格无有效数据".to_string(),
                        "所选文件中未检测到任何非空企业记录，请检查表格内容。".to_string(),
                    ),
                    ImportError::TooManyRows { max_allowed } => (
                        "导入失败：记录数超出上限".to_string(),
                        format!("单分类最多支持导入 {} 条非空记录。", max_allowed),
                    ),
                    ImportError::ScientificNotationDetected { row, raw_value } => (
                        "导入已拦截：检测到科学计数法损坏".to_string(),
                        format!(
                            "第 {} 行编码或税号 ({}) 已被 Excel 转为科学计数法丢失精度，请将该列设为文本格式后重试。",
                            row, raw_value
                        ),
                    ),
                    ImportError::MissingRequiredField { row, field_name } => (
                        "导入已中止：存在必填项为空".to_string(),
                        format!("第 {} 行的 [{}] 为空，请补全后重新导入。", row, field_name),
                    ),
                    ImportError::DuplicateRecords { .. } => unreachable!(),
                };

                *self.full_conflict_report.borrow_mut() = detail.clone();
                let items = vec![UiConflict {
                    text: SharedString::from(detail),
                }];
                window.set_error_headline(SharedString::from(headline));
                window.set_conflict_total_count(1);
                window.set_conflict_items(ModelRc::from(Rc::new(VecModel::from(items))));
                window.set_show_error_overlay(true);
                self.overlay_open_flag.store(true, Ordering::Relaxed);
                window.set_status_toast(SharedString::default());
            }
        }
    }

    /// 触发当前分类导出备份（`.xlsx` 或 `.csv`）
    fn trigger_export(&self, window: &MainWindow, is_xlsx: bool) {
        if self
            .io_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let category = TabCategory::from_index(window.get_active_tab());
        let dataset_snapshot = match category {
            TabCategory::Jinxiang => self.jinxiang_data.borrow().clone(),
            TabCategory::Xiaoxiang => self.xiaoxiang_data.borrow().clone(),
        };

        if dataset_snapshot.entries.is_empty() {
            self.io_busy.store(false, Ordering::SeqCst);
            self.show_toast(window, "当前分类暂无数据可导出");
            return;
        }

        let date_suffix = platform::get_local_date_yyyymmdd();
        let default_name = if is_xlsx {
            format!("码上查_{}备份_{}.xlsx", category.display_name(), date_suffix)
        } else {
            format!("码上查_{}备份_{}.csv", category.display_name(), date_suffix)
        };

        let Some(target_path) = platform::pick_export_file(self.hwnd.get(), &default_name, is_xlsx)
        else {
            self.io_busy.store(false, Ordering::SeqCst);
            return;
        };

        window.set_status_toast(SharedString::from("正在导出备份文件..."));
        let io_busy_clone = Arc::clone(&self.io_busy);
        let weak_win = window.as_weak();

        thread::spawn(move || {
            let bytes_res = if is_xlsx {
                export_to_xlsx_bytes(&dataset_snapshot)
            } else {
                export_to_csv_bytes(&dataset_snapshot)
            };

            let write_res =
                bytes_res.and_then(|bytes| platform::atomic_write_file(&target_path, &bytes));

            io_busy_clone.store(false, Ordering::SeqCst);
            platform::trim_working_set();

            let _ = slint::invoke_from_event_loop(move || {
                ACTIVE_CONTROLLER.with(|slot| {
                    if let (Some(ctrl), Some(win)) = (slot.borrow().as_ref(), weak_win.upgrade()) {
                        match write_res {
                            Ok(()) => ctrl.show_toast(&win, "备份导出成功"),
                            Err(err) => ctrl.show_toast(&win, &format!("导出失败: {}", err)),
                        }
                    }
                });
            });
        });
    }

    /// 启动指定表格文件路径的后台校验与全量替换导入（供文件选择器与原生拖拽共用）
    pub fn run_import_from_path(&self, window: &MainWindow, path: PathBuf) {
        let category = TabCategory::from_index(window.get_active_tab());
        window.set_show_settings_menu(false);
        self.overlay_open_flag
            .store(window.get_show_error_overlay(), Ordering::Relaxed);
        window.set_status_toast(SharedString::from("正在校验并导入台账..."));

        let storage_dir = self.storage_dir.clone();
        let key = self.encryption_key;
        let mode = self.key_mode;
        let io_busy_clone = Arc::clone(&self.io_busy);
        let weak_win_thread = window.as_weak();

        thread::spawn(move || {
            let now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let outcome = match import_from_path(&path, now_secs) {
                Ok((dataset, _mapping)) => match seal_value(&dataset, &key, mode) {
                    Ok(sealed) => {
                        let target_path = storage_dir.join(category.storage_filename());
                        match platform::atomic_write_file(&target_path, &sealed) {
                            Ok(()) => Ok(dataset),
                            Err(e) => Err(ImportError::IoError(e)),
                        }
                    }
                    Err(e) => Err(ImportError::IoError(format!("加密失败: {:?}", e))),
                },
                Err(err) => Err(err),
            };

            io_busy_clone.store(false, Ordering::SeqCst);
            platform::trim_working_set();

            let _ = slint::invoke_from_event_loop(move || {
                ACTIVE_CONTROLLER.with(|slot| {
                    if let (Some(main_ctrl), Some(w)) =
                        (slot.borrow().as_ref(), weak_win_thread.upgrade())
                    {
                        main_ctrl.apply_import_outcome(&w, category, outcome);
                    }
                });
            });
        });
    }

    /// 处理外部拖入窗口的表格文件（当设置面板开启时执行一步导入）
    pub fn handle_dropped_file(window: &MainWindow, path: PathBuf) {
        if !window.get_show_settings_menu() {
            return;
        }
        ACTIVE_CONTROLLER.with(|slot| {
            if let Some(ctrl) = slot.borrow().as_ref() {
                if ctrl
                    .io_busy
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    ctrl.run_import_from_path(window, path);
                }
            }
        });
    }

    /// 接收 Win32 原生 WM_DROPFILES 事件触发拖拽导入（主线程无锁直接分发）
    pub fn on_native_file_dropped(path: PathBuf) {
        ACTIVE_CONTROLLER.with(|slot| {
            if let Some(ctrl) = slot.borrow().as_ref() {
                if let Some(weak_w) = ctrl.weak_window.borrow().as_ref() {
                    if let Some(w) = weak_w.upgrade() {
                        Self::handle_dropped_file(&w, path);
                    }
                }
            }
        });
    }

    /// 绑定全部 Slint 界面事件回调
    fn bind_callbacks(this: &Rc<Self>, window: &MainWindow) {
        // 1. 切换进项 / 销项分类
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_tab_switched(move |_tab_idx| {
                if let Some(win) = weak_win.upgrade() {
                    let query = win.get_search_text().to_string();
                    ctrl.refresh_search_results(&win, &query);
                }
            });
        }

        // 2. 切换置顶状态并持久化
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_toggle_topmost(move || {
                if let Some(win) = weak_win.upgrade() {
                    let new_topmost = !ctrl.config.borrow().is_topmost;
                    ctrl.config.borrow_mut().is_topmost = new_topmost;
                    win.set_is_topmost(new_topmost);
                    platform::apply_window_topmost(ctrl.hwnd.get(), new_topmost);
                    let _ = ctrl.save_config();
                    ctrl.show_toast(
                        &win,
                        if new_topmost {
                            "已开启窗口置顶"
                        } else {
                            "已取消窗口置顶"
                        },
                    );
                }
            });
        }

        // 3. 切换三档字号并持久化
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_change_font_tier(move |tier_idx| {
                if let Some(win) = weak_win.upgrade() {
                    let tier = FontSizeTier::from_index(tier_idx);
                    ctrl.config.borrow_mut().font_tier = tier;
                    win.set_font_tier(tier.as_index());
                    ctrl.adjust_window_size_for_font_tier(tier);
                    let _ = ctrl.save_config();
                }
            });
        }

        // 4. 切换默认启动分类并持久化
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_change_default_tab(move |tab_idx| {
                if let Some(win) = weak_win.upgrade() {
                    let cat = TabCategory::from_index(tab_idx);
                    ctrl.config.borrow_mut().default_tab = cat;
                    win.set_default_tab(cat.as_index());
                    let _ = ctrl.save_config();
                }
            });
        }

        // 5. 切换“企业名称复制”开关并持久化
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_change_allow_copy_name(move |allowed| {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.config.borrow_mut().allow_copy_name = allowed;
                    win.set_allow_copy_name(allowed);
                    let _ = ctrl.save_config();
                }
            });
        }

        // 5.1 切换“展示所有企业”开关并持久化，立即刷新列表视图
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_change_show_all_enterprises(move |show_all| {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.config.borrow_mut().show_all_enterprises = show_all;
                    win.set_show_all_enterprises(show_all);
                    let _ = ctrl.save_config();
                    let query = win.get_search_text().to_string();
                    ctrl.refresh_search_results(&win, &query);
                }
            });
        }

        // 6. 极速同步二级浮层焦点拦截标志 (O(1) 零重搜开销)
        {
            let ctrl = Rc::clone(this);
            window.on_overlay_toggled(move |is_open| {
                ctrl.overlay_open_flag.store(is_open, Ordering::Relaxed);
            });
        }

        // 7. 无边框窗口拖拽、最小化与关闭
        {
            let ctrl = Rc::clone(this);
            window.on_start_window_drag(move || {
                platform::start_window_drag(ctrl.hwnd.get());
            });
        }
        {
            let ctrl = Rc::clone(this);
            window.on_minimize_window(move || {
                platform::minimize_window(ctrl.hwnd.get());
            });
        }
        {
            let ctrl = Rc::clone(this);
            window.on_close_window(move || {
                platform::close_window(ctrl.hwnd.get());
            });
        }

        // 8. 搜索词实时编辑过滤 (< 1ms 零防抖)
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_search_edited(move |text| {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.refresh_search_results(&win, text.as_str());
                }
            });
        }

        // 9. 点击透明遮罩直接复制对应字段 (名称 / 编号 / 税号)
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_copy_field(move |field_label, value| {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.perform_copy(&win, field_label.as_str(), value.as_str());
                }
            });
        }

        // 10. 一键复制全部查重冲突明细
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_copy_all_conflicts(move || {
                if let Some(win) = weak_win.upgrade() {
                    let report = ctrl.full_conflict_report.borrow().clone();
                    if !report.is_empty() {
                        match platform::set_clipboard_text_with_retry(&report) {
                            Ok(()) => ctrl.show_toast(&win, "已复制全部冲突明细"),
                            Err(e) => ctrl.show_toast(&win, &e),
                        }
                    }
                }
            });
        }

        // 11. 触发后台全量替换导入 (含前置严格查重与原子加密落盘)
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_trigger_import(move || {
                if ctrl
                    .io_busy
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    return;
                }

                let Some(path) = platform::pick_import_file(ctrl.hwnd.get()) else {
                    ctrl.io_busy.store(false, Ordering::SeqCst);
                    return;
                };

                let Some(win) = weak_win.upgrade() else {
                    ctrl.io_busy.store(false, Ordering::SeqCst);
                    return;
                };

                ctrl.run_import_from_path(&win, path);
            });
        }

        // 12. 触发导出备份为 Excel (.xlsx)
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_trigger_export_xlsx(move || {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.trigger_export(&win, true);
                }
            });
        }

        // 13. 触发导出备份为 CSV (UTF-8 BOM)
        {
            let ctrl = Rc::clone(this);
            let weak_win = window.as_weak();
            window.on_trigger_export_csv(move || {
                if let Some(win) = weak_win.upgrade() {
                    ctrl.trigger_export(&win, false);
                }
            });
        }
    }
}
