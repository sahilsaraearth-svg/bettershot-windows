import { useEffect, useRef, useState, useCallback } from "react";
import { listen, emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface SelectionRect {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

export function RegionSelector() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [screenshotData, setScreenshotData] = useState<string | null>(null);
  const selectionRef = useRef<SelectionRect | null>(null);
  const isSelectingRef = useRef(false);
  const screenshotRef = useRef<HTMLImageElement | null>(null);
  const [_tick, setTick] = useState(0); // force redraw

  // Listen for screenshot path from backend, then display it
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setup = async () => {
      // Backend sends the path to the captured screenshot
      unlisten = await listen<string>("screenshot-ready-for-selection", async (event) => {
        const path = event.payload;
        // Convert file path to asset URL via Tauri's asset protocol
        const assetUrl = convertFileSrc(path);
        setScreenshotData(assetUrl);

        const img = new Image();
        img.onload = () => {
          screenshotRef.current = img;
          drawCanvas(img, null);
        };
        img.src = assetUrl;
      });
    };

    setup();
    return () => { unlisten?.(); };
  }, []);

  // Simple inline convertFileSrc (same as @tauri-apps/api but avoids extra import)
  function convertFileSrc(path: string): string {
    const url = encodeURIComponent(path);
    return `https://asset.localhost/${url}`;
  }

  const drawCanvas = useCallback((img: HTMLImageElement, sel: SelectionRect | null) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // Draw the frozen screenshot
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

    // Semi-transparent dark overlay
    ctx.fillStyle = "rgba(0, 0, 0, 0.45)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (sel) {
      const x = Math.min(sel.startX, sel.endX);
      const y = Math.min(sel.startY, sel.endY);
      const w = Math.abs(sel.endX - sel.startX);
      const h = Math.abs(sel.endY - sel.startY);

      if (w > 0 && h > 0) {
        // Clear overlay on selection — shows the real screenshot through
        ctx.clearRect(x, y, w, h);
        ctx.drawImage(img, x, y, w, h, x, y, w, h);

        // Blue selection border
        ctx.strokeStyle = "#3b82f6";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);

        // Size label
        const label = `${Math.round(w)} × ${Math.round(h)}`;
        ctx.font = "bold 13px monospace";
        const labelWidth = ctx.measureText(label).width + 14;
        const labelY = y > 28 ? y - 26 : y + h + 4;
        ctx.fillStyle = "#3b82f6";
        ctx.fillRect(x, labelY, labelWidth, 22);
        ctx.fillStyle = "#ffffff";
        ctx.fillText(label, x + 7, labelY + 15);
      }
    }

    // Instructions at top
    ctx.fillStyle = "rgba(255,255,255,0.85)";
    ctx.font = "bold 15px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Drag to select a region   •   Esc to cancel", canvas.width / 2, 36);
    ctx.textAlign = "left";
  }, []);

  // Redraw whenever selection changes
  useEffect(() => {
    if (screenshotRef.current) {
      drawCanvas(screenshotRef.current, selectionRef.current);
    }
  });

  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    isSelectingRef.current = true;
    selectionRef.current = { startX: e.clientX, startY: e.clientY, endX: e.clientX, endY: e.clientY };
    setTick(t => t + 1);
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!isSelectingRef.current || !selectionRef.current) return;
    selectionRef.current = { ...selectionRef.current, endX: e.clientX, endY: e.clientY };
    setTick(t => t + 1);
  }, []);

  const handleMouseUp = useCallback(async () => {
    if (!isSelectingRef.current || !selectionRef.current) return;
    isSelectingRef.current = false;

    const sel = selectionRef.current;
    const x = Math.min(sel.startX, sel.endX);
    const y = Math.min(sel.startY, sel.endY);
    const w = Math.abs(sel.endX - sel.startX);
    const h = Math.abs(sel.endY - sel.startY);

    if (w < 8 || h < 8) {
      // Too small — cancel
      selectionRef.current = null;
      setTick(t => t + 1);
      return;
    }

    // Hide selector first
    const win = getCurrentWindow();
    await win.hide();

    // Emit to main window
    await emitTo("main", "region-selected", {
      x: Math.round(x),
      y: Math.round(y),
      width: Math.round(w),
      height: Math.round(h),
    });
  }, []);

  // Escape key cancels
  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const win = getCurrentWindow();
        await win.hide();
        await emitTo("main", "region-selection-cancelled", {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="fixed inset-0 w-full h-full overflow-hidden" style={{ cursor: "crosshair", background: "transparent" }}>
      {!screenshotData && (
        <div className="flex items-center justify-center w-full h-full bg-black/60 text-white text-base font-medium">
          Preparing capture…
        </div>
      )}
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full"
        style={{ display: screenshotData ? "block" : "none", cursor: "crosshair" }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
      />
    </div>
  );
}
