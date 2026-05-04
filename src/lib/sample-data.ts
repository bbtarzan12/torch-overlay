import type { CurrentRun, RunSummary } from "./types";

export const sampleCurrentRun: CurrentRun = {
  mapNameKo: "종식의 벽",
  difficulty: "7-0",
  elapsedSeconds: 18 * 60 + 34,
  crystal: 14,
  estimatedItemValue: 3.4,
  totalEstimatedValue: 17.4,
  unpricedItemCount: 2,
  itemCount: 8
};

export const sampleRuns: RunSummary[] = [
  makeRun(1, "끝없는 광야", "딥 스페이스", 11 * 60 + 42, 18, 2.8, 1, 9),
  makeRun(2, "잔잔한 빛의 강당", "아득한 8단계", 13 * 60 + 8, 15, 2.1, 2, 7),
  makeRun(3, "황야의 들판", "8-0", 10 * 60 + 51, 11, 0.7, 4, 6),
  makeRun(4, "슬픈 가락의 장벽", "7-2", 16 * 60 + 20, 9, 1.4, 3, 7),
  makeRun(5, "번개 산마루", "7-1", 15 * 60 + 44, 8, 0.5, 5, 6),
  makeRun(6, "종식의 벽", "7-0", 18 * 60 + 10, 14, 2.3, 2, 8),
  makeRun(7, "비극의 숲", "5단계", 9 * 60 + 38, 5, 0.2, 6, 6),
  makeRun(8, "왕의 허브", "7-0", 17 * 60 + 22, 12, 1.1, 3, 7),
  makeRun(9, "성스러운 정원", "7-1", 14 * 60 + 58, 13, 1.5, 2, 8),
  makeRun(10, "코어 광산", "딥 스페이스", 12 * 60 + 6, 17, 2.5, 1, 9),
  makeRun(11, "끝없는 광야", "7-0", 8 * 60 + 44, 6, 0.9, 3, 5),
  makeRun(12, "축원의 성전", "6단계", 11 * 60 + 48, 8, 0.6, 4, 6)
];

function makeRun(
  id: number,
  mapNameKo: string,
  difficulty: string,
  durationSeconds: number,
  crystal: number,
  estimatedItemValue: number,
  unpricedItemCount: number,
  itemCount: number
): RunSummary {
  return {
    id,
    mapNameKo,
    difficulty,
    durationSeconds,
    crystal,
    estimatedItemValue,
    totalEstimatedValue: crystal + estimatedItemValue,
    unpricedItemCount,
    itemCount
  };
}
