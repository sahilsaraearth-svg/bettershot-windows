import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
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
  const [isSelecting, setIsSelecting] = useState(false);
  const [selection, setSelection] = useState<SelectionRect | null>(null);
  const selectionRef = useRef<SelectionRect | null>(null);
  const isSelectingRef = useRef(false);
  const screenshotRef = useRef<HTMLImageElement | null>(null);

  // Load screenshot when selector opens
  useEffect(() => {
    const loadScreenshot = async () => {
      try {
        const data = await invoke<string>("capture_screen_for_selector");
        setScreenshotData(data);
        
        const img = new Image();
        img.onload = () => {
          screenshotRef.current = img;
          drawCanvas(img, null);
        };
        img.src = data;
      } catch (err) {
        console.error("Failed to capture screen:", err);
      }
    };

    loadScreenshot();

    // Also listen for screenshot ready events
    const unlisten = listen<string>("screenshot-ready-for-selection", (_event) => {
      // Already handled by capture_screen_for_selector
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const drawCanvas = useCallback((img: HTMLImageElement, sel: SelectionRect | null) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // Draw screenshot
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

    // Darken overlay
    ctx.fillStyle = "rgba(0, 0, 0, 0.4)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (sel) {
      const x = Math.min(sel.startX, sel.endX);
      const y = Math.min(sel.startY, sel.endY);
      const w = Math.abs(sel.endX - sel.startX);
      const h = Math.abs(sel.endY - sel.startY);

      if (w > 0 && h > 0) {
        // Clear the selected region (show it bright)
        ctx.clearRect(x, y, w, h);
        ctx.drawImage(img, x, y, w, h, x, y, w, h);

        // Draw selection border
        ctx.strokeStyle = "#3b82f6";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);

        // Draw size indicator
        const label = `${w} × ${h}`;
        ctx.font = "bold 14px monospace";
        ctx.fillStyle = "#3b82f6";
        ctx.fillRect(x, y > 24 ? y - 24 : y + 4, ctx.measureText(label).width + 12, 22);
        ctx.fillStyle = "#ffffff";
        ctx.fillText(label, x + 6, y > 24 ? y - 6 : y + 19);
      }
    }

    // Draw crosshair instructions
    if (!sel) {
      ctx.fillStyle = "rgba(255, 255, 255, 0.9)";
      ctx.font = "bold 16px system-ui";
      ctx.textAlign = "center";
      ctx.fillText("Click and drag to select a region • Press Esc to cancel", canvas.width / 2, 36);
      ctx.textAlign = "left";
    }
  }, []);

  // Redraw on selection change
  useEffect(() => {
    if (screenshotRef.current) {
      drawCanvas(screenshotRef.current, selection);
    }
  }, [selection, drawCanvas]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    isSelectingRef.current = true;
    setIsSelecting(true);
    const rect = { startX: e.clientX, startY: e.clientY, endX: e.clientX, endY: e.clientY };
    selectionRef.current = rect;
    setSelection(rect);
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isSelectingRef.current || !selectionRef.current) return;
    const updated = { ...selectionRef.current, endX: e.clientX, endY: e.clientY };
    selectionRef.current = updated;
    setSelection(updated);
  }, []);

  const handleMouseUp = useCallback(async (_e: React.MouseEvent) => {
    if (!isSelectingRef.current || !selectionRef.current) return;
    isSelectingRef.current = false;
    setIsSelecting(false);

    const sel = selectionRef.current;
    const x = Math.min(sel.startX, sel.endX);
    const y = Math.min(sel.startY, sel.endY);
    const w = Math.abs(sel.endX - sel.startX);
    const h = Math.abs(sel.endY - sel.startY);

    if (w < 5 || h < 5) {
      // Too small, cancel
      setSelection(null);
      selectionRef.current = null;
      return;
    }

    try {
      // Emit the selection back to the main window
      await emit("region-selected", { x, y, width: w, height: h });
      
      // Hide selector window
      const win = getCurrentWindow();
      await win.hide();
    } catch (err) {
      console.error("Failed to emit selection:", err);
    }
  }, []);

  // Handle Escape key
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        await emit("region-selection-cancelled", {});
        const win = getCurrentWindow();
        await win.hide();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <div
      className="fixed inset-0 w-full h-full overflow-hidden"
      style={{ cursor: isSelecting ? "crosshair" : "crosshair", background: "transparent" }}
    >
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full"
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        style={{ display: screenshotData ? "block" : "none" }}
      />
      {!screenshotData && (
        <div className="flex items-center justify-center w-full h-full bg-black/50 text-white text-lg">
          Preparing capture...
        </div>
      )}
    </div>
  );
}
