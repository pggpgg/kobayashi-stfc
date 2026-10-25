import {
  type Dispatch,
  type SetStateAction,
  useCallback,
  useMemo,
  useState,
} from "react";
import type { OptimizerStrategyType } from "./api";
import type { BelowDecksPoolMode } from "./optimizeWarmStart";

export interface SeedSettings {
  selectedSeeds: string[];
  heuristicsOnly: boolean;
  fastDiscovery: boolean;
  belowDecksStrategy: "ordered" | "exploration";
}
export interface StrategySettings {
  optimizerStrategy: OptimizerStrategyType;
  tieredScoutSims: number | null;
  tieredTopK: number | null;
  tieredRandomExplorationPct: number | null;
  enableLearnedPairPrior: boolean;
  localRefinement: boolean;
}
export interface DiversitySettings {
  noveltyLambdaText: string;
  noveltyDiverseTopText: string;
  noveltyPoolText: string;
  noveltyHistoryAnchors: boolean;
}
export interface ConstraintSettings {
  optimizeMustInclude: string;
  optimizeExclude: string;
  optimizeCaptainMust: string;
  optimizeBridgeMust: string;
  optimizeBelowMust: string;
  /** Saved representation and existing API conversion remain compatible. */
  optimizeGroupsJson: string;
}
export interface ChainSettings {
  chainGrindEnabled: boolean;
  chainKillsTarget: number;
  chainSecondary: "min_hull_damage" | "max_loot_per_hull_proxy";
}
export interface SearchScopeSettings {
  maxCandidates: number | null;
  belowDecksPoolMode: BelowDecksPoolMode;
}
export interface OptimizerSettings
  extends SeedSettings,
    StrategySettings,
    DiversitySettings,
    ConstraintSettings,
    ChainSettings,
    SearchScopeSettings {}

export type UpdateOptimizerSettings = (
  patch: Partial<OptimizerSettings>,
) => void;

export function defaultOptimizerSettings(): OptimizerSettings {
  return {
    maxCandidates: 100,
    belowDecksPoolMode: "strict",
    optimizerStrategy: "tiered",
    tieredScoutSims: null,
    tieredTopK: null,
    tieredRandomExplorationPct: null,
    enableLearnedPairPrior: true,
    localRefinement: false,
    selectedSeeds: [],
    heuristicsOnly: false,
    fastDiscovery: false,
    belowDecksStrategy: "ordered",
    noveltyLambdaText: "",
    noveltyDiverseTopText: "",
    noveltyPoolText: "",
    noveltyHistoryAnchors: false,
    optimizeMustInclude: "",
    optimizeExclude: "",
    optimizeCaptainMust: "",
    optimizeBridgeMust: "",
    optimizeBelowMust: "",
    optimizeGroupsJson: "",
    chainGrindEnabled: false,
    chainKillsTarget: 3,
    chainSecondary: "min_hull_damage",
  };
}

type FieldSetters = {
  [K in keyof OptimizerSettings]: Dispatch<
    SetStateAction<OptimizerSettings[K]>
  >;
};

export function useOptimizerSettings() {
  const [settings, setSettings] = useState(defaultOptimizerSettings);
  const update = useCallback<UpdateOptimizerSettings>(
    (patch) => setSettings((previous) => ({ ...previous, ...patch })),
    [],
  );
  // Existing workspace actions can keep their field setters. New panel settings
  // use the typed patch callback and need no additional prop/setter plumbing.
  const setters = useMemo(() => {
    function forField<K extends keyof OptimizerSettings>(
      key: K,
    ): Dispatch<SetStateAction<OptimizerSettings[K]>> {
      return (value) =>
        setSettings((previous) => ({
          ...previous,
          [key]: typeof value === "function" ? value(previous[key]) : value,
        }));
    }
    return Object.fromEntries(
      (
        Object.keys(defaultOptimizerSettings()) as (keyof OptimizerSettings)[]
      ).map((key) => [key, forField(key)]),
    ) as FieldSetters;
  }, []);
  return { settings, update, setters };
}
