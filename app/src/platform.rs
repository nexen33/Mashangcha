use mashangcha_core::model::KeyMode;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::mem::{size_of, transmute, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{copy_nonoverlapping, null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    GetLastError, GlobalFree, ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, FALSE, HANDLE, HWND,
    LPARAM, LRESULT, MAX_PATH, POINT, RECT, SYSTEMTIME, TRUE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::Storage::FileSystem::{
    GetVolumeInformationW, MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    KEY_WOW64_64KEY, REG_SZ,
};
use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR,
    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
#[cfg(target_pointer_width = "64")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetMessagePos, GetWindowLongW, GetWindowRect, LoadImageW, MessageBoxW, PostMessageW,
    SendMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_WNDPROC, GWL_EXSTYLE,
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, HWND_NOTOPMOST, HWND_TOPMOST, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED,
    MA_NOACTIVATE, MB_ICONERROR, MB_OK, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_MINIMIZE, WM_CLOSE, WM_DROPFILES, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_SETICON, WNDPROC,
    WS_EX_TOPMOST,
};
#[cfg(target_pointer_width = "32")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetMessagePos, GetWindowLongW, GetWindowRect, LoadImageW, MessageBoxW, PostMessageW,
    SendMessageW, SetWindowLongW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_WNDPROC,
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, HWND_NOTOPMOST, HWND_TOPMOST, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED,
    MA_NOACTIVATE, MB_ICONERROR, MB_OK, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_MINIMIZE, WM_CLOSE, WM_DROPFILES, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_SETICON, WNDPROC,
    WS_EX_TOPMOST,
};

/// Win32 UTF-16LE 剪贴板标准格式常量
const CF_UNICODETEXT: u32 = 13;

/// 将 Rust `&str` 转换为以 `\0` 结尾的 UTF-16LE 宽字符向量
fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 将 Rust `&Path` 转换为以 `\0` 结尾的 UTF-16LE 宽字符向量
fn path_to_wide_null(p: &Path) -> Vec<u16> {
    p.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 探测系统字体目录是否存在微软雅黑（`msyh.ttc` 或 `msyh.ttf`），缺失时自动回退至中易宋体（`SimSun`）
pub fn detect_primary_cjk_font() -> &'static str {
    let windir = env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windir.join("Fonts");
    if fonts_dir.join("msyh.ttc").exists() || fonts_dir.join("msyh.ttf").exists() {
        "Microsoft YaHei"
    } else {
        "SimSun"
    }
}

/// 提取指定盘符根路径（如 `"C:\\"` 或 `"E:\\"`）的 32 位硬件卷序列号
pub fn get_volume_serial(drive_root: &str) -> u32 {
    let root_w = to_wide_null(drive_root);
    let mut serial: u32 = 0;
    unsafe {
        let ok = GetVolumeInformationW(
            root_w.as_ptr(),
            null_mut(),
            0,
            &mut serial,
            null_mut(),
            null_mut(),
            null_mut(),
            0,
        );
        if ok == FALSE {
            return 0;
        }
    }
    serial
}

/// 获取系统 Windows 所在盘符根路径（如 `"C:\\"`）
fn get_system_drive_root() -> String {
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), MAX_PATH) } as usize;
    if len >= 3 {
        let s = String::from_utf16_lossy(&buf[..3]);
        if s.as_bytes().get(1) == Some(&b':') {
            return s;
        }
    }
    r"C:\".to_string()
}

/// 从任意路径提取盘符根路径（如 `d:\Mashangcha` -> `"d:\\"`）
fn extract_drive_root(path: &Path) -> String {
    let s = path.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        format!("{}:\\", bytes[0] as char)
    } else {
        get_system_drive_root()
    }
}

/// 穿透 WOW64 64 位注册表视图读取 `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`
pub fn read_machine_guid() -> Option<String> {
    let subkey = to_wide_null(r"SOFTWARE\Microsoft\Cryptography");
    let value_name = to_wide_null("MachineGuid");
    let mut hkey: HKEY = 0;

    unsafe {
        let status = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        );
        if status != 0 {
            return None;
        }

        let mut value_type: u32 = 0;
        let mut data_buf = [0u16; 128];
        let mut byte_len: u32 = (data_buf.len() * size_of::<u16>()) as u32;

        let query_status = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            null(),
            &mut value_type,
            data_buf.as_mut_ptr() as *mut u8,
            &mut byte_len,
        );
        RegCloseKey(hkey);

        if query_status != 0 || value_type != REG_SZ {
            return None;
        }

        let u16_len = (byte_len as usize) / size_of::<u16>();
        let slice = &data_buf[..u16_len];
        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let guid = String::from_utf16_lossy(&slice[..end]).trim().to_string();
        if guid.is_empty() {
            None
        } else {
            Some(guid)
        }
    }
}

/// 判定当前运行模式并采集对应双轨硬件指纹：
/// 返回 `(数据存储目录, 运行模式, MachineGuid, 硬件卷序列号)`
pub fn resolve_storage_context() -> (PathBuf, KeyMode, Option<String>, u32) {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    if exe_dir.join("portable.flag").exists() {
        let drive_root = extract_drive_root(&exe_dir);
        let serial = get_volume_serial(&drive_root);
        return (exe_dir, KeyMode::Portable, None, serial);
    }

    let appdata_dir = env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| exe_dir.clone())
        .join("Mashangcha");

    let _ = fs::create_dir_all(&appdata_dir);

    let machine_guid = read_machine_guid();
    let sys_drive = get_system_drive_root();
    let serial = get_volume_serial(&sys_drive);

    (appdata_dir, KeyMode::Default, machine_guid, serial)
}

/// 防掉电且抗杀毒软件锁的原子文件落盘：
/// 1. 写入同级 `<filename>.tmp`
/// 2. `sync_all()` 刷入物理介质
/// 3. 调用 `MoveFileExW` 原子替换目标文件（遇错误码 5 或 32 时执行 5 次指数退避重试）
pub fn atomic_write_file(target_path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut tmp_os = target_path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);

    {
        let mut file =
            File::create(&tmp_path).map_err(|e| format!("无法创建临时文件 {:?}: {}", tmp_path, e))?;
        file.write_all(data)
            .map_err(|e| format!("写入临时文件失败: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("刷盘校验(sync_all)失败: {}", e))?;
    }

    let tmp_w = path_to_wide_null(&tmp_path);
    let target_w = path_to_wide_null(target_path);
    let backoff_ms = [20u64, 50, 100, 200, 400];

    for (attempt, &wait_ms) in backoff_ms.iter().enumerate() {
        let ok = unsafe {
            MoveFileExW(
                tmp_w.as_ptr(),
                target_w.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok != FALSE {
            return Ok(());
        }

        let err_code = unsafe { GetLastError() };
        if (err_code == ERROR_ACCESS_DENIED || err_code == ERROR_SHARING_VIOLATION)
            && attempt + 1 < backoff_ms.len()
        {
            thread::sleep(Duration::from_millis(wait_ms));
            continue;
        }

        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "原子替换文件失败 (Win32 Error {})",
            err_code
        ));
    }

    let _ = fs::remove_file(&tmp_path);
    Err("原子替换文件在 5 次退避重试后仍被占用".to_string())
}

/// 将纯文本写入 Windows 系统剪贴板（带 5 次退避重试与完整的 GlobalLock/GlobalUnlock/GlobalFree 生命周期闭环）
pub fn set_clipboard_text_with_retry(text: &str) -> Result<(), String> {
    let wide = to_wide_null(text);
    let byte_len = wide.len() * size_of::<u16>();
    let retry_delays_ms = [5u64, 10, 20, 30, 50];

    let mut opened = false;
    for &delay in &retry_delays_ms {
        let ok = unsafe { OpenClipboard(0) };
        if ok != FALSE {
            opened = true;
            break;
        }
        thread::sleep(Duration::from_millis(delay));
    }

    if !opened {
        let ok = unsafe { OpenClipboard(0) };
        if ok == FALSE {
            return Err("无法打开系统剪贴板（可能被其他程序锁定）".to_string());
        }
    }

    unsafe {
        EmptyClipboard();

        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if hmem.is_null() {
            CloseClipboard();
            return Err("分配剪贴板全局内存失败".to_string());
        }

        let locked_ptr = GlobalLock(hmem) as *mut u16;
        if locked_ptr.is_null() {
            GlobalFree(hmem);
            CloseClipboard();
            return Err("锁定剪贴板全局内存失败".to_string());
        }

        copy_nonoverlapping(wide.as_ptr(), locked_ptr, wide.len());
        GlobalUnlock(hmem);

        let handle = SetClipboardData(CF_UNICODETEXT, hmem as HANDLE);
        if handle == 0 {
            GlobalFree(hmem);
            CloseClipboard();
            return Err("写入剪贴板数据失败".to_string());
        }

        CloseClipboard();
    }

    Ok(())
}

/// 弹出 Win32 原生打开文件对话框（用于导入 `.xlsx`/`.xls`/`.csv`）
pub fn pick_import_file(hwnd: isize) -> Option<PathBuf> {
    let filter: Vec<u16> = "表格文件 (*.xlsx;*.xls;*.csv)\0*.xlsx;*.xls;*.csv\0所有文件 (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let title = to_wide_null("选择要导入的企业台账表格 (全量替换当前分类)");
    let mut file_buf = vec![0u16; 1024];

    unsafe {
        let mut ofn: OPENFILENAMEW = zeroed();
        ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd as HWND;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = file_buf.as_mut_ptr();
        ofn.nMaxFile = file_buf.len() as u32;
        ofn.lpstrTitle = title.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;

        if GetOpenFileNameW(&mut ofn) != FALSE {
            let end = file_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_buf.len());
            if end > 0 {
                return Some(PathBuf::from(OsString::from_wide(&file_buf[..end])));
            }
        }
    }
    None
}

/// 弹出 Win32 原生保存文件对话框（用于导出 `.xlsx` 或 `.csv`）
pub fn pick_export_file(hwnd: isize, default_name: &str, is_xlsx: bool) -> Option<PathBuf> {
    let filter_str = if is_xlsx {
        "Excel 工作簿 (*.xlsx)\0*.xlsx\0\0"
    } else {
        "CSV UTF-8 表格 (*.csv)\0*.csv\0\0"
    };
    let filter: Vec<u16> = filter_str.encode_utf16().collect();
    let def_ext = to_wide_null(if is_xlsx { "xlsx" } else { "csv" });
    let title = to_wide_null("导出当前分类企业台账备份");

    let mut file_buf = vec![0u16; 1024];
    for (i, code_unit) in default_name.encode_utf16().enumerate() {
        if i + 1 < file_buf.len() {
            file_buf[i] = code_unit;
        }
    }

    unsafe {
        let mut ofn: OPENFILENAMEW = zeroed();
        ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = hwnd as HWND;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = file_buf.as_mut_ptr();
        ofn.nMaxFile = file_buf.len() as u32;
        ofn.lpstrDefExt = def_ext.as_ptr();
        ofn.lpstrTitle = title.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;

        if GetSaveFileNameW(&mut ofn) != FALSE {
            let end = file_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(file_buf.len());
            if end > 0 {
                return Some(PathBuf::from(OsString::from_wide(&file_buf[..end])));
            }
        }
    }
    None
}

/// 动态设置或取消窗口强置顶（`HWND_TOPMOST` / `HWND_NOTOPMOST`）
pub fn apply_window_topmost(hwnd: isize, is_topmost: bool) {
    if hwnd == 0 {
        return;
    }
    let insert_after = if is_topmost {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// 从当前 `.exe` 内嵌的 PE 资源表 (Resource ID = 1) 加载多尺寸图标并绑定至窗口标题栏与任务栏
pub fn apply_window_icon(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let hinstance = GetModuleHandleW(null());
        let icon_small = LoadImageW(
            hinstance,
            1usize as *const u16,
            IMAGE_ICON,
            16,
            16,
            LR_SHARED,
        );
        if icon_small != 0 {
            SendMessageW(
                hwnd as HWND,
                WM_SETICON,
                ICON_SMALL as WPARAM,
                icon_small as LPARAM,
            );
        }

        let icon_big = LoadImageW(
            hinstance,
            1usize as *const u16,
            IMAGE_ICON,
            32,
            32,
            LR_SHARED,
        );
        if icon_big != 0 {
            SendMessageW(
                hwnd as HWND,
                WM_SETICON,
                ICON_BIG as WPARAM,
                icon_big as LPARAM,
            );
        }
    }
}

/// 初始化无边框 Widget 窗口的 DWM 阴影、圆角与文件拖拽支持
/// （保持 winit 原生 `no-frame: true` 的 `WM_NCCALCSIZE` 拦截机制，绝不调用 `SetWindowRgn` 以免触发 Win95 复古标题栏回退）
pub fn setup_frameless_rounded_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        // 1. 开启 SHELL32 原生文件拖拽接收
        DragAcceptFiles(hwnd as HWND, TRUE);

        // 2. 动态调用 dwmapi.dll 开启 Win11 原生圆角（保留 winit 默认 DwmEnableBlurBehindWindow 透明合成，不调用 DwmExtendFrameIntoClientArea 以免覆盖透明通道）
        let dwm_dll = to_wide_null("dwmapi.dll");
        let hmod = LoadLibraryW(dwm_dll.as_ptr());
        if hmod != 0 {
            if let Some(proc_addr) = GetProcAddress(hmod, b"DwmSetWindowAttribute\0".as_ptr()) {
                type DwmSetWindowAttributeFn =
                    unsafe extern "system" fn(HWND, u32, *const u32, u32) -> i32;
                let dwm_set_attr: DwmSetWindowAttributeFn = transmute(proc_addr);
                let pref_round: u32 = 2; // DWMWCP_ROUND
                let _ = dwm_set_attr(hwnd as HWND, 33, &pref_round, size_of::<u32>() as u32);
            }
        }
    }
}

/// 触发内核级零延迟无边框窗口拖拽
pub fn start_window_drag(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd as HWND, WM_LBUTTONUP, 0, 0);
        SendMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
    }
}

/// 将无边框窗口最小化至任务栏
pub fn minimize_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        ShowWindow(hwnd as HWND, SW_MINIMIZE);
    }
}

/// 关闭窗口并退出进程事件循环
pub fn close_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
    }
}

/// 获取指定 Win32 窗口的物理宽高
pub fn get_window_size(hwnd: isize) -> (u32, u32) {
    if hwnd == 0 {
        return (0, 0);
    }
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        if GetWindowRect(hwnd as HWND, &mut rect) != 0 {
            let w = (rect.right - rect.left).max(0) as u32;
            let h = (rect.bottom - rect.top).max(0) as u32;
            return (w, h);
        }
    }
    (0, 0)
}

/// 设置指定 Win32 窗口的物理宽高
pub fn set_window_size(hwnd: isize, width: u32, height: u32) {
    if hwnd == 0 || width == 0 || height == 0 {
        return;
    }
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            0,
            0,
            0,
            width as i32,
            height as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// 窗口子类化全局上下文（纯原子状态，无任何 Mutex 锁）
struct SubclassContext {
    prev_wndproc: AtomicIsize,
    overlay_open_flag: Arc<AtomicBool>,
}

static SUBCLASS_CTX: OnceLock<SubclassContext> = OnceLock::new();

/// 32/64 位兼容的自定义窗口过程：
/// 1. 置顶列表点击免抢焦点（`MA_NOACTIVATE`）
/// 2. 拦截 `WM_DROPFILES` 实现拖拽文件一步导入
unsafe extern "system" fn custom_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let Some(ctx) = SUBCLASS_CTX.get() else {
        return 0;
    };
    let prev_ptr = ctx.prev_wndproc.load(Ordering::Relaxed);
    if prev_ptr == 0 {
        return 0;
    }
    let prev_fn: WNDPROC = transmute(prev_ptr);

    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let is_topmost = (ex_style & WS_EX_TOPMOST) != 0;

    // 1. 无边框窗口原生八向与四角缩放命中测试 (WM_NCHITTEST)
    if msg == WM_NCHITTEST {
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if GetWindowRect(hwnd, &mut rect) != 0 {
            let pos_packed = GetMessagePos();
            let x_screen = (pos_packed & 0xFFFF) as i16 as i32;
            let y_screen = ((pos_packed >> 16) & 0xFFFF) as i16 as i32;

            let margin = 10;
            let on_left = x_screen >= rect.left && x_screen < rect.left + margin;
            let on_right = x_screen < rect.right && x_screen >= rect.right - margin;
            let on_top = y_screen >= rect.top && y_screen < rect.top + margin;
            let on_bottom = y_screen < rect.bottom && y_screen >= rect.bottom - margin;

            if on_top && on_left { return HTTOPLEFT as LRESULT; }
            if on_top && on_right { return HTTOPRIGHT as LRESULT; }
            if on_bottom && on_left { return HTBOTTOMLEFT as LRESULT; }
            if on_bottom && on_right { return HTBOTTOMRIGHT as LRESULT; }
            if on_left { return HTLEFT as LRESULT; }
            if on_right { return HTRIGHT as LRESULT; }
            if on_top { return HTTOP as LRESULT; }
            if on_bottom { return HTBOTTOM as LRESULT; }
        }
    }

    // 2. 限制窗口拖拽缩放物理边界 (WM_GETMINMAXINFO)
    if msg == WM_GETMINMAXINFO {
        let info = lparam as *mut MINMAXINFO;
        if !info.is_null() {
            if let Some((min_w, max_w, min_h, max_h)) = crate::state::get_active_window_bounds() {
                (*info).ptMinTrackSize.x = min_w as i32;
                (*info).ptMinTrackSize.y = min_h as i32;
                (*info).ptMaxTrackSize.x = max_w as i32;
                (*info).ptMaxTrackSize.y = max_h as i32;
                return 0;
            }
        }
    }

    // 3. 拖拽缩放/移动释放后持久化自定义尺寸 (WM_EXITSIZEMOVE)
    if msg == WM_EXITSIZEMOVE {
        crate::state::on_window_size_changed(hwnd as isize);
    }

    // 4. 拦截背景擦除以减震防闪烁 (WM_ERASEBKGND)
    // 由 Slint 双缓冲层独占绘制，阻止 Win32 默认画刷在左上拉伸窗口重调大小时擦除客户区导致的白屏闪频
    if msg == WM_ERASEBKGND {
        return 1;
    }

    if msg == WM_MOUSEACTIVATE {
        if is_topmost {
            let pos_packed = GetMessagePos();
            let y_screen = ((pos_packed >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT {
                x: 0,
                y: y_screen,
            };
            ScreenToClient(hwnd, &mut pt);

            // 点击 5 行结果列表区域 (y >= 96) 且未打开二级浮层：直接透传鼠标点击执行复制，不抢外部软件焦点
            if pt.y >= 96 && !ctx.overlay_open_flag.load(Ordering::Relaxed) {
                return MA_NOACTIVATE as LRESULT;
            }
        }
    }

    if msg == WM_DROPFILES {
        let hdrop = wparam as HDROP;
        let len = DragQueryFileW(hdrop, 0, null_mut(), 0);
        if len > 0 {
            let mut buf = vec![0u16; (len as usize) + 1];
            let copied = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), buf.len() as u32);
            if copied > 0 {
                let path = PathBuf::from(OsString::from_wide(&buf[..copied as usize]));
                crate::state::AppController::on_native_file_dropped(path);
            }
        }
        DragFinish(hdrop);
        return 0;
    }

    CallWindowProcW(prev_fn, hwnd, msg, wparam, lparam)
}

/// 在 32/64 位目标下挂载置顶点击免抢焦点与拖拽子类化钩子（零 Mutex 锁）
pub fn install_focus_subclass(
    hwnd: isize,
    overlay_open_flag: Arc<AtomicBool>,
) {
    if hwnd == 0 {
        return;
    }

    let _ = SUBCLASS_CTX.set(SubclassContext {
        prev_wndproc: AtomicIsize::new(0),
        overlay_open_flag,
    });

    if let Some(ctx) = SUBCLASS_CTX.get() {
        #[cfg(target_pointer_width = "64")]
        let prev = unsafe {
            SetWindowLongPtrW(
                hwnd as HWND,
                GWLP_WNDPROC,
                custom_wndproc as usize as isize,
            )
        };
        #[cfg(target_pointer_width = "32")]
        let prev = unsafe {
            SetWindowLongW(
                hwnd as HWND,
                GWL_WNDPROC,
                custom_wndproc as usize as i32,
            ) as isize
        };
        ctx.prev_wndproc.store(prev as isize, Ordering::Relaxed);
    }
}

/// 在启动首屏完成或大文件导入/导出结束后收缩进程物理内存工作集
pub fn trim_working_set() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// 弹出系统原生错误消息框
pub fn show_error_messagebox(title: &str, msg: &str) {
    let title_w = to_wide_null(title);
    let msg_w = to_wide_null(msg);
    unsafe {
        MessageBoxW(0, msg_w.as_ptr(), title_w.as_ptr(), MB_ICONERROR | MB_OK);
    }
}

/// 获取使用设备本机当前日期的 8 位数字格式（如 `"20261109"`）
pub fn get_local_date_yyyymmdd() -> String {
    let mut st = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    unsafe {
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st);
    }
    format!("{:04}{:02}{:02}", st.wYear, st.wMonth, st.wDay)
}

