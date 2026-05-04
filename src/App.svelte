<script lang="ts">
  import { onMount, tick } from "svelte";
  import { buildChartSeries } from "./lib/chart";
  import { formatDuration, formatNumber, formatRate } from "./lib/format";
  import { sampleCurrentRun, sampleItems, sampleRuns } from "./lib/sample-data";
  import {
    installStartupUpdate,
    loadTrackerSnapshot,
    resetTrackerSession,
    setClickableRects,
    setItemIgnored,
    setManualItemPrice,
    setOverlayOpacity,
    setOverlayWindowSize,
    setPositionLocked
  } from "./lib/tauri";
  import type {
    ChartMode,
    CurrentRun,
    DetailTab,
    ItemFilter,
    ItemValuationRow,
    RunSummary,
    UpdateInfo
  } from "./lib/types";

  let currentRun: CurrentRun = sampleCurrentRun;
  let runs: RunSummary[] = sampleRuns;
  let items: ItemValuationRow[] = sampleItems;
  let detailsOpen = false;
  let positionLocked = false;
  let opacity = 92;
  let chartMode: ChartMode = "rate";
  let detailTab: DetailTab = "runs";
  let itemFilter: ItemFilter = "all";
  let manualPrices: Record<number, string> = {};
  let itemActionError = "";
  let updateInfo: UpdateInfo = { state: "idle" };
  let hasSnapshot = false;
  let snapshotTotalEstimatedValue = 0;
  let snapshotUnpricedItemCount = 0;

  let shellElement: HTMLElement;
  let barElement: HTMLElement;
  let detailsElement: HTMLElement;

  $: completedEstimatedValue = runs.reduce((sum, run) => sum + run.totalEstimatedValue, 0);
  $: fallbackTotalEstimatedValue = completedEstimatedValue + currentRun.totalEstimatedValue;
  $: totalEstimatedValue = hasSnapshot ? snapshotTotalEstimatedValue : fallbackTotalEstimatedValue;
  $: fallbackUnpricedItemCount = runs.reduce(
    (sum, run) => sum + run.unpricedItemCount,
    currentRun.unpricedItemCount
  );
  $: unpricedItemCount = hasSnapshot ? snapshotUnpricedItemCount : fallbackUnpricedItemCount;
  $: totalSeconds = runs.reduce((sum, run) => sum + run.durationSeconds, 0);
  $: averageSeconds = runs.length > 0 ? totalSeconds / runs.length : 0;
  $: averageCrystal = runs.length > 0 ? completedEstimatedValue / runs.length : 0;
  $: averageRate = totalSeconds > 0 ? (completedEstimatedValue / totalSeconds) * 3600 : 0;
  $: currentRate = formatRate(currentRun.totalEstimatedValue, currentRun.elapsedSeconds);
  $: chartSeries = buildChartSeries(runs, chartMode);
  $: chartTitle = chartMode === "cumulative" ? "누적 결정" : "시간 당 결정";
  $: pricedItemCount = items.filter((item) => !item.ignored && !item.unpriced).length;
  $: unpricedItemRows = items.filter((item) => item.unpriced).length;
  $: ignoredItemCount = items.filter((item) => item.ignored).length;
  $: itemTotalValue = items.reduce((sum, item) => sum + item.valueInCrystal, 0);
  $: showUpdateStatus = !["idle", "not_available"].includes(updateInfo.state);
  $: updateStatusText = formatUpdateStatus(updateInfo);
  $: visibleItems = items.filter((item) => {
    if (itemFilter === "priced") {
      return !item.ignored && !item.unpriced;
    }

    if (itemFilter === "unpriced") {
      return item.unpriced;
    }

    if (itemFilter === "ignored") {
      return item.ignored;
    }

    return true;
  });

  onMount(() => {
    let disposed = false;
    let pollTimer: number | undefined;

    void (async () => {
      await setPositionLocked(positionLocked);
      await setOverlayOpacity(opacity / 100);
      await refreshSnapshot();

      if (disposed) {
        return;
      }

      pollTimer = window.setInterval(refreshSnapshot, 1_000);

      await syncOverlayLayout();
      window.addEventListener("resize", syncOverlayLayout);
      void installStartupUpdate((info) => {
        updateInfo = info;
        void syncOverlayLayout();
      });
    })();

    return () => {
      disposed = true;
      if (pollTimer) {
        window.clearInterval(pollTimer);
      }
      window.removeEventListener("resize", syncOverlayLayout);
    };
  });

  async function togglePositionLocked() {
    positionLocked = !positionLocked;
    await setPositionLocked(positionLocked);
    await syncOverlayLayout();
  }

  async function toggleDetails() {
    detailsOpen = !detailsOpen;
    await syncOverlayLayout();
  }

  async function setMode(mode: ChartMode) {
    chartMode = mode;
    await syncOverlayLayout();
  }

  async function setDetailTab(tab: DetailTab) {
    detailTab = tab;
    await syncOverlayLayout();
  }

  async function setItemFilter(filter: ItemFilter) {
    itemFilter = filter;
    await syncOverlayLayout();
  }

  async function resetSession() {
    const snapshot = await resetTrackerSession();
    applySnapshot(snapshot);
    await syncOverlayLayout();
  }

  async function refreshSnapshot() {
    const snapshot = await loadTrackerSnapshot();
    applySnapshot(snapshot);
  }

  function applySnapshot(snapshot: Awaited<ReturnType<typeof loadTrackerSnapshot>>) {
    if (!snapshot) {
      return;
    }

    currentRun = snapshot.currentRun;
    runs = snapshot.runs;
    items = snapshot.items;
    hasSnapshot = true;
    snapshotTotalEstimatedValue = snapshot.totalEstimatedValue;
    snapshotUnpricedItemCount = snapshot.unpricedItemCount;
  }

  async function handleOpacityInput() {
    await setOverlayOpacity(opacity / 100);
  }

  async function syncClickableRects() {
    await tick();

    const controls = shellElement?.querySelectorAll<HTMLElement>(
      "button, input, .opacity-control, .runs-scroll, .items-scroll"
    );

    if (!controls) {
      return;
    }

    await setClickableRects([...controls].map((element) => element.getBoundingClientRect()));
  }

  async function syncOverlayLayout() {
    await syncClickableRects();
    await resizeOverlayWindow();
  }

  function handleManualPriceInput(configBaseId: number, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    manualPrices = { ...manualPrices, [configBaseId]: input.value };
  }

  async function saveManualPrice(item: ItemValuationRow) {
    const rawPrice = manualPrices[item.configBaseId]?.trim() ?? "";
    const price = Number(rawPrice);

    if (!rawPrice || !Number.isFinite(price) || price <= 0) {
      itemActionError = "가격은 0보다 큰 숫자로 입력해야 합니다.";
      await syncOverlayLayout();
      return;
    }

    itemActionError = "";
    let snapshot: Awaited<ReturnType<typeof setManualItemPrice>>;

    try {
      snapshot = await setManualItemPrice(item.configBaseId, price);
    } catch (error) {
      itemActionError = `가격 저장 실패: ${formatError(error)}`;
      await syncOverlayLayout();
      return;
    }

    if (snapshot) {
      applySnapshot(snapshot);
    } else {
      items = items.map((entry) =>
        entry.configBaseId === item.configBaseId
          ? {
              ...entry,
              priceInCrystal: price,
              priceSource: entry.ignored ? "ignored" : "manual",
              observedAt: new Date().toISOString(),
              observationCount: entry.observationCount + 1,
              valueInCrystal: entry.ignored ? 0 : entry.quantity * price,
              unpriced: false
            }
          : entry
      );
    }

    const nextManualPrices = { ...manualPrices };
    delete nextManualPrices[item.configBaseId];
    manualPrices = nextManualPrices;
    await syncOverlayLayout();
  }

  async function toggleIgnored(item: ItemValuationRow) {
    itemActionError = "";
    const ignored = !item.ignored;
    let snapshot: Awaited<ReturnType<typeof setItemIgnored>>;

    try {
      snapshot = await setItemIgnored(item.configBaseId, ignored);
    } catch (error) {
      itemActionError = `무시 설정 실패: ${formatError(error)}`;
      await syncOverlayLayout();
      return;
    }

    if (snapshot) {
      applySnapshot(snapshot);
    } else {
      items = items.map((entry) =>
        entry.configBaseId === item.configBaseId
          ? {
              ...entry,
              ignored,
              priceSource: ignored ? "ignored" : entry.priceInCrystal == null ? "unpriced" : "manual",
              valueInCrystal:
                ignored || entry.priceInCrystal == null ? 0 : entry.priceInCrystal * entry.quantity,
              unpriced: !ignored && entry.priceInCrystal == null
            }
          : entry
      );
    }

    await syncOverlayLayout();
  }

  function formatError(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function priceSourceLabel(item: ItemValuationRow): string {
    if (item.ignored) {
      return "무시";
    }

    if (item.priceSource === "fixed") {
      return "고정";
    }

    if (item.priceSource === "manual") {
      return "수동";
    }

    if (item.priceSource === "fresh") {
      return "수집";
    }

    return "미평가";
  }

  function formatObservedAt(value: string | null | undefined): string {
    if (!value) {
      return "-";
    }

    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return "-";
    }

    return `${(date.getMonth() + 1).toString().padStart(2, "0")}/${date
      .getDate()
      .toString()
      .padStart(2, "0")} ${date.getHours().toString().padStart(2, "0")}:${date
      .getMinutes()
      .toString()
      .padStart(2, "0")}`;
  }

  function formatUpdateStatus(info: UpdateInfo): string {
    if (info.state === "checking") {
      return "확인 중";
    }

    if (info.state === "available") {
      return `${info.version ?? "새 버전"} 준비`;
    }

    if (info.state === "downloading") {
      return info.progress == null ? "다운로드 중" : `다운로드 ${Math.round(info.progress)}%`;
    }

    if (info.state === "installing") {
      return "설치 중";
    }

    if (info.state === "ready_to_install") {
      return "재시작 중";
    }

    if (info.state === "error") {
      return "오류";
    }

    return "";
  }

  async function resizeOverlayWindow() {
    await tick();

    const barBounds = barElement?.getBoundingClientRect();
    if (!barBounds) {
      return;
    }

    let right = barBounds.right;
    let bottom = barBounds.bottom;

    if (detailsOpen && detailsElement) {
      const detailsBounds = detailsElement.getBoundingClientRect();
      right = Math.max(right, detailsBounds.right);
      bottom = Math.max(bottom, detailsBounds.bottom);
    }

    await setOverlayWindowSize(Math.ceil(right), Math.ceil(bottom));
  }
</script>

<main
  bind:this={shellElement}
  class="tracker-shell"
  data-position-locked={positionLocked}
>
  <section
    bind:this={barElement}
    class={`tracker-bar${showUpdateStatus ? " has-update" : ""}`}
    aria-label="TLI 트래커 상단 바"
  >
    <div class="segment status" data-tauri-drag-region={positionLocked ? undefined : ""}>
      <button
        class="pin-toggle"
        type="button"
        aria-pressed={positionLocked}
        title={positionLocked ? "위치 고정" : "위치 이동 가능"}
        aria-label={positionLocked ? "위치 고정" : "위치 이동 가능"}
        onclick={togglePositionLocked}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
          <path d="M10.7 1.2 14.8 5.3 13.5 6.6 12.4 5.5 9.3 8.6l.4 2.7-.9.9-2.9-2.9-3.7 3.7-1-1 3.7-3.7-2.9-2.9.9-.9 2.7.4 3.1-3.1-1.1-1.1 1.3-1.3Z" />
        </svg>
      </button>
      <span class="value">{currentRun.mapNameKo} {currentRun.difficulty} {formatDuration(currentRun.elapsedSeconds)}</span>
    </div>

    <div class="segment current-run" data-tauri-drag-region={positionLocked ? undefined : ""}>
      <span class="label">런</span>
      <span class="value">
        <span class="highlight">+{formatNumber(currentRun.totalEstimatedValue)} 결정</span>
        <span class="separator">·</span>
        <span class="metric">{currentRate}/h</span>
      </span>
    </div>

    <div class="segment reset-stats" data-tauri-drag-region={positionLocked ? undefined : ""}>
      <span class="label">누적</span>
      <span class="value">
        <span class="highlight">{formatNumber(totalEstimatedValue)} 결정</span>
        <span class="separator">·</span>
        <span class="metric">평균 {formatNumber(averageRate)}/h</span>
        <span class="separator">·</span>
        <span class="metric muted">미평가 {unpricedItemCount}</span>
      </span>
    </div>

    <div class="segment overlay-controls" aria-label="오버레이 제어">
      <label class="opacity-control">
        <span class="label">투명</span>
        <input
          class="opacity-slider"
          type="range"
          min="5"
          max="100"
          bind:value={opacity}
          oninput={handleOpacityInput}
          aria-label="투명도"
        />
        <span class="opacity-value">{opacity}%</span>
      </label>
    </div>

    {#if showUpdateStatus}
      <div class="segment update-status" data-state={updateInfo.state}>
        <span class="label">업데이트</span>
        <span class="value">{updateStatusText}</span>
      </div>
    {/if}

    <div class="segment actions">
      <button class="reset-button" type="button" onclick={resetSession}>초기화</button>
      <button
        class="detail-toggle"
        type="button"
        aria-expanded={detailsOpen}
        aria-controls="tracker-details"
        onclick={toggleDetails}
      >
        상세 <span class="chevron" aria-hidden="true">▾</span>
      </button>
    </div>
  </section>

  {#if detailsOpen}
    <section
      bind:this={detailsElement}
      class="details-panel"
      id="tracker-details"
      aria-label="TLI 트래커 상세 정보"
      style={`--bar-width: ${barElement?.offsetWidth ?? 1380}px;`}
    >
      <nav class="detail-tabs" aria-label="상세 탭">
        <button
          type="button"
          aria-pressed={detailTab === "runs"}
          onclick={() => setDetailTab("runs")}
        >
          런 통계
        </button>
        <button
          type="button"
          aria-pressed={detailTab === "items"}
          onclick={() => setDetailTab("items")}
        >
          아이템 평가
          <span>{items.length}</span>
        </button>
      </nav>

      {#if detailTab === "runs"}
        <div class="analytics-grid">
          <section class="chart-panel" aria-label="런 그래프">
            <div class="panel-head">
              <span class="panel-title">{chartTitle}</span>
              <span class="chart-switch" aria-label="그래프 종류">
                <button type="button" aria-pressed={chartMode === "rate"} onclick={() => setMode("rate")}>런별</button>
                <button type="button" aria-pressed={chartMode === "cumulative"} onclick={() => setMode("cumulative")}>누적</button>
              </span>
            </div>

            <svg class:chart-cumulative={chartMode === "cumulative"} class="chart" viewBox="0 0 1200 126" role="img" aria-label="런 그래프">
              <path class="chart-grid" d="M54 20H1170M54 48H1170M54 76H1170M54 106H1170" />
              <path class="chart-area" d={chartSeries.areaPath} />
              <path class="chart-line" d={chartSeries.linePath} />
              {#each chartSeries.points as point}
                <circle class="chart-point" cx={point.x} cy={point.y} r="3" />
              {/each}
              <text class="chart-label" x="8" y="23">{chartSeries.maxLabel}</text>
              <text class="chart-label" x="8" y="107">{chartSeries.minLabel}</text>
              <text class="chart-label" x="54" y="122">1</text>
              <text class="chart-label" x="1150" y="122">{chartSeries.endLabel}</text>
            </svg>
          </section>

          <section class="runs-panel" aria-label="런 기록">
            <div class="runs-scroll">
              <table class="run-table">
                <colgroup>
                  <col class="map-col" />
                  <col class="difficulty-col" />
                  <col class="time-col" />
                  <col class="crystal-col" />
                  <col class="unpriced-col" />
                  <col class="rate-col" />
                </colgroup>
                <thead>
                  <tr>
                    <th>맵</th>
                    <th>난이도</th>
                    <th>시간</th>
                    <th>가치</th>
                    <th>미평가</th>
                    <th>결정/h</th>
                  </tr>
                </thead>
                <tbody>
                  {#each runs as run}
                    <tr>
                      <td>{run.mapNameKo}</td>
                      <td class="muted">{run.difficulty}</td>
                      <td>{formatDuration(run.durationSeconds)}</td>
                      <td class="gold">{formatNumber(run.totalEstimatedValue)}</td>
                      <td>{run.unpricedItemCount}</td>
                      <td class="gold">{formatRate(run.totalEstimatedValue, run.durationSeconds)}</td>
                    </tr>
                  {/each}
                </tbody>
                <tfoot>
                  <tr>
                    <td>평균</td>
                    <td></td>
                    <td>{formatDuration(averageSeconds)}</td>
                    <td>{formatNumber(averageCrystal)}</td>
                    <td>{unpricedItemCount}</td>
                    <td>{formatNumber(averageRate)}</td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </section>
        </div>
      {:else}
        <section class="item-panel" aria-label="아이템 평가">
          <div class="item-toolbar">
            <div class="item-filters" aria-label="아이템 필터">
              <button type="button" aria-pressed={itemFilter === "all"} onclick={() => setItemFilter("all")}>전체 {items.length}</button>
              <button type="button" aria-pressed={itemFilter === "priced"} onclick={() => setItemFilter("priced")}>평가 {pricedItemCount}</button>
              <button type="button" aria-pressed={itemFilter === "unpriced"} onclick={() => setItemFilter("unpriced")}>미평가 {unpricedItemRows}</button>
              <button type="button" aria-pressed={itemFilter === "ignored"} onclick={() => setItemFilter("ignored")}>무시 {ignoredItemCount}</button>
            </div>
            <div class="item-summary">
              <span>평가액 <strong>{formatNumber(itemTotalValue)}</strong></span>
              <span>수집가 {pricedItemCount}</span>
              <span>미평가 {unpricedItemRows}</span>
            </div>
          </div>

          {#if itemActionError}
            <p class="item-error">{itemActionError}</p>
          {/if}

          <div class="items-scroll">
            <table class="item-table">
              <colgroup>
                <col class="item-id-col" />
                <col class="item-qty-col" />
                <col class="item-state-col" />
                <col class="item-price-col" />
                <col class="item-updated-col" />
                <col class="item-manual-col" />
                <col class="item-value-col" />
                <col class="item-action-col" />
              </colgroup>
              <thead>
                <tr>
                  <th>아이템</th>
                  <th>수량</th>
                  <th>상태</th>
                  <th>수집 가격</th>
                  <th>갱신</th>
                  <th>수동 입력</th>
                  <th>평가액</th>
                  <th>설정</th>
                </tr>
              </thead>
              <tbody>
                {#each visibleItems as item}
                  <tr class:ignored-row={item.ignored} class:unpriced-row={item.unpriced}>
                    <td class="item-id">
                      <span>{item.itemNameKo ?? `ID ${item.configBaseId}`}</span>
                      {#if item.itemNameKo}
                        <small>ID {item.configBaseId}</small>
                      {/if}
                    </td>
                    <td>{formatNumber(item.quantity, 0)}</td>
                    <td>
                      <span class="source-pill" data-source={item.priceSource}>{priceSourceLabel(item)}</span>
                      {#if item.observationCount > 0}
                        <span class="source-meta">{item.observationCount}회</span>
                      {/if}
                    </td>
                    <td class="gold">
                      {item.priceInCrystal == null ? "-" : formatNumber(item.priceInCrystal, 2)}
                    </td>
                    <td>
                      {formatObservedAt(item.observedAt)}
                    </td>
                    <td>
                      {#if item.configBaseId === 100300}
                        <span class="locked-action">화폐 고정</span>
                      {:else}
                        <div class="manual-price">
                          <input
                            type="number"
                            min="0.01"
                            step="0.1"
                            inputmode="decimal"
                            placeholder={item.priceInCrystal == null ? "결정" : formatNumber(item.priceInCrystal, 2)}
                            value={manualPrices[item.configBaseId] ?? ""}
                            oninput={(event) => handleManualPriceInput(item.configBaseId, event)}
                            aria-label={`ID ${item.configBaseId} 수동 가격`}
                          />
                          <button type="button" onclick={() => saveManualPrice(item)}>적용</button>
                        </div>
                      {/if}
                    </td>
                    <td class="gold">{formatNumber(item.valueInCrystal)}</td>
                    <td>
                      {#if item.configBaseId === 100300}
                        <span class="locked-action">고정</span>
                      {:else}
                        <button class="ignore-button" type="button" onclick={() => toggleIgnored(item)}>
                          {item.ignored ? "무시 해제" : "무시"}
                        </button>
                      {/if}
                    </td>
                  </tr>
                {:else}
                  <tr>
                    <td class="empty-cell" colspan="8">현재 필터에 해당하는 아이템이 없습니다.</td>
                  </tr>
                {/each}
              </tbody>
              <tfoot>
                <tr>
                  <td>합계</td>
                  <td></td>
                  <td></td>
                  <td></td>
                  <td></td>
                  <td></td>
                  <td>{formatNumber(itemTotalValue)}</td>
                  <td></td>
                </tr>
              </tfoot>
            </table>
          </div>
        </section>
      {/if}
    </section>
  {/if}
</main>
