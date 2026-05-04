import type { ChartMode, RunSummary } from "./types";

const chartWidth = 1200;
const chartHeight = 126;
const left = 54;
const right = 30;
const top = 16;
const bottom = 24;

export interface ChartPoint {
  x: number;
  y: number;
  value: number;
}

export interface ChartSeries {
  points: ChartPoint[];
  linePath: string;
  areaPath: string;
  minLabel: string;
  maxLabel: string;
  endLabel: string;
}

export function buildChartSeries(runs: RunSummary[], mode: ChartMode): ChartSeries {
  const values = mode === "cumulative" ? cumulativeValues(runs) : runs.map(ratePerHour);
  const maxValue = Math.max(...values, mode === "cumulative" ? 1 : 10);
  const minValue = 0;
  const plotWidth = chartWidth - left - right;
  const plotHeight = chartHeight - top - bottom;

  const points = values.map((value, index) => {
    const ratioX = values.length <= 1 ? 0 : index / (values.length - 1);
    const ratioY = maxValue === minValue ? 0 : (value - minValue) / (maxValue - minValue);

    return {
      x: Math.round(left + ratioX * plotWidth),
      y: Math.round(top + (1 - ratioY) * plotHeight),
      value
    };
  });

  const linePath = points.map((point, index) => `${index === 0 ? "M" : "L"}${point.x} ${point.y}`).join(" ");
  const baseY = top + plotHeight;
  const areaPath =
    points.length === 0
      ? ""
      : `${linePath} L${points[points.length - 1].x} ${baseY} L${points[0].x} ${baseY} Z`;

  return {
    points,
    linePath,
    areaPath,
    minLabel: mode === "cumulative" ? "0" : "0/h",
    maxLabel: mode === "cumulative" ? Math.round(maxValue).toString() : `${Math.round(maxValue)}/h`,
    endLabel: runs.length.toString()
  };
}

function ratePerHour(run: RunSummary): number {
  if (run.durationSeconds <= 0) {
    return 0;
  }

  return (run.crystal / run.durationSeconds) * 3600;
}

function cumulativeValues(runs: RunSummary[]): number[] {
  let total = 0;

  return runs.map((run) => {
    total += run.crystal;
    return total;
  });
}

