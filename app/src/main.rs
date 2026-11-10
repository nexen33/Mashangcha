#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;
mod state;

use mashangcha_core::crypto::derive_key;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, SharedString};
use state::AppController;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

slint::include_modules!();

/// 注册全局异常崩溃日志钩子：日常运行 0 磁盘写入，仅在未捕获 Panic 时追加记录 `crash.log` 并弹窗提示
fn install_panic_hook(crash_log_path: PathBuf) {
    std::panic::set_hook(Box::new(move |panic_info| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "未知运行时异常".to_string()
        };

        let log_line = format!(
            "[ts={}] PANIC at {}: {}\r\n",
            timestamp, location, payload
        );

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_log_path)
        {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.sync_all();
        }

        platform::show_error_messagebox(
            "码上查 - 运行异常",
            &format!(
                "程序遇到未预期的内部错误，已记录日志至:\n{}\n\n错误摘要: {}",
                crash_log_path.display(),
                payload
            ),
        );
    }));
}

/// 从 Slint 窗口实例提取底层 32 位 / 64 位兼容的 Win32 `HWND`
fn extract_win32_hwnd(window: &MainWindow) -> isize {
    if let Ok(handle) = window.window().window_handle().window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            return win32_handle.hwnd.get() as isize;
        }
    }
    0
}

fn main() {
    // 1. 解析运行模式与双轨硬件指纹，派生 256-bit 对称加密密钥
    let (storage_dir, key_mode, machine_guid, volume_serial) = platform::resolve_storage_context();
    let encryption_key = derive_key(key_mode, machine_guid.as_deref(), volume_serial);

    // 2. 注册零日常 IO 的 Panic 崩溃钩子
    install_panic_hook(storage_dir.join("crash.log"));

    // 3. 创建主窗口并注入 Win7 兼容主中文字体 (Microsoft YaHei 回退 SimSun)
    let window = match MainWindow::new() {
        Ok(w) => w,
        Err(e) => {
            platform::show_error_messagebox(
                "码上查 - 启动失败",
                &format!("无法初始化软件渲染窗口: {}", e),
            );
            return;
        }
    };
    window.set_primary_font_family(SharedString::from(platform::detect_primary_cjk_font()));

    // 4. 初始化响应式状态机并渲染首屏列表
    let controller =
        AppController::new_and_bind(&window, storage_dir, key_mode, encryption_key);

    // 5. 显示窗口并提取底层 Win32 HWND
    if let Err(e) = window.show() {
        platform::show_error_messagebox(
            "码上查 - 窗口显示失败",
            &format!("无法显示应用主窗口: {}", e),
        );
        return;
    }

    let hwnd = extract_win32_hwnd(&window);
    controller.set_hwnd(hwnd);
    controller.apply_initial_window_size();

    // 6. 初始化无边框圆角阴影样式、绑定内嵌多尺寸窗口图标、应用置顶状态并挂载子类化钩子
    platform::setup_frameless_rounded_window(hwnd);
    platform::apply_window_icon(hwnd);
    platform::apply_window_topmost(hwnd, controller.is_topmost());
    platform::install_focus_subclass(hwnd, controller.overlay_open_flag());

    // 7. 回收启动阶段产生的临时内存页，使常驻物理工作集回落至极小基线
    platform::trim_working_set();

    // 8. 启动主线程事件循环
    let _ = window.run();
}

