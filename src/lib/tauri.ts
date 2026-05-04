import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import type { TrackerSnapshot, UpdateInfo } from "./types";

let pendingUpdate: Update | null = null;

export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function loadTrackerSnapshot(): Promise<TrackerSnapshot | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  return invoke<TrackerSnapshot>("tracker_snapshot", {});
}

export async function resetTrackerSession(): Promise<TrackerSnapshot | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  return invoke<TrackerSnapshot>("reset_tracker_session", {});
}

export async function setManualItemPrice(
  configBaseId: number,
  priceInCrystal: number
): Promise<TrackerSnapshot | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  return invoke<TrackerSnapshot>("set_manual_item_price", { configBaseId, priceInCrystal });
}

export async function setItemIgnored(
  configBaseId: number,
  ignored: boolean
): Promise<TrackerSnapshot | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  return invoke<TrackerSnapshot>("set_item_ignored", { configBaseId, ignored });
}

export async function setPositionLocked(locked: boolean): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke("set_position_locked", { locked });
}

export async function setClickableRects(rects: DOMRect[]): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke("set_clickable_rects", {
    rects: rects.map((rect) => ({
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height
    }))
  });
}

export async function setOverlayWindowSize(width: number, height: number): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke("set_overlay_window_size", { width, height });
}

export async function setOverlayOpacity(opacity: number): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke("set_overlay_opacity", { opacity });
}

export async function checkForUpdate(): Promise<UpdateInfo> {
  if (!isTauriRuntime()) {
    return { state: "not_available", message: "브라우저 미리보기에서는 업데이트 확인을 생략합니다." };
  }

  try {
    await pendingUpdate?.close();
    pendingUpdate = null;

    const update = await check();

    if (!update) {
      return { state: "not_available" };
    }

    pendingUpdate = update;

    return {
      state: "available",
      version: update.version,
      notes: update.body
    };
  } catch (error) {
    return {
      state: "error",
      message: error instanceof Error ? error.message : String(error)
    };
  }
}

export async function installAvailableUpdate(
  onProgress?: (info: UpdateInfo) => void
): Promise<UpdateInfo> {
  if (!isTauriRuntime()) {
    return { state: "error", message: "브라우저 미리보기에서는 업데이트 설치를 실행할 수 없습니다." };
  }

  try {
    const update = pendingUpdate ?? (await check());

    if (!update) {
      pendingUpdate = null;
      return { state: "not_available" };
    }

    pendingUpdate = update;

    let downloaded = 0;
    let contentLength: number | undefined;

    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        downloaded = 0;
        contentLength = event.data.contentLength;
        onProgress?.({ state: "downloading", version: update.version, progress: 0 });
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        const progress = contentLength ? Math.min(100, (downloaded / contentLength) * 100) : undefined;
        onProgress?.({ state: "downloading", version: update.version, progress });
      } else if (event.event === "Finished") {
        onProgress?.({ state: "installing", version: update.version, progress: 100 });
      }
    });

    await update.close();
    pendingUpdate = null;

    return { state: "installing", version: update.version, progress: 100 };
  } catch (error) {
    return {
      state: "error",
      message: error instanceof Error ? error.message : String(error)
    };
  }
}
