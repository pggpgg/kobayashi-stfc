import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import SimResults from './SimResults';
import type { SimulateStats, CrewRecommendation } from '../lib/api';

const baseProps = {
  simResult: null as SimulateStats | null,
  recommendations: [] as CrewRecommendation[],
  loadingSim: false,
  loadingOptimize: false,
  optimizeProgress: null as number | null,
  optimizeCrewsDone: null as number | null,
  optimizeTotalCrews: null as number | null,
};

describe('SimResults', () => {
  it('renders empty state message when no results', () => {
    render(<SimResults {...baseProps} />);
    expect(screen.getByText(/Run Sim for current crew/)).toBeTruthy();
  });

  it('shows "Running..." when loadingSim is true', () => {
    render(<SimResults {...baseProps} loadingSim={true} />);
    expect(screen.getByText('Running\u2026')).toBeTruthy();
  });

  it('shows optimization progress bar when loadingOptimize', () => {
    render(
      <SimResults
        {...baseProps}
        loadingOptimize={true}
        optimizeProgress={45}
        optimizeCrewsDone={90}
        optimizeTotalCrews={200}
      />,
    );
    expect(screen.getByText(/90 \/ 200 crews/)).toBeTruthy();
    expect(screen.getByText(/45%/)).toBeTruthy();
  });

  it('displays sim result stats', () => {
    const simResult: SimulateStats = {
      win_rate: 0.85,
      stall_rate: 0.1,
      loss_rate: 0.05,
      avg_hull_remaining: 0.42,
      n: 5000,
    };
    render(<SimResults {...baseProps} simResult={simResult} />);
    expect(screen.getByText('Win rate: 85.00%')).toBeTruthy();
    expect(screen.getByText('Stall rate: 10.00%')).toBeTruthy();
    expect(screen.getByText('Loss rate: 5.00%')).toBeTruthy();
    expect(screen.getByText('Avg hull remaining: 42.00%')).toBeTruthy();
    expect(screen.getByText('(n=5000)')).toBeTruthy();
  });

  it('displays 95% CI when present', () => {
    const simResult: SimulateStats = {
      win_rate: 0.85,
      stall_rate: 0.1,
      loss_rate: 0.05,
      avg_hull_remaining: 0.42,
      n: 5000,
      win_rate_95_ci: [0.83, 0.87],
    };
    render(<SimResults {...baseProps} simResult={simResult} />);
    expect(screen.getByText(/0\.830/)).toBeTruthy();
    expect(screen.getByText(/0\.870/)).toBeTruthy();
  });

  it('renders recommendation rows', () => {
    const recs: CrewRecommendation[] = [
      {
        captain: 'Kirk',
        bridge: 'Spock, Uhura',
        below_decks: 'Scotty, McCoy, Sulu',
        win_rate: 0.95,
        stall_rate: 0.03,
        loss_rate: 0.02,
        avg_hull_remaining: 0.6,
      },
      {
        captain: 'Picard',
        bridge: 'Riker, Data',
        below_decks: 'Worf, Crusher, LaForge',
        win_rate: 0.9,
        stall_rate: 0.05,
        loss_rate: 0.05,
        avg_hull_remaining: 0.55,
      },
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);
    expect(screen.getByText('Kirk')).toBeTruthy();
    expect(screen.getByText('Picard')).toBeTruthy();
    expect(screen.getByText('Spock, Uhura')).toBeTruthy();
    expect(screen.getByText('Select 2\u20135 rows to compare.')).toBeTruthy();
  });

  it('shows compare section when 2+ rows selected', () => {
    const recs: CrewRecommendation[] = [
      {
        captain: 'Kirk',
        bridge: 'Spock, Uhura',
        below_decks: 'Scotty, McCoy, Sulu',
        win_rate: 0.95,
        stall_rate: 0.03,
        loss_rate: 0.02,
        avg_hull_remaining: 0.6,
      },
      {
        captain: 'Picard',
        bridge: 'Riker, Data',
        below_decks: 'Worf, Crusher, LaForge',
        win_rate: 0.9,
        stall_rate: 0.05,
        loss_rate: 0.05,
        avg_hull_remaining: 0.55,
      },
    ];
    render(<SimResults {...baseProps} recommendations={recs} />);

    // Select both rows
    const checkboxes = screen.getAllByRole('checkbox');
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    expect(screen.getByText('Compare (delta)')).toBeTruthy();
  });

  it('limits selection to 5 rows', () => {
    const recs: CrewRecommendation[] = Array.from({ length: 7 }, (_, i) => ({
      captain: `Cap${i}`,
      bridge: `B${i}`,
      below_decks: `BD${i}`,
      win_rate: 0.9 - i * 0.05,
      stall_rate: 0.05,
      loss_rate: 0.05,
      avg_hull_remaining: 0.5,
    }));
    render(<SimResults {...baseProps} recommendations={recs} />);

    const checkboxes = screen.getAllByRole('checkbox');
    // Select first 5
    for (let i = 0; i < 5; i++) {
      fireEvent.click(checkboxes[i]);
    }
    // 6th click should not add (cap at 5)
    fireEvent.click(checkboxes[5]);
    // The 6th checkbox should not be checked
    expect((checkboxes[5] as HTMLInputElement).checked).toBe(false);
  });
});
