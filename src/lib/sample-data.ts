import type { CurrentRun, RunSummary } from "./types";

export const sampleCurrentRun: CurrentRun = {
  mapNameKo: "종식의 벽",
  difficulty: "7-0",
  elapsedSeconds: 18 * 60 + 34,
  crystal: 14
};

export const sampleRuns: RunSummary[] = [
  { id: 1, mapNameKo: "끝없는 광야", difficulty: "딥 스페이스", durationSeconds: 11 * 60 + 42, crystal: 18 },
  { id: 2, mapNameKo: "잔잔한 빛의 강당", difficulty: "아득한 8단계", durationSeconds: 13 * 60 + 8, crystal: 15 },
  { id: 3, mapNameKo: "황야의 들판", difficulty: "8-0", durationSeconds: 10 * 60 + 51, crystal: 11 },
  { id: 4, mapNameKo: "슬픈 가락의 장벽", difficulty: "7-2", durationSeconds: 16 * 60 + 20, crystal: 9 },
  { id: 5, mapNameKo: "번개 산마루", difficulty: "7-1", durationSeconds: 15 * 60 + 44, crystal: 8 },
  { id: 6, mapNameKo: "종식의 벽", difficulty: "7-0", durationSeconds: 18 * 60 + 10, crystal: 14 },
  { id: 7, mapNameKo: "비극의 숲", difficulty: "5단계", durationSeconds: 9 * 60 + 38, crystal: 5 },
  { id: 8, mapNameKo: "왕의 허브", difficulty: "7-0", durationSeconds: 17 * 60 + 22, crystal: 12 },
  { id: 9, mapNameKo: "성스러운 정원", difficulty: "7-1", durationSeconds: 14 * 60 + 58, crystal: 13 },
  { id: 10, mapNameKo: "코어 광산", difficulty: "딥 스페이스", durationSeconds: 12 * 60 + 6, crystal: 17 },
  { id: 11, mapNameKo: "끝없는 광야", difficulty: "7-0", durationSeconds: 8 * 60 + 44, crystal: 6 },
  { id: 12, mapNameKo: "축원의 성전", difficulty: "6단계", durationSeconds: 11 * 60 + 48, crystal: 8 }
];

