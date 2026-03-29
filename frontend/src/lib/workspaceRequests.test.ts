import { describe, it, expect } from 'vitest';
import { createEmptyCrew } from './types';
import {
  buildWorkspaceOptimizeStartBody,
  buildWorkspaceSimulateParams,
  buildOptimizeConstraintsFromForm,
} from './workspaceRequests';

describe('buildWorkspaceSimulateParams', () => {
  it('returns null without captain', () => {
    const crew = createEmptyCrew(50);
    expect(
      buildWorkspaceSimulateParams({
        shipId: 'Saladin',
        scenarioId: '2918121098',
        crew,
        simsPerCrew: 1000,
        shipTier: 1,
        shipLevel: 50,
      }),
    ).toBeNull();
  });

  it('uses simsPerCrew as num_sims (not a hardcoded default)', () => {
    const crew = createEmptyCrew(50);
    crew.captain = 'Kirk';
    const p = buildWorkspaceSimulateParams({
      shipId: 'Enterprise',
      scenarioId: 'hostile1',
      crew,
      simsPerCrew: 12_500,
      shipTier: 2,
      shipLevel: 45,
    });
    expect(p).not.toBeNull();
    expect(p!.num_sims).toBe(12_500);
    expect(p!.ship).toBe('Enterprise');
    expect(p!.hostile).toBe('hostile1');
    expect(p!.ship_tier).toBe(2);
    expect(p!.ship_level).toBe(45);
  });
});

describe('buildWorkspaceOptimizeStartBody', () => {
  it('uses simsPerCrew as sims to match single-sim control', () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: 'X',
      scenarioId: 'Y',
      simsPerCrew: 50_000,
      maxCandidates: 200,
      optimizerStrategy: 'genetic',
      prioritizeBelowDecksAbility: true,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: 'ordered',
      shipTier: 1,
      shipLevel: 50,
    });
    expect(body.sims).toBe(50_000);
    expect(body.strategy).toBe('genetic');
    expect(body.max_candidates).toBe(200);
    expect(body.prioritize_below_decks_ability).toBe(true);
    expect(body.below_decks_strategy).toBeUndefined();
  });

  it('includes heuristics_seeds when non-empty', () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: '',
      scenarioId: '',
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: 'exhaustive',
      prioritizeBelowDecksAbility: false,
      selectedSeeds: ['meta'],
      heuristicsOnly: true,
      belowDecksStrategy: 'exploration',
      shipTier: 1,
      shipLevel: 1,
    });
    expect(body.heuristics_seeds).toEqual(['meta']);
    expect(body.heuristics_only).toBe(true);
    expect(body.below_decks_strategy).toBe('exploration');
  });

  it('includes constraints when form fields are set', () => {
    const body = buildWorkspaceOptimizeStartBody({
      shipId: 'S',
      scenarioId: 'H',
      simsPerCrew: 1000,
      maxCandidates: null,
      optimizerStrategy: 'exhaustive',
      prioritizeBelowDecksAbility: false,
      selectedSeeds: [],
      heuristicsOnly: false,
      belowDecksStrategy: 'ordered',
      shipTier: 1,
      shipLevel: 50,
      optimizeConstraints: {
        mustIncludeComma: 'Alice, Bob',
        excludeComma: 'Eve',
        captainMust: 'Alice',
        bridgeMustComma: 'Bob',
        belowMustComma: 'Zed',
        groupsJson: '[{"officers":["X","Y"],"min_count":2}]',
      },
    });
    expect(body.constraints).toEqual({
      must_include: ['Alice', 'Bob'],
      exclude: ['Eve'],
      captain_must_be: 'Alice',
      bridge_must_include: ['Bob'],
      below_decks_must_include: ['Zed'],
      groups: [{ officers: ['X', 'Y'], min_count: 2 }],
    });
  });
});

describe('buildOptimizeConstraintsFromForm', () => {
  it('returns undefined when all empty', () => {
    expect(
      buildOptimizeConstraintsFromForm({
        mustIncludeComma: '',
        excludeComma: '  ',
        captainMust: '',
        bridgeMustComma: '',
        belowMustComma: '',
        groupsJson: '',
      }),
    ).toBeUndefined();
  });
});
