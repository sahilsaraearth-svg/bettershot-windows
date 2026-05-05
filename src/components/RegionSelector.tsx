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
  const [ready, setReady] = useState(false);
  const selectionRef = useRef<SelectionRect | null>(null);
  const isSelectingRef = useRef(false);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [, setTick] = useState(0);

  // Backend sends base64 PNG directly — no file path, no asset protocol, no encoding issues
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<string>("screenshot-ready-for-selection", (event) => {
      const base64Data = event.payload; // "data:image/png;base64,..."
      const img = new Image();
      img.onload = () => {
        imgRef.current = img;
        resizeAndDraw(img, null);
        setReady(true);
      };
      img.onerror = () => console.error("Failed to load base64 screenshot");
      img.src = base64Data;
    }).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

  const resizeAndDraw = useCallback((img: HTMLImageElement, sel: SelectionRect | null) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // Draw screenshot scaled to fill canvas
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

    // Dark overlay
    ctx.fillStyle = "rgba(0,0,0,0.45)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (sel) {
      const x = Math.min(sel.startX, sel.endX);
      const y = Math.min(sel.startY, sel.endY);
      const w = Math.abs(sel.endX - sel.startX);
      const h = Math.abs(sel.endY - sel.startY);

      if (w > 0 && h > 0) {
        // Show real screenshot through the selection
        ctx.clearRect(x, y, w, h);
        ctx.drawImage(img, x, y, w, h, x, y, w, h);

        // Blue selection border
        ctx.strokeStyle = "#3b82f6";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);

        // Corner handles
        const handleSize = 6;
        ctx.fillStyle = "#3b82f6";
        [[x, y], [x + w, y], [x, y + h], [x + w, y + h]].forEach(([hx, hy]) => {
          ctx.fillRect(hx - handleSize / 2, hy - handleSize / 2, handleSize, handleSize);
        });

        // Size label
        const label = `${Math.round(w)} × ${Math.round(h)}`;
        ctx.font = "bold 13px monospace";
        const lw = ctx.measureText(label).width + 14;
        const lx = x;
        const ly = y > 28 ? y - 26 : y + h + 4;
        ctx.fillStyle = "#3b82f6";
        ctx.fillRect(lx, ly, lw, 22);
        ctx.fillStyle = "#fff";
        ctx.fillText(label, lx + 7, ly + 15);
      }
    }

    // Instruction banner
    ctx.fillStyle = "rgba(0,0,0,0.55)";
    ctx.fillRect(0, 0, canvas.width, 44);
    ctx.fillStyle = "rgba(255,255,255,0.9)";
    ctx.font = "bold 14px system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Drag to select a region   •   Esc to cancel", canvas.width / 2, 27);
    ctx.textAlign = "left";
  }, []);

  // Redraw on every tick (selection change)
  useEffect(() => {
    if (imgRef.current) {
      resizeAndDraw(imgRef.current, selectionRef.current);
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
      selectionRef.current = null;
      setTick(t => t + 1);
      return;
    }

    // Hide selector first so it doesn't block
    const win = getCurrentWindow();
    await win.hide();

    // Tell main window what region was selected
    await emitTo("main", "region-selected", {
      x: Math.round(x),
      y: Math.round(y),
      width: Math.round(w),
      height: Math.round(h),
    });
  }, []);

  // Escape cancels
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
    <div className="fixed inset-0 w-full h-full overflow-hidden" style={{ cursor: "crosshair", background: "#000" }}>
      {!ready && (
        <div className="flex items-center justify-center w-full h-full bg-black text-white text-base font-medium">
          Preparing capture…
        </div>
      )}
      <canvas
        ref={canvasRef}
        className="absolute inset-0"
        style={{ display: ready ? "block" : "none", cursor: "crosshair", width: "100%", height: "100%" }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
      />
    </div>
  );
}
