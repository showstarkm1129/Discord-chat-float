use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use thiserror::Error;

mod window_toggle;

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+D";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct AppSettings {
    webhook_url: String,
    username: String,
    avatar_url: String,
    channel_label: String,
    draft: String,
    target_window_title: String,
    target_process_name: String,
    shortcut: String,
    window_x: Option<i32>,
    window_y: Option<i32>,
}

#[derive(Default)]
struct ShortcutRegistration {
    current: Mutex<String>,
}

#[derive(Debug, Serialize)]
struct SendResult {
    ok: bool,
    rate_limited: bool,
    message: String,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("設定ファイルを扱えませんでした")]
    ConfigPath,
    #[error("設定を保存できませんでした")]
    Save,
    #[error("設定を読み込めませんでした")]
    Load,
    #[error("Webhook URLを設定してください")]
    MissingWebhook,
    #[error("送信内容が空です")]
    EmptyMessage,
    #[error("ウィンドウ操作に失敗しました")]
    Window,
    #[error("{0}")]
    TargetWindow(String),
    #[error("ショートカットを登録できませんでした")]
    Shortcut,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<AppSettings, AppError> {
    read_settings(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), AppError> {
    update_shortcut_if_needed(&app, &settings)?;
    write_settings(&app, &settings)
}

#[tauri::command]
async fn send_webhook_message(
    content: String,
    settings: AppSettings,
) -> Result<SendResult, AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::EmptyMessage);
    }
    if settings.webhook_url.trim().is_empty() {
        return Err(AppError::MissingWebhook);
    }

    let client = reqwest::Client::new();
    let mut payload = serde_json::json!({ "content": content });

    if !settings.username.trim().is_empty() {
        payload["username"] = serde_json::json!(settings.username.trim());
    }
    if !settings.avatar_url.trim().is_empty() {
        payload["avatar_url"] = serde_json::json!(settings.avatar_url.trim());
    }

    let response = client
        .post(settings.webhook_url.trim())
        .json(&payload)
        .send()
        .await;

    match response {
        Ok(response) if response.status().is_success() => Ok(SendResult {
            ok: true,
            rate_limited: false,
            message: "送信しました".into(),
        }),
        Ok(response) if response.status() == StatusCode::TOO_MANY_REQUESTS => Ok(SendResult {
            ok: false,
            rate_limited: true,
            message: "少し待ってください".into(),
        }),
        Ok(response) => Ok(SendResult {
            ok: false,
            rate_limited: false,
            message: format!("送信に失敗しました ({})", response.status()),
        }),
        Err(_) => Ok(SendResult {
            ok: false,
            rate_limited: false,
            message: "送信に失敗しました。Webhook URLやネットワークを確認してください".into(),
        }),
    }
}

#[tauri::command]
fn hide_quick_window(app: AppHandle) -> Result<(), AppError> {
    let window = main_window(&app)?;
    remember_window_position(&app, &window)?;
    window.minimize().map_err(|_| AppError::Window)
}

#[tauri::command]
fn show_quick_window(app: AppHandle) -> Result<(), AppError> {
    show_and_focus(&app)
}

#[tauri::command]
fn toggle_discord_window() -> Result<(), AppError> {
    let settings = AppSettings::default();
    toggle_target_from_settings(&settings)
}

#[tauri::command]
fn toggle_target_window(app: AppHandle) -> Result<(), AppError> {
    let settings = read_settings(&app)?;
    toggle_target_from_settings(&settings)
}

#[tauri::command]
fn list_target_windows() -> Vec<window_toggle::WindowInfo> {
    window_toggle::list_windows()
}

pub fn run() {
    tauri::Builder::default()
        .manage(ShortcutRegistration::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = shortcut;
                        let _ = read_settings(app)
                            .and_then(|settings| toggle_target_from_settings(&settings))
                            .or_else(|_| toggle_quick_window(app));
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            send_webhook_message,
            hide_quick_window,
            show_quick_window,
            toggle_discord_window,
            toggle_target_window,
            list_target_windows
        ])
        .setup(|app| {
            let settings = read_settings(app.handle()).unwrap_or_default();
            {
                if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            register_shortcut(app.handle(), &settings).map_err(|err| err.to_string())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle();
                if let Ok(webview) = main_window(app) {
                    let _ = remember_window_position(app, &webview);
                    let _ = webview.minimize();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn toggle_quick_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let window = main_window(app)?;
    if window.is_minimized().unwrap_or(false) {
        window.unminimize().map_err(|_| AppError::Window)?;
        window.show().map_err(|_| AppError::Window)?;
        window.set_focus().map_err(|_| AppError::Window)?;
        return Ok(());
    }

    if window.is_visible().unwrap_or(false) {
        remember_window_position(app, &window)?;
        window.minimize().map_err(|_| AppError::Window)?;
        return Ok(());
    }

    show_and_focus(app)
}

fn toggle_target_from_settings(settings: &AppSettings) -> Result<(), AppError> {
    window_toggle::toggle_target_window(
        settings.target_window_title.trim(),
        settings.target_process_name.trim(),
    )
    .map_err(AppError::TargetWindow)
}

fn effective_shortcut(settings: &AppSettings) -> String {
    let shortcut = settings.shortcut.trim();
    if shortcut.is_empty() {
        DEFAULT_SHORTCUT.to_string()
    } else {
        shortcut.to_string()
    }
}

fn update_shortcut_if_needed<R: Runtime>(
    app: &AppHandle<R>,
    settings: &AppSettings,
) -> Result<(), AppError> {
    let shortcut = effective_shortcut(settings);
    let state = app.state::<ShortcutRegistration>();
    let current = state.current.lock().map_err(|_| AppError::Shortcut)?;
    if *current == shortcut {
        return Ok(());
    }
    drop(current);
    register_shortcut(app, settings)
}

fn register_shortcut<R: Runtime>(app: &AppHandle<R>, settings: &AppSettings) -> Result<(), AppError> {
    let shortcut = effective_shortcut(settings);
    app.global_shortcut()
        .unregister_all()
        .map_err(|_| AppError::Shortcut)?;
    app.global_shortcut()
        .register(shortcut.as_str())
        .map_err(|_| AppError::Shortcut)?;

    let state = app.state::<ShortcutRegistration>();
    let mut current = state.current.lock().map_err(|_| AppError::Shortcut)?;
    *current = shortcut;
    Ok(())
}

fn show_and_focus<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let window = main_window(app)?;
    if window.is_minimized().unwrap_or(false) {
        window.unminimize().map_err(|_| AppError::Window)?;
    }
    window.show().map_err(|_| AppError::Window)?;
    window.set_focus().map_err(|_| AppError::Window)?;
    Ok(())
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Result<WebviewWindow<R>, AppError> {
    app.get_webview_window("main").ok_or(AppError::Window)
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, AppError> {
    let dir = app.path().app_config_dir().map_err(|_| AppError::ConfigPath)?;
    Ok(dir.join("settings.json"))
}

fn read_settings<R: Runtime>(app: &AppHandle<R>) -> Result<AppSettings, AppError> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = fs::read_to_string(path).map_err(|_| AppError::Load)?;
    serde_json::from_str(&raw).map_err(|_| AppError::Load)
}

fn write_settings<R: Runtime>(app: &AppHandle<R>, settings: &AppSettings) -> Result<(), AppError> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| AppError::Save)?;
    }

    let raw = serde_json::to_string_pretty(settings).map_err(|_| AppError::Save)?;
    fs::write(path, raw).map_err(|_| AppError::Save)
}

fn remember_window_position<R: Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<(), AppError> {
    let mut settings = read_settings(app).unwrap_or_default();
    if let Ok(position) = window.outer_position() {
        settings.window_x = Some(position.x);
        settings.window_y = Some(position.y);
        write_settings(app, &settings)?;
    }
    Ok(())
}
