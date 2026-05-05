import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
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

  useEffect(() => {
    // RegionSelector mounts when the selector window becomes visible.
    // We directly invoke capture_screen_for_selector to get base64 —
    // this is safer than listening for a push event that may fire before mount.
    invoke<string>("capture_screen_for_selector")
      .then((base64Data) => {
        const img = new Image();
        img.onload = () => {
          imgRef.current = img;
          draw(img, null);
          setReady(true);
        };
        img.onerror = (e) => console.error("Failed to load base64 screenshot", e);
        img.src = base64Data;
      })
      .catch((err) => console.error("capture_screen_for_selector failed:", err));
  }, []);

  const draw = useCallback((img: HTMLImageElement, sel: SelectionRect | null) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

    // dark overlay
    ctx.fillStyle = "rgba(0,0,0,0.45)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (sel) {
      const x = Math.min(sel.startX, sel.endX);
      const y = Math.min(sel.startY, sel.endY);
      const w = Math.abs(sel.endX - sel.startX);
      const h = Math.abs(sel.endY - sel.startY);
      if (w > 0 && h > 0) {
        ctx.clearRect(x, y, w, h);
        ctx.drawImage(img, x, y, w, h, x, y, w, h);
        ctx.strokeStyle = "#3b82f6";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);
        // corner handles
        ctx.fillStyle = "#3b82f6";
        for (const [hx, hy] of [[x,y],[x+w,y],[x,y+h],[x+w,y+h]] as [number,number][]) {
          ctx.fillRect(hx - 4, hy - 4, 8, 8);
        }
        // size label
        const label = `${Math.round(w)} × ${Math.round(h)}`;
        ctx.font = "bold 13px monospace";
        const lw = ctx.measureText(label).width + 14;
        const ly = y > 28 ? y - 26 : y + h + 4;
        ctx.fillStyle = "#3b82f6";
        ctx.fillRect(x, ly, lw, 22);
        ctx.fillStyle = "#fff";
        ctx.fillText(label, x + 7, ly + 15);
      }
    }

    // instruction bar
    ctx.fillStyle = "rgba(0,0,0,0.6)";
    ctx.fillRect(0, 0, canvas.width, 44);
    ctx.fillStyle = "rgba(255,255,255,0.92)";
    ctx.font = "bold 14px system-ui,sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Drag to select a region   •   Esc to cancel", canvas.width / 2, 27);
    ctx.textAlign = "left";
  }, []);

  useEffect(() => {
    if (imgRef.current) draw(imgRef.current, selectionRef.current);
  });

  const onMouseDown = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    isSelectingRef.current = true;
    selectionRef.current = { startX: e.clientX, startY: e.clientY, endX: e.clientX, endY: e.clientY };
    setTick(t => t + 1);
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!isSelectingRef.current || !selectionRef.current) return;
    selectionRef.current = { ...selectionRef.current, endX: e.clientX, endY: e.clientY };
    setTick(t => t + 1);
  }, []);

  const onMouseUp = useCallback(async () => {
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
    await getCurrentWindow().hide();
    await emitTo("main", "region-selected", {
      x: Math.round(x), y: Math.round(y),
      width: Math.round(w), height: Math.round(h),
    });
  }, []);

  useEffect(() => {
    const onKey = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        await getCurrentWindow().hide();
        await emitTo("main", "region-selection-cancelled", {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="fixed inset-0 overflow-hidden" style={{ cursor: "crosshair", background: "#000" }}>
      {!ready && (
        <div className="flex items-center justify-center w-full h-full text-white text-sm font-medium">
          Preparing capture…
        </div>
      )}
      <canvas
        ref={canvasRef}
        style={{ display: ready ? "block" : "none", cursor: "crosshair", width: "100%", height: "100%" }}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
      />
    </div>
  );
}
