# Better Shot for Windows 🪟

A professional screenshot capture and editing tool for Windows — built with [Tauri](https://tauri.app/) + React.

> Ported from the original macOS Better Shot app. All core features work natively on Windows.

---

## ⬇️ Download & Install

Go to the [**Releases**](https://github.com/sahilsaraearth-svg/bettershot-windows/releases/latest) page and download:

- **`Better.Shot_0.2.5_x64-setup.exe`** — Windows installer (recommended)

> **Requirement:** [Microsoft WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) must be installed. Most Windows 11 machines already have it. If not, the installer will prompt you.

---

## ✨ Features

| Feature | Status |
|---|---|
| Region capture (draw a selection) | ✅ |
| Fullscreen capture | ✅ |
| Window capture | ✅ |
| All-monitors capture | ✅ |
| Image editor (crop, annotate, effects) | ✅ |
| Background effects (padding, shadows, gradients) | ✅ |
| Copy to clipboard | ✅ |
| Save to Desktop / custom folder | ✅ |
| System tray with global shortcuts | ✅ |
| Auto-launch on startup | ✅ |
| Quick overlay window | ✅ |
| OCR (text recognition) | ❌ macOS only |

---

## ⌨️ Default Keyboard Shortcuts

| Action | Shortcut |
|---|---|
| Capture Region | `Ctrl+Shift+4` |
| Capture Fullscreen | `Ctrl+Shift+3` |
| Capture Window | `Ctrl+Shift+2` |
| Open Better Shot | `Ctrl+Shift+1` |

Shortcuts can be customized in **Preferences**.

---

## 🛠️ Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) stable
- [Tauri CLI](https://tauri.app/start/): `npm install -g @tauri-apps/cli`

### Steps

```bash
git clone https://github.com/sahilsaraearth-svg/bettershot-windows.git
cd bettershot-windows
npm install
npm run tauri dev        # development
npm run tauri build      # production build
```

Output will be at:
```
src-tauri/target/release/bundle/nsis/Better Shot_0.2.5_x64-setup.exe
```

---

## 🏗️ Tech Stack

- **Frontend:** React 18 + TypeScript + Vite + Tailwind CSS
- **Backend:** Rust (Tauri 2)
- **Screenshot capture:** [xcap](https://github.com/nashaofu/xcap)
- **Clipboard:** [arboard](https://github.com/1Password/arboard)
- **Bundler:** NSIS (Windows installer)

---

## 📝 License

MIT
