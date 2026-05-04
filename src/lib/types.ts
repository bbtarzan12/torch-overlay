export type ChartMode = "rate" | "cumulative";
export type DetailTab = "runs" | "items";
export type ItemFilter = "all" | "priced" | "unpriced" | "ignored";
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
  estimatedItemValue: number;
  totalEstimatedValue: number;
  unpricedItemCount: number;
  itemCount: number;
}

export interface CurrentRun {
  mapNameKo: string;
  difficulty: string;
  elapsedSeconds: number;
  crystal: number;
  estimatedItemValue: number;
  totalEstimatedValue: number;
  unpricedItemCount: number;
  itemCount: number;
}

export interface LootSummary {
  configBaseId: number;
  quantity: number;
  priceInCrystal?: number | null;
  valueInCrystal: number;
}

export interface ItemValuationRow {
  configBaseId: number;
  itemNameKo?: string | null;
  quantity: number;
  ignored: boolean;
  priceInCrystal?: number | null;
  priceSource: string;
  observedAt?: string | null;
  observationCount: number;
  valueInCrystal: number;
  unpriced: boolean;
}

export interface TrackerSnapshot {
  currentRun: CurrentRun;
  runs: RunSummary[];
  totalCrystal: number;
  estimatedItemValue: number;
  totalEstimatedValue: number;
  averageRate: number;
  averagePerRun: number;
  unpricedItemCount: number;
  knownPriceCount: number;
  recentLoot: LootSummary[];
  items: ItemValuationRow[];
}

export interface UpdateInfo {
  state: UpdateState;
  version?: string;
  notes?: string;
  progress?: number;
  message?: string;
}
