import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import OptimizePanel from './OptimizePanel';
import type { CrewState } from '../lib/types';

const emptyCrew: CrewState = {
  captain: null,
  bridge: [null, null],
  belowDeck: [null, null, null],
};

const baseProps = {
  collapsed: false,
  onToggleCollapsed: vi.fn(),
  crew: emptyCrew,
  loadingOptimize: false,
  optimizeCrewsDone: null as number | null,
  optimizeTotalCrews: null as number | null,
  maxCandidates: null as number | null,
  onMaxCandidatesChange: vi.fn(),
  prioritizeBelowDecksAbility: false,
  onPrioritizeBelowDecksAbilityChange: vi.fn(),
  availableSeeds: [] as string[],
  selectedSeeds: [] as string[],
  onSelectedSeedsChange: vi.fn(),
  heuristicsOnly: false,
  onHeuristicsOnlyChange: vi.fn(),
  belowDecksStrategy: 'ordered' as const,
  onBelowDecksStrategyChange: vi.fn(),
};

describe('OptimizePanel', () => {
  it('renders expanded panel with Strategy heading', () => {
    render(<OptimizePanel {...baseProps} />);
    expect(screen.getByText('Strategy')).toBeTruthy();
  });

  it('renders collapsed state with expand button', () => {
    render(<OptimizePanel {...baseProps} collapsed={true} />);
    expect(screen.getByLabelText('Expand panel')).toBeTruthy();
  });

  it('calls onToggleCollapsed when collapse button clicked', () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onToggleCollapsed={fn} />);
    fireEvent.click(screen.getByLabelText('Collapse panel'));
    expect(fn).toHaveBeenCalledOnce();
  });

  it('shows heuristic seeds when available', () => {
    render(
      <OptimizePanel
        {...baseProps}
        availableSeeds={['swarm-crews', 'borg-crews']}
      />,
    );
    expect(screen.getByText('Heuristics seeds')).toBeTruthy();
    expect(screen.getByText('swarm-crews')).toBeTruthy();
    expect(screen.getByText('borg-crews')).toBeTruthy();
  });

  it('does not show heuristic seeds when empty', () => {
    render(<OptimizePanel {...baseProps} availableSeeds={[]} />);
    expect(screen.queryByText('Heuristics seeds')).toBeNull();
  });

  it('shows below-decks strategy when seeds are selected', () => {
    render(
      <OptimizePanel
        {...baseProps}
        availableSeeds={['swarm-crews']}
        selectedSeeds={['swarm-crews']}
      />,
    );
    expect(screen.getByText('Below-decks strategy')).toBeTruthy();
    expect(screen.getByText(/Heuristics only/)).toBeTruthy();
  });

  it('calls onMaxCandidatesChange when input changes', () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onMaxCandidatesChange={fn} />);
    const input = screen.getByPlaceholderText('No limit');
    fireEvent.change(input, { target: { value: '500' } });
    expect(fn).toHaveBeenCalledWith(500);
  });

  it('clamps max candidates to 2,000,000', () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} onMaxCandidatesChange={fn} />);
    const input = screen.getByPlaceholderText('No limit');
    fireEvent.change(input, { target: { value: '9999999' } });
    expect(fn).toHaveBeenCalledWith(2_000_000);
  });

  it('sets maxCandidates to null when input cleared', () => {
    const fn = vi.fn();
    render(<OptimizePanel {...baseProps} maxCandidates={100} onMaxCandidatesChange={fn} />);
    const input = screen.getByPlaceholderText('No limit');
    fireEvent.change(input, { target: { value: '' } });
    expect(fn).toHaveBeenCalledWith(null);
  });

  it('toggles prioritize below-decks checkbox', () => {
    const fn = vi.fn();
    render(
      <OptimizePanel
        {...baseProps}
        onPrioritizeBelowDecksAbilityChange={fn}
      />,
    );
    const checkbox = screen.getByRole('checkbox', {
      name: /Only below-decks officers with ability/,
    });
    fireEvent.click(checkbox);
    expect(fn).toHaveBeenCalledWith(true);
  });

  it('shows live status during optimization', () => {
    render(
      <OptimizePanel
        {...baseProps}
        loadingOptimize={true}
        optimizeCrewsDone={50}
        optimizeTotalCrews={200}
      />,
    );
    expect(screen.getByText('Live status: 50 / 200 crews')).toBeTruthy();
  });
});
