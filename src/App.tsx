import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Settings = {
  webhook_url: string;
  username: string;
  avatar_url: string;
  channel_label: string;
  draft: string;
};

type SendResult = {
  ok: boolean;
  rate_limited: boolean;
  message: string;
};

const emptySettings: Settings = {
  webhook_url: "",
  username: "",
  avatar_url: "",
  channel_label: "",
  draft: "",
};

function App() {
  const [settings, setSettings] = useState<Settings>(emptySettings);
  const [message, setMessage] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isSending, setIsSending] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const destinationLabel = useMemo(() => {
    if (settings.channel_label.trim()) return settings.channel_label.trim();
    return settings.webhook_url ? "Discord Webhook" : "送信先未設定";
  }, [settings.channel_label, settings.webhook_url]);

  useEffect(() => {
    invoke<Settings>("load_settings")
      .then((loaded) => {
        setSettings({ ...emptySettings, ...loaded });
        setMessage(loaded.draft ?? "");
      })
      .catch(() => setError("設定を読み込めませんでした"));
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => inputRef.current?.focus(), 80);
    return () => window.clearTimeout(timer);
  }, [isSettingsOpen]);

  useEffect(() => {
    function handleWindowKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      hideWindow();
    }

    window.addEventListener("keydown", handleWindowKeyDown);
    return () => window.removeEventListener("keydown", handleWindowKeyDown);
  });

  async function saveSettings(nextSettings = settings) {
    setError("");
    setStatus("");
    await invoke("save_settings", {
      settings: { ...nextSettings, draft: message },
    });
    setStatus("保存しました");
  }

  async function saveDraft(nextDraft: string) {
    setSettings((current) => {
      const next = { ...current, draft: nextDraft };
      invoke("save_settings", { settings: next }).catch(() => undefined);
      return next;
    });
  }

  async function sendMessage(content = message) {
    const trimmed = content.trim();
    if (!trimmed || isSending) return;
    if (!settings.webhook_url.trim()) {
      setError("Webhook URLを設定してください");
      setIsSettingsOpen(true);
      return;
    }

    setIsSending(true);
    setError("");
    setStatus("送信中...");

    try {
      const result = await invoke<SendResult>("send_webhook_message", {
        content: trimmed,
        settings,
      });

      if (result.ok) {
        setMessage("");
        await saveDraft("");
        setStatus("送信しました");
        inputRef.current?.focus();
        return;
      }

      setError(result.rate_limited ? "少し待ってください" : result.message);
      setStatus("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "送信に失敗しました");
      setStatus("");
    } finally {
      setIsSending(false);
    }
  }

  async function sendTest() {
    await sendMessage("Discord Chat Float のテスト送信です。");
  }

  async function hideWindow() {
    await saveDraft(message);
    await invoke("hide_quick_window");
  }

  async function toggleDiscord() {
    setError("");
    setStatus("");
    try {
      await invoke("toggle_discord_window");
      setStatus("Discordを切り替えました");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      hideWindow();
      return;
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      sendMessage();
    }
  }

  function updateSetting<K extends keyof Settings>(key: K, value: Settings[K]) {
    setSettings((current) => ({ ...current, [key]: value }));
  }

  return (
    <main className="app-shell">
      <div className="title-bar" data-tauri-drag-region>
        <span data-tauri-drag-region>Discord Chat Float</span>
        <button type="button" className="window-button" onClick={hideWindow} aria-label="最小化">
          _
        </button>
      </div>

      <section className="top-row">
        <div>
          <p className="eyebrow">Main mode</p>
          <h1>Discord呼び出し</h1>
        </div>
        <div className="button-row">
          <button type="button" onClick={toggleDiscord}>
            Discord切替
          </button>
          <button
            type="button"
            className="ghost-button"
            onClick={() => setIsSettingsOpen((value) => !value)}
          >
            Webhook設定
          </button>
        </div>
      </section>

      <section className="mode-panel">
        <span>Ctrl+Shift+D でDiscordを前面化 / 最小化</span>
        <small>Discordデスクトップアプリを開いておいてください</small>
      </section>

      {isSettingsOpen && (
        <section className="settings-panel" aria-label="設定">
          <div className="subheading">
            <span>Webhook送信モード</span>
            <small>サブ機能として残しています</small>
          </div>
          <label>
            Webhook URL
            <input
              type="password"
              value={settings.webhook_url}
              placeholder="https://discord.com/api/webhooks/..."
              onChange={(event) => updateSetting("webhook_url", event.target.value)}
            />
          </label>
          <div className="settings-grid">
            <label>
              表示名
              <input
                value={settings.username}
                placeholder="任意"
                onChange={(event) => updateSetting("username", event.target.value)}
              />
            </label>
            <label>
              送信先メモ（表示だけ）
              <input
                value={settings.channel_label}
                placeholder="例: #general（送信先はWebhookで決まります）"
                onChange={(event) => updateSetting("channel_label", event.target.value)}
              />
            </label>
          </div>
          <label>
            アイコンURL
            <input
              value={settings.avatar_url}
              placeholder="任意"
              onChange={(event) => updateSetting("avatar_url", event.target.value)}
            />
          </label>
          <div className="button-row">
            <button type="button" onClick={() => saveSettings()} disabled={isSending}>
              保存
            </button>
            <button type="button" onClick={sendTest} disabled={isSending}>
              テスト送信
            </button>
          </div>
        </section>
      )}

      <textarea
        ref={inputRef}
        value={message}
        rows={3}
        placeholder="Discordへ送る文章"
        onChange={(event) => {
          setMessage(event.target.value);
          saveDraft(event.target.value);
        }}
        onKeyDown={handleKeyDown}
        disabled={isSending}
      />

      <footer>
        <span className={error ? "error-text" : "status-text"}>
          {error || status || `${destinationLabel} / EnterでWebhook送信 / Escで最小化`}
        </span>
        <button type="button" onClick={() => sendMessage()} disabled={isSending || !message.trim()}>
          {isSending ? "送信中" : "送信"}
        </button>
      </footer>
    </main>
  );
}

export default App;
