import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";
import type { TrackerSnapshot, UpdateInfo } from "./types";

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

export async function installStartupUpdate(
  onStatus?: (info: UpdateInfo) => void
): Promise<UpdateInfo> {
  const isDevRuntime = Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);

  if (!isTauriRuntime() || isDevRuntime) {
    return { state: "not_available", message: "개발 실행에서는 업데이트 확인을 생략합니다." };
  }

  const publish = (info: UpdateInfo) => {
    onStatus?.(info);
    return info;
  };

  try {
    publish({ state: "checking" });
    const update = await check();

    if (!update) {
      return publish({ state: "not_available" });
    }

    const baseInfo = {
      version: update.version,
      notes: update.body
    };
    let downloadedBytes = 0;
    let contentLength: number | undefined;

    publish({ state: "available", ...baseInfo });

    await update.downloadAndInstall((event: DownloadEvent) => {
      if (event.event === "Started") {
        downloadedBytes = 0;
        contentLength = event.data.contentLength;
        publish({ state: "downloading", progress: 0, ...baseInfo });
      } else if (event.event === "Progress") {
        downloadedBytes += event.data.chunkLength;
        publish({
          state: "downloading",
          progress: contentLength ? Math.min(100, (downloadedBytes / contentLength) * 100) : undefined,
          ...baseInfo
        });
      } else if (event.event === "Finished") {
        publish({ state: "installing", progress: 100, ...baseInfo });
      }
    });

    publish({
      state: "ready_to_install",
      message: "업데이트 설치 완료. 앱을 재시작합니다.",
      progress: 100,
      ...baseInfo
    });
    await relaunch();

    return { state: "ready_to_install", progress: 100, ...baseInfo };
  } catch (error) {
    return publish({
      state: "error",
      message: error instanceof Error ? error.message : String(error)
    });
  }
}
