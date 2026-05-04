<script lang="ts">
  import { onMount, tick } from "svelte";
  import { buildChartSeries } from "./lib/chart";
  import { formatDuration, formatNumber, formatRate } from "./lib/format";
  import { sampleCurrentRun, sampleRuns } from "./lib/sample-data";
  import { checkForUpdate, loadTrackerSnapshot, setClickableRects, setPositionLocked } from "./lib/tauri";
  import type { ChartMode, CurrentRun, RunSummary, UpdateInfo } from "./lib/types";

  let currentRun: CurrentRun = sampleCurrentRun;
  let runs: RunSummary[] = sampleRuns;
  let detailsOpen = true;
  let positionLocked = true;
  let opacity = 92;
  let chartMode: ChartMode = "rate";
  let updateInfo: UpdateInfo = { state: "idle" };

  let shellElement: HTMLElement;
  let barElement: HTMLElement;
  let detailsElement: HTMLElement;

  $: totalCrystal = runs.reduce((sum, run) => sum + run.crystal, 0);
  $: totalSeconds = runs.reduce((sum, run) => sum + run.durationSeconds, 0);
  $: averageSeconds = runs.length > 0 ? totalSeconds / runs.length : 0;
  $: averageCrystal = runs.length > 0 ? totalCrystal / runs.length : 0;
  $: averageRate = totalSeconds > 0 ? (totalCrystal / totalSeconds) * 3600 : 0;
  $: currentRate = formatRate(currentRun.crystal, currentRun.elapsedSeconds);
  $: chartSeries = buildChartSeries(runs, chartMode);
  $: chartTitle = chartMode === "cumulative" ? "누적 결정" : "시간 당 결정";

  onMount(() => {
    let disposed = false;
    let updateTimer: number | undefined;

    void (async () => {
      const snapshot = await loadTrackerSnapshot();

      if (disposed) {
        return;
      }

      if (snapshot) {
        currentRun = snapshot.currentRun;
        runs = snapshot.runs.length > 0 ? snapshot.runs : sampleRuns;
      }

      await syncClickableRects();
      window.addEventListener("resize", syncClickableRects);

      updateTimer = window.setTimeout(async () => {
        updateInfo = { state: "checking" };
        updateInfo = await checkForUpdate();
        await syncClickableRects();
      }, 10_000);
    })();

    return () => {
      disposed = true;
      if (updateTimer) {
        window.clearTimeout(updateTimer);
      }
      window.removeEventListener("resize", syncClickableRects);
    };
  });

  async function togglePositionLocked() {
    positionLocked = !positionLocked;
    await setPositionLocked(positionLocked);
    await syncClickableRects();
  }

  async function toggleDetails() {
    detailsOpen = !detailsOpen;
    await syncClickableRects();
  }

  async function setMode(mode: ChartMode) {
    chartMode = mode;
    await syncClickableRects();
  }

  async function handleUpdateClick() {
    updateInfo = { state: "checking" };
    updateInfo = await checkForUpdate();
  }

  function resetSession() {
    runs = [];
    currentRun = {
      ...currentRun,
      elapsedSeconds: 0,
      crystal: 0
    };
  }

  async function syncClickableRects() {
    await tick();

    const controls = shellElement?.querySelectorAll<HTMLElement>(
      "button, input, .opacity-control"
    );

    if (!controls) {
      return;
    }

    await setClickableRects([...controls].map((element) => element.getBoundingClientRect()));
  }
</script>

<main
  bind:this={shellElement}
  class="tracker-shell"
  data-position-locked={positionLocked}
  style={`--app-opacity: ${opacity / 100};`}
>
  <section bind:this={barElement} class="tracker-bar" aria-label="TLI 트래커 상단 바">
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
        <span class="highlight">+{currentRun.crystal} 결정</span>
        <span class="separator">·</span>
        <span class="metric">{currentRate}/h</span>
      </span>
    </div>

    <div class="segment reset-stats" data-tauri-drag-region={positionLocked ? undefined : ""}>
      <span class="label">누적</span>
      <span class="value">
        <span class="highlight">{totalCrystal} 결정</span>
        <span class="separator">·</span>
        <span class="metric">평균 {formatNumber(averageRate)}/h</span>
        <span class="separator">·</span>
        <span class="metric muted">{formatNumber(averageCrystal)}/런</span>
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
          aria-label="투명도"
        />
        <span class="opacity-value">{opacity}%</span>
      </label>

      {#if updateInfo.state === "available"}
        <button class="update-button" type="button" onclick={handleUpdateClick}>업데이트</button>
      {:else if updateInfo.state === "checking"}
        <span class="update-status">확인중</span>
      {/if}
    </div>

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
                <col class="rate-col" />
              </colgroup>
              <thead>
                <tr>
                  <th>맵</th>
                  <th>난이도</th>
                  <th>시간</th>
                  <th>결정</th>
                  <th>결정/h</th>
                </tr>
              </thead>
              <tbody>
                {#each runs as run}
                  <tr>
                    <td>{run.mapNameKo}</td>
                    <td class="muted">{run.difficulty}</td>
                    <td>{formatDuration(run.durationSeconds)}</td>
                    <td class="gold">{run.crystal}</td>
                    <td class="gold">{formatRate(run.crystal, run.durationSeconds)}</td>
                  </tr>
                {/each}
              </tbody>
              <tfoot>
                <tr>
                  <td>평균</td>
                  <td></td>
                  <td>{formatDuration(averageSeconds)}</td>
                  <td>{formatNumber(averageCrystal)}</td>
                  <td>{formatNumber(averageRate)}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        </section>
      </div>
    </section>
  {/if}
</main>
