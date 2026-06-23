#[cfg(windows)]
mod platform {
    use serde::Serialize;
    use windows::Win32::{
        Foundation::{CloseHandle, BOOL, HWND, LPARAM},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
            SW_MINIMIZE, SW_RESTORE,
        },
    };

    #[derive(Debug, Clone, Serialize)]
    pub struct WindowInfo {
        pub title: String,
        pub process_name: String,
    }

    #[derive(Default)]
    struct SearchState {
        hwnd: Option<HWND>,
        target_title: String,
        target_process_name: String,
    }

    #[derive(Default)]
    struct ListState {
        windows: Vec<WindowInfo>,
    }

    pub fn list_windows() -> Vec<WindowInfo> {
        let mut state = ListState::default();
        let state_ptr = &mut state as *mut ListState;

        unsafe {
            let _ = EnumWindows(Some(enum_windows_for_list), LPARAM(state_ptr as isize));
        }

        state.windows
    }

    pub fn toggle_target_window(title: &str, process_name: &str) -> Result<(), String> {
        let hwnd = find_target_window(title, process_name)
            .or_else(find_discord_window)
            .ok_or("対象ウィンドウが見つかりません")?;

        unsafe {
            let foreground = GetForegroundWindow();
            if IsIconic(hwnd).as_bool() || hwnd != foreground {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            } else {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
        }

        Ok(())
    }

    fn find_discord_window() -> Option<HWND> {
        find_target_window("", "Discord.exe")
    }

    fn find_target_window(title: &str, process_name: &str) -> Option<HWND> {
        let mut state = SearchState {
            target_title: title.to_string(),
            target_process_name: process_name.to_string(),
            ..SearchState::default()
        };
        let state_ptr = &mut state as *mut SearchState;

        unsafe {
            let _ = EnumWindows(Some(enum_windows), LPARAM(state_ptr as isize));
        }

        state.hwnd
    }

    unsafe extern "system" fn enum_windows_for_list(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }

        let title = window_title(hwnd);
        if title.trim().is_empty() || title == "Discord Chat Float" {
            return true.into();
        }

        let process_name = exe_name(&process_path(hwnd));
        if process_name.is_empty() {
            return true.into();
        }

        let state = &mut *(lparam.0 as *mut ListState);
        state.windows.push(WindowInfo {
            title,
            process_name,
        });

        true.into()
    }

    unsafe extern "system" fn enum_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }

        let title = window_title(hwnd);
        if title.trim().is_empty() {
            return true.into();
        }

        let process_name = exe_name(&process_path(hwnd));
        let title_lc = title.to_ascii_lowercase();
        let process_lc = process_name.to_ascii_lowercase();
        let state = &mut *(lparam.0 as *mut SearchState);
        let target_title_lc = state.target_title.to_ascii_lowercase();
        let target_process_lc = state.target_process_name.to_ascii_lowercase();
        let process_matches =
            target_process_lc.is_empty() || process_lc == target_process_lc;
        let title_matches = target_title_lc.is_empty() || title_lc == target_title_lc;
        let is_discord_fallback =
            target_process_lc == "discord.exe" && (process_lc == "discord.exe" || title_lc.contains("discord"));
        let is_target = (process_matches && title_matches) || is_discord_fallback;

        if is_target {
            state.hwnd = Some(hwnd);
            return false.into();
        }

        true.into()
    }

    unsafe fn window_title(hwnd: HWND) -> String {
        let length = GetWindowTextLengthW(hwnd);
        if length <= 0 {
            return String::new();
        }

        let mut buffer = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(hwnd, &mut buffer);
        String::from_utf16_lossy(&buffer[..written as usize])
    }

    unsafe fn process_path(hwnd: HWND) -> String {
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id as *mut u32));
        if process_id == 0 {
            return String::new();
        }

        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) else {
            return String::new();
        };

        let mut buffer = vec![0u16; 32768];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );

        let _ = CloseHandle(process);

        if result.is_err() {
            return String::new();
        }

        String::from_utf16_lossy(&buffer[..size as usize])
    }

    fn exe_name(path: &str) -> String {
        path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
    }
}

#[cfg(not(windows))]
mod platform {
    use serde::Serialize;

    #[derive(Debug, Clone, Serialize)]
    pub struct WindowInfo {
        pub title: String,
        pub process_name: String,
    }

    pub fn list_windows() -> Vec<WindowInfo> {
        Vec::new()
    }

    pub fn toggle_target_window(_title: &str, _process_name: &str) -> Result<(), String> {
        Err("Discord呼び出しモードは今のところWindows専用です".into())
    }
}

pub use platform::WindowInfo;

pub fn list_windows() -> Vec<WindowInfo> {
    platform::list_windows()
}

pub fn toggle_target_window(title: &str, process_name: &str) -> Result<(), String> {
    platform::toggle_target_window(title, process_name)
}
