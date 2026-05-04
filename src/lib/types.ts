export type ChartMode = "rate" | "cumulative";
export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "not_available"
  | "downloading"
  | "ready_to_install"
  | "installing"
  | "error";

export interface RunSummary {
  id: number;
  mapNameKo: string;
  difficulty: string;
  durationSeconds: number;
  crystal: number;
}

export interface CurrentRun {
  mapNameKo: string;
  difficulty: string;
  elapsedSeconds: number;
  crystal: number;
}

export interface TrackerSnapshot {
  currentRun: CurrentRun;
  runs: RunSummary[];
  totalCrystal: number;
  averageRate: number;
  averagePerRun: number;
}

export interface UpdateInfo {
  state: UpdateState;
  version?: string;
  notes?: string;
  progress?: number;
  message?: string;
}

