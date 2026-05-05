import { editorActions } from "@/stores/editorStore";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { isAssetId, isDataUrl, migrateStoredValue } from "@/lib/asset-registry";
import { processScreenshotWithDefaultBackground } from "@/lib/auto-process";
import { hasCompletedOnboarding } from "@/lib/onboarding";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import {
  availableMonitors,
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { Store } from "@tauri-apps/plugin-store";
import { AppWindowMac, Crop, Monitor } from "lucide-react";
import { toast } from "sonner";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardShortcut } from "./components/preferences/KeyboardShortcutManager";
import { SettingsIcon } from "./components/SettingsIcon";

const ImageEditor = lazy(() => import("./components/ImageEditor").then(m => ({ default: m.ImageEditor })));
const OnboardingFlow = lazy(() => import("./components/onboarding/OnboardingFlow").then(m => ({ default: m.OnboardingFlow })));
const PreferencesPage = lazy(() => import("./components/preferences/PreferencesPage").then(m => ({ default: m.PreferencesPage })));

type AppMode = "main" | "editing" | "preferences";
type CaptureMode = "region" | "fullscreen" | "window";

function LoadingFallback() {
  return (
    <div className="min-h-dvh flex items-center justify-center bg-background">
      <div className="flex items-center gap-2 text-muted-foreground">
        <svg className="animate-spin size-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <span>Loading...</span>
      </div>
    </div>
  );
}

const DEFAULT_SHORTCUTS: KeyboardShortcut[] = [
  { id: "region", action: "Capture Region", shortcut: "CommandOrControl+Shift+2", enabled: true },
  { id: "fullscreen", action: "Capture Screen", shortcut: "CommandOrControl+Shift+F", enabled: false },
  { id: "window", action: "Capture Window", shortcut: "CommandOrControl+Shift+D", enabled: false },
];

function formatShortcut(shortcut: string): string {
  return shortcut
    .replace(/CommandOrControl/g, "Ctrl")
    .replace(/Command/g, "Ctrl")
    .replace(/Control/g, "Ctrl")
    .replace(/Shift/g, "Shift+")
    .replace(/Alt/g, "Alt+")
    .replace(/\+\+/g, "+");
}

async function restoreWindowOnScreen(mouseX?: number, mouseY?: number) {
  const appWindow = getCurrentWindow();
  const windowWidth = 1200;
  const windowHeight = 800;
  await appWindow.setSize(new LogicalSize(windowWidth, windowHeight));
  if (mouseX !== undefined && mouseY !== undefined) {
    try {
      const monitors = await availableMonitors();
      const targetMonitor = monitors.find((m) => {
        const pos = m.position; const size = m.size;
        return mouseX >= pos.x && mouseX < pos.x + size.width && mouseY >= pos.y && mouseY < pos.y + size.height;
      });
      if (targetMonitor) {
        const sf = targetMonitor.scaleFactor;
        const cx = targetMonitor.position.x + (targetMonitor.size.width - windowWidth * sf) / 2;
        const cy = targetMonitor.position.y + (targetMonitor.size.height - windowHeight * sf) / 2;
        await appWindow.setPosition(new PhysicalPosition(cx, cy));
      } else { await appWindow.center(); }
    } catch { await appWindow.center(); }
  } else { await appWindow.center(); }
  await appWindow.show();
  await appWindow.setFocus();
}

async function restoreWindow() { await restoreWindowOnScreen(); }

async function showQuickOverlay(screenshotPath: string, mouseX?: number, mouseY?: number) {
  try {
    const store = await Store.load("settings.json", { defaults: {}, autoSave: true });
    await store.set("lastCapturePath", screenshotPath);
    await store.save();
  } catch {}
  try { await emitTo("quick-overlay", "overlay-show-capture", { path: screenshotPath }); } catch {}
  try {
    const { getAllWebviewWindows } = await import("@tauri-apps/api/webviewWindow");
    const allWindows = await getAllWebviewWindows();
    const overlay = allWindows.find((win) => win.label === "quick-overlay");
    if (!overlay) return;
    const overlayWidth = 360; const overlayHeight = 240; const margin = 16;
    let targetX = 0; let targetY = 0;
    try {
      const monitors = await availableMonitors();
      let tm = monitors[0];
      if (mouseX !== undefined && mouseY !== undefined) {
        const found = monitors.find((m) => { const pos = m.position; const s = m.size; return mouseX >= pos.x && mouseX < pos.x + s.width && mouseY >= pos.y && mouseY < pos.y + s.height; });
        if (found) tm = found;
      }
      const sf = tm.scaleFactor;
      targetX = tm.position.x + tm.size.width - overlayWidth * sf - margin * sf;
      targetY = tm.position.y + tm.size.height - overlayHeight * sf - margin * sf;
    } catch {}
    await overlay.setSize(new LogicalSize(overlayWidth, overlayHeight));
    await overlay.setPosition(new PhysicalPosition(targetX, targetY));
    await overlay.setAlwaysOnTop(true);
    await overlay.show();
    await overlay.setFocus();
  } catch {}
}

function App() {
  const [mode, setMode] = useState<AppMode>("main");
  const [saveDir, setSaveDir] = useState<string>("");
  const [copyToClipboard, setCopyToClipboard] = useState(true);
  const [autoApplyBackground, setAutoApplyBackground] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isCapturing, setIsCapturing] = useState(false);
  const [tempScreenshotPath, setTempScreenshotPath] = useState<string | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [shortcuts, setShortcuts] = useState<KeyboardShortcut[]>(DEFAULT_SHORTCUTS);
  const [settingsVersion, setSettingsVersion] = useState(0);
  const [tempDir, setTempDir] = useState<string>("");

  const settingsRef = useRef({ autoApplyBackground, saveDir, copyToClipboard, tempDir });
  const registeredShortcutsRef = useRef<Set<string>>(new Set());
  const lastCaptureTimeRef = useRef(0);
  const pendingRegionCaptureRef = useRef<string | null>(null);

  useEffect(() => { settingsRef.current = { autoApplyBackground, saveDir, copyToClipboard, tempDir }; }, [autoApplyBackground, saveDir, copyToClipboard, tempDir]);

  const loadSettings = useCallback(async () => {
    try {
      const store = await Store.load("settings.json", { defaults: { copyToClipboard: true, autoApplyBackground: false }, autoSave: true });
      const savedCopyToClip = await store.get<boolean>("copyToClipboard");
      if (savedCopyToClip !== null && savedCopyToClip !== undefined) setCopyToClipboard(savedCopyToClip);
      const savedAutoApply = await store.get<boolean>("autoApplyBackground");
      if (savedAutoApply !== null && savedAutoApply !== undefined) setAutoApplyBackground(savedAutoApply);
      const savedSaveDir = await store.get<string>("saveDir");
      if (savedSaveDir) setSaveDir(savedSaveDir);
      const savedShortcuts = await store.get<KeyboardShortcut[]>("keyboardShortcuts");
      if (savedShortcuts && savedShortcuts.length > 0) {
        const savedIds = new Set(savedShortcuts.map((s) => s.id));
        const missingDefaults = DEFAULT_SHORTCUTS.filter((d) => !savedIds.has(d.id));
        setShortcuts([...savedShortcuts, ...missingDefaults]);
      } else { setShortcuts(DEFAULT_SHORTCUTS); }
    } catch {}
  }, []);

  useEffect(() => {
    const init = async () => {
      let desktopPath = "";
      try { desktopPath = await invoke<string>("get_desktop_directory"); } catch (err) { setError(`Failed to get Desktop directory`); }
      try { const td = await invoke<string>("get_temp_directory"); setTempDir(td); } catch {}
      try {
        const store = await Store.load("settings.json", { defaults: { copyToClipboard: true, autoApplyBackground: false }, autoSave: true });
        const savedCopyToClip = await store.get<boolean>("copyToClipboard");
        if (savedCopyToClip !== null && savedCopyToClip !== undefined) setCopyToClipboard(savedCopyToClip);
        const savedAutoApply = await store.get<boolean>("autoApplyBackground");
        if (savedAutoApply !== null && savedAutoApply !== undefined) setAutoApplyBackground(savedAutoApply);
        const savedSaveDir = await store.get<string>("saveDir");
        if (savedSaveDir && savedSaveDir.trim() !== "") { setSaveDir(savedSaveDir); }
        else { setSaveDir(desktopPath); if (desktopPath) { await store.set("saveDir", desktopPath); await store.save(); } }
        const savedShortcuts = await store.get<KeyboardShortcut[]>("keyboardShortcuts");
        if (savedShortcuts && savedShortcuts.length > 0) setShortcuts(savedShortcuts);
        const savedBg = await store.get<string>("defaultBackgroundImage");
        if (savedBg && !isAssetId(savedBg) && !isDataUrl(savedBg)) {
          const migrated = migrateStoredValue(savedBg);
          if (migrated && migrated !== savedBg) { await store.set("defaultBackgroundImage", migrated); await store.save(); }
        }
      } catch (err) { if (desktopPath) setSaveDir(desktopPath); }
    };
    init();
    if (!hasCompletedOnboarding()) setShowOnboarding(true);
  }, []);

  // Region selection listener
  useEffect(() => {
    let unlistenSel: (() => void) | null = null;
    let unlistenCan: (() => void) | null = null;
    let mounted = true;
    const setup = async () => {
      unlistenSel = await listen<{ x: number; y: number; width: number; height: number }>("region-selected", async (event) => {
        if (!mounted) return;
        const { x, y, width, height } = event.payload;
        pendingRegionCaptureRef.current = null;
        const { autoApplyBackground: shouldAutoApply, saveDir: currentSaveDir, copyToClipboard: shouldCopyToClipboard, tempDir: td } = settingsRef.current;
        try {
          // screenshotData lives in Rust static — just pass coords + saveDir
          const croppedPath = await invoke<string>("crop_and_save_region", { x, y, width, height, saveDir: td || currentSaveDir });
          invoke("play_screenshot_sound").catch(() => {});
          if (shouldAutoApply) {
            try {
              const processed = await processScreenshotWithDefaultBackground(croppedPath);
              const savedPath = await invoke<string>("save_edited_image", { imageData: processed, saveDir: currentSaveDir, copyToClip: shouldCopyToClipboard });
              const appWindow = getCurrentWindow();
              await appWindow.hide();
              await showQuickOverlay(savedPath);
            } catch (err) { setError(`Failed to process: ${err instanceof Error ? err.message : String(err)}`); await restoreWindow(); }
            finally { setIsCapturing(false); }
            return;
          }
          // Restore window FIRST so it's visible when ImageEditor mounts and loads image
          await restoreWindow();
          setTempScreenshotPath(croppedPath);
          setMode("editing");
        } catch (err) { setError(err instanceof Error ? err.message : String(err)); await restoreWindow(); }
        finally { setIsCapturing(false); }
      });
      unlistenCan = await listen("region-selection-cancelled", async () => {
        if (!mounted) return;
        pendingRegionCaptureRef.current = null;
        setIsCapturing(false);
        await restoreWindow();
      });
    };
    setup();
    return () => { mounted = false; unlistenSel?.(); unlistenCan?.(); };
  }, []);

  const handleCapture = useCallback(async (captureMode: CaptureMode = "region") => {
    const now = Date.now();
    if (now - lastCaptureTimeRef.current < 600) return;
    lastCaptureTimeRef.current = now;
    if (isCapturing) return;
    setIsCapturing(true);
    setError(null);
    const appWindow = getCurrentWindow();
    const { autoApplyBackground: shouldAutoApply, saveDir: currentSaveDir, copyToClipboard: shouldCopyToClipboard, tempDir: currentTempDir } = settingsRef.current;
    try {
      await appWindow.hide();
      await new Promise((resolve) => setTimeout(resolve, 300));
      if (captureMode === "region") {
        const { getAllWebviewWindows } = await import("@tauri-apps/api/webviewWindow");
        const allWindows = await getAllWebviewWindows();
        const selector = allWindows.find((win) => win.label === "region-selector");
        if (selector) {
          // Rust captures screen, stores base64 in static, shows selector window
          await invoke("native_capture_interactive", { saveDir: currentTempDir || currentSaveDir });
          pendingRegionCaptureRef.current = "pending"; // just a flag — actual data is in Rust
          // Selector window is already shown by Rust; ensure fullscreen
          await selector.setFullscreen(true);
          await selector.setFocus();
        } else {
          const screenshotPath = await invoke<string>("native_capture_fullscreen", { saveDir: currentTempDir || currentSaveDir });
          invoke("play_screenshot_sound").catch(() => {});
          if (shouldAutoApply) {
            const processed = await processScreenshotWithDefaultBackground(screenshotPath);
            const savedPath = await invoke<string>("save_edited_image", { imageData: processed, saveDir: currentSaveDir, copyToClip: shouldCopyToClipboard });
            await showQuickOverlay(savedPath);
            setIsCapturing(false); return;
          }
          await restoreWindow(); setTempScreenshotPath(screenshotPath); setMode("editing");
          setIsCapturing(false);
        }
        return;
      }
      const cmdMap: Record<Exclude<CaptureMode, "region">, string> = { fullscreen: "native_capture_fullscreen", window: "native_capture_window" };
      const screenshotPath = await invoke<string>(cmdMap[captureMode], { saveDir: currentTempDir || currentSaveDir });
      let mouseX: number | undefined; let mouseY: number | undefined;
      try { const [x, y] = await invoke<[number, number]>("get_mouse_position"); mouseX = x; mouseY = y; } catch {}
      invoke("play_screenshot_sound").catch(() => {});
      if (shouldAutoApply) {
        try {
          const processed = await processScreenshotWithDefaultBackground(screenshotPath);
          const savedPath = await invoke<string>("save_edited_image", { imageData: processed, saveDir: currentSaveDir, copyToClip: shouldCopyToClipboard });
          await appWindow.hide(); await showQuickOverlay(savedPath, mouseX, mouseY);
        } catch (err) { setError(`Failed to process: ${err instanceof Error ? err.message : String(err)}`); await restoreWindow(); }
        finally { setIsCapturing(false); }
        return;
      }
      await restoreWindowOnScreen(mouseX, mouseY); setTempScreenshotPath(screenshotPath); setMode("editing");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (!msg.includes("cancelled")) setError(msg);
      if (!shouldAutoApply) await restoreWindow();
    } finally {
      if (captureMode !== "region") setIsCapturing(false);
    }
  }, [isCapturing]);

  useEffect(() => {
    const setupHotkeys = async () => {
      try {
        const toUnregister = Array.from(registeredShortcutsRef.current);
        if (toUnregister.length > 0) { try { await unregister(toUnregister); } catch {} }
        registeredShortcutsRef.current.clear();
        const actionMap: Record<string, CaptureMode> = { "Capture Region": "region", "Capture Screen": "fullscreen", "Capture Window": "window" };
        for (const shortcut of shortcuts) {
          if (!shortcut.enabled) continue;
          const action = actionMap[shortcut.action];
          if (action) {
            try { await register(shortcut.shortcut, () => handleCapture(action)); registeredShortcutsRef.current.add(shortcut.shortcut); }
            catch (err) { console.error(`Failed to register ${shortcut.shortcut}:`, err); }
          }
        }
      } catch (err) { setError(`Hotkey error: ${err instanceof Error ? err.message : String(err)}`); }
    };
    setupHotkeys();
    return () => {
      const toUnregister = Array.from(registeredShortcutsRef.current);
      if (toUnregister.length > 0) unregister(toUnregister).catch(() => {});
      registeredShortcutsRef.current.clear();
    };
  }, [shortcuts, settingsVersion, handleCapture]);

  const handleCaptureRef = useRef(handleCapture);
  useEffect(() => { handleCaptureRef.current = handleCapture; }, [handleCapture]);

  useEffect(() => {
    let u1: (() => void) | null = null, u2: (() => void) | null = null, u3: (() => void) | null = null;
    let u4: (() => void) | null = null, u5: (() => void) | null = null, u6: (() => void) | null = null;
    let mounted = true;
    const setup = async () => {
      u1 = await listen("capture-triggered", () => { if (mounted) handleCaptureRef.current("region"); });
      u2 = await listen("capture-fullscreen", () => { if (mounted) handleCaptureRef.current("fullscreen"); });
      u3 = await listen("capture-window", () => { if (mounted) handleCaptureRef.current("window"); });
      u4 = await listen("open-preferences", () => { if (mounted) setMode("preferences"); });
      u5 = await listen<{ path: string }>("open-editor-for-path", async (event) => {
        if (!mounted) return;
        await restoreWindow(); setTempScreenshotPath(event.payload.path); setMode("editing");
      });
      u6 = await listen("show-last-capture-overlay", async () => {
        if (!mounted) return;
        try { const store = await Store.load("settings.json"); const lp = await store.get<string>("lastCapturePath"); if (lp) await showQuickOverlay(lp); } catch {}
      });
    };
    setup();
    return () => { mounted = false; u1?.(); u2?.(); u3?.(); u4?.(); u5?.(); u6?.(); };
  }, []);

  const handleSettingsChange = useCallback(async () => { await loadSettings(); setSettingsVersion(v => v + 1); }, [loadSettings]);
  const handleAutoApplyToggle = useCallback(async (checked: boolean) => {
    setAutoApplyBackground(checked);
    try { const store = await Store.load("settings.json"); await store.set("autoApplyBackground", checked); await store.save(); }
    catch { toast.error("Failed to save setting"); }
  }, []);
  const handleBackFromPreferences = useCallback(async () => { await loadSettings(); setSettingsVersion(v => v + 1); setMode("main"); }, [loadSettings]);

  async function handleEditorSave(editedImageData: string) {
    try {
      const savedPath = await invoke<string>("save_edited_image", { imageData: editedImageData, saveDir, copyToClip: copyToClipboard });
      toast.success("Image saved", { description: savedPath, duration: 4000 });
      editorActions.reset(); setMode("main"); setTempScreenshotPath(null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg); toast.error("Failed to save image", { description: msg, duration: 5000 });
      editorActions.reset(); setMode("main");
    }
  }

  async function handleEditorCancel() { editorActions.reset(); setMode("main"); setTempScreenshotPath(null); }

  const getShortcutDisplay = (actionId: string): string => {
    const shortcut = shortcuts.find(s => s.id === actionId);
    if (shortcut && shortcut.enabled) return formatShortcut(shortcut.shortcut);
    const def = DEFAULT_SHORTCUTS.find(s => s.id === actionId);
    return def ? formatShortcut(def.shortcut) : "—";
  };

  if (mode === "editing" && tempScreenshotPath) {
    return (<Suspense fallback={<LoadingFallback />}><ImageEditor imagePath={tempScreenshotPath} onSave={handleEditorSave} onCancel={handleEditorCancel} /></Suspense>);
  }
  if (showOnboarding) {
    return (<Suspense fallback={<LoadingFallback />}><OnboardingFlow onComplete={() => setShowOnboarding(false)} /></Suspense>);
  }
  if (mode === "preferences") {
    return (<Suspense fallback={<LoadingFallback />}><PreferencesPage onBack={handleBackFromPreferences} onSettingsChange={handleSettingsChange} /></Suspense>);
  }

  return (
    <main className="min-h-dvh flex flex-col items-center justify-center p-8 bg-background text-foreground">
      <div className="w-full max-w-2xl space-y-6">
        <div className="relative text-center space-y-2">
          <div className="absolute top-0 right-0"><SettingsIcon onClick={() => setMode("preferences")} /></div>
          <div className="flex flex-col items-center gap-1">
            <div className="flex items-center gap-2">
              <h1 className="text-5xl font-bold text-foreground text-balance">ClipShot</h1>
              <span className="rounded-full border border-border bg-card px-2 py-0.5 text-xs font-medium text-muted-foreground tabular-nums">v{__APP_VERSION__}</span>
            </div>
            <p className="text-muted-foreground text-sm text-pretty">Capture, edit, and enhance your screenshots with professional quality.</p>
          </div>
        </div>
        <Card className="bg-card border-border">
          <CardContent className="p-6 space-y-6">
            <div className="grid grid-cols-2 gap-3">
              <Button onClick={() => handleCapture("region")} disabled={isCapturing} variant="cta" size="lg" className="py-3 disabled:opacity-50 disabled:cursor-not-allowed">
                <Crop className="size-4" />Region
              </Button>
              <Button onClick={() => handleCapture("fullscreen")} disabled={isCapturing} variant="cta" size="lg" className="py-3 disabled:opacity-50 disabled:cursor-not-allowed">
                <Monitor className="size-4" />Screen
              </Button>
              <Button onClick={() => handleCapture("window")} disabled={isCapturing} variant="cta" size="lg" className="col-span-2 py-3 disabled:opacity-50 disabled:cursor-not-allowed">
                <AppWindowMac className="size-4" />Window
              </Button>
            </div>
            <div className="flex items-center justify-between py-2 px-1">
              <div className="flex-1">
                <label htmlFor="auto-apply-toggle" className="text-sm font-medium text-foreground cursor-pointer block">Auto-apply background</label>
                <p className="text-xs text-muted-foreground">Apply default background and save instantly</p>
              </div>
              <Switch id="auto-apply-toggle" checked={autoApplyBackground} onCheckedChange={handleAutoApplyToggle} />
            </div>
            {isCapturing && (
              <div className="flex items-center justify-center gap-2 text-muted-foreground text-sm">
                <svg className="animate-spin size-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                Waiting for selection...
              </div>
            )}
            {error && (
              <div className="p-4 bg-red-950/30 border border-red-800/50 rounded-lg">
                <div className="font-medium text-red-300 mb-1">Error</div>
                <div className="text-red-400 text-sm text-pretty">{error}</div>
              </div>
            )}
          </CardContent>
        </Card>
        <Card className="bg-card border-border">
          <CardContent className="p-5 space-y-4">
            <h3 className="font-medium text-foreground text-sm">Keyboard Shortcuts</h3>
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground uppercase tracking-wide">Capture</p>
              <div className="grid grid-cols-2 gap-x-8 gap-y-2 text-sm">
                {[["Region", "region"], ["Screen", "fullscreen"], ["Window", "window"]].map(([label, id]) => (
                  <div key={id} className="flex items-center justify-between">
                    <span className="text-muted-foreground">{label}</span>
                    <kbd className="px-2 py-1 bg-secondary border border-border rounded text-foreground font-mono text-xs tabular-nums">{getShortcutDisplay(id)}</kbd>
                  </div>
                ))}
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Cancel</span>
                  <kbd className="px-2 py-1 bg-secondary border border-border rounded text-foreground font-mono text-xs tabular-nums">Esc</kbd>
                </div>
              </div>
            </div>
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground uppercase tracking-wide">Editor</p>
              <div className="grid grid-cols-2 gap-x-8 gap-y-2 text-sm">
                {[["Save", "Ctrl+S"], ["Copy", "Shift+Ctrl+C"], ["Undo", "Ctrl+Z"], ["Redo", "Shift+Ctrl+Z"], ["Delete annotation", "⌫"], ["Close editor", "Esc"]].map(([label, kbd]) => (
                  <div key={label} className="flex items-center justify-between">
                    <span className="text-muted-foreground">{label}</span>
                    <kbd className="px-2 py-1 bg-secondary border border-border rounded text-foreground font-mono text-xs tabular-nums">{kbd}</kbd>
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </main>
  );
}

export default App;
