import type { CrewState } from './types';
import type { OptimizerStrategyType } from './api';

/** Params for POST /api/simulate from workspace UI (single-crew Monte Carlo). */
export function buildWorkspaceSimulateParams(args: {
  shipId: string;
  scenarioId: string;
  crew: CrewState;
  simsPerCrew: number;
  shipTier: number;
  shipLevel: number;
}): {
  ship: string;
  hostile: string;
  crew: { captain: string; bridge: (string | null)[]; below_deck: (string | null)[] };
  num_sims: number;
  ship_tier: number;
  ship_level: number;
} | null {
  if (!args.crew.captain) return null;
  return {
    ship: args.shipId || 'Saladin',
    hostile: args.scenarioId || '2918121098',
    crew: {
      captain: args.crew.captain,
      bridge: args.crew.bridge,
      below_deck: args.crew.belowDeck,
    },
    num_sims: args.simsPerCrew,
    ship_tier: args.shipTier,
    ship_level: args.shipLevel,
  };
}

/** Body for POST /api/optimize/start from workspace UI (mirrors handleRunOptimize). */
export function buildWorkspaceOptimizeStartBody(args: {
  shipId: string;
  scenarioId: string;
  simsPerCrew: number;
  maxCandidates: number | null;
  optimizerStrategy: OptimizerStrategyType;
  prioritizeBelowDecksAbility: boolean;
  selectedSeeds: string[];
  heuristicsOnly: boolean;
  belowDecksStrategy: 'ordered' | 'exploration';
  shipTier: number;
  shipLevel: number;
}) {
  return {
    ship: args.shipId || 'Saladin',
    hostile: args.scenarioId || '2918121098',
    sims: args.simsPerCrew,
    max_candidates: args.maxCandidates ?? undefined,
    strategy: args.optimizerStrategy,
    prioritize_below_decks_ability: args.prioritizeBelowDecksAbility || undefined,
    heuristics_seeds: args.selectedSeeds.length > 0 ? args.selectedSeeds : undefined,
    heuristics_only: args.heuristicsOnly || undefined,
    below_decks_strategy:
      args.belowDecksStrategy !== 'ordered' ? args.belowDecksStrategy : undefined,
    ship_tier: args.shipTier,
    ship_level: args.shipLevel,
  };
}
