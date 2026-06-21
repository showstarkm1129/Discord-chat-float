#[cfg(windows)]
mod platform {
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

    #[derive(Default)]
    struct SearchState {
        hwnd: Option<HWND>,
    }

    pub fn toggle_discord_window() -> Result<(), String> {
        let hwnd = find_discord_window().ok_or("Discordのウィンドウが見つかりません")?;

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
        let mut state = SearchState::default();
        let state_ptr = &mut state as *mut SearchState;

        unsafe {
            let _ = EnumWindows(Some(enum_windows), LPARAM(state_ptr as isize));
        }

        state.hwnd
    }

    unsafe extern "system" fn enum_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }

        let title = window_title(hwnd);
        if title.trim().is_empty() {
            return true.into();
        }

        let process_name = process_name(hwnd);
        let title_lc = title.to_ascii_lowercase();
        let process_lc = process_name.to_ascii_lowercase();
        let is_discord = process_lc.ends_with("discord.exe")
            || process_lc.contains("\\discord\\")
            || title_lc.contains("discord");

        if is_discord {
            let state = &mut *(lparam.0 as *mut SearchState);
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

    unsafe fn process_name(hwnd: HWND) -> String {
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
}

#[cfg(not(windows))]
mod platform {
    pub fn toggle_discord_window() -> Result<(), String> {
        Err("Discord呼び出しモードは今のところWindows専用です".into())
    }
}

pub fn toggle_discord_window() -> Result<(), String> {
    platform::toggle_discord_window()
}
