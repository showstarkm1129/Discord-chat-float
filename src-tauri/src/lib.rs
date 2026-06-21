use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use thiserror::Error;

mod window_toggle;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct AppSettings {
    webhook_url: String,
    username: String,
    avatar_url: String,
    channel_label: String,
    draft: String,
    window_x: Option<i32>,
    window_y: Option<i32>,
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
    window_toggle::toggle_discord_window().map_err(AppError::TargetWindow)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if shortcut == &quick_shortcut() && event.state() == ShortcutState::Pressed {
                        let _ = window_toggle::toggle_discord_window()
                            .or_else(|_| toggle_quick_window(app).map_err(|err| err.to_string()));
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
            toggle_discord_window
        ])
        .setup(|app| {
            if let Ok(settings) = read_settings(app.handle()) {
                if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                }
            }

            app.global_shortcut().register(quick_shortcut())?;
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

fn quick_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyD)
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
