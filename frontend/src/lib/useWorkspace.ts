import { useState, useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import {
  createEmptyCrew,
  createEmptyPins,
  belowDeckSlotCount,
  type CrewState,
  type PinsState,
} from './types';
import {
  simulate,
  optimizeStart,
  getOptimizeStatus,
  savePreset,
  getOptimizeEstimate,
  fetchHeuristics,
  formatApiError,
  type SimulateStats,
  type OptimizeEstimate,
  type CrewRecommendation,
  type Preset,
} from './api';
import { useProfile } from '../contexts/ProfileContext';

const POLL_INTERVAL_MS = 350;

export function useWorkspace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { activeProfileId } = useProfile();

  // Scenario state
  const [shipLevel, setShipLevel] = useState(50);
  const [shipId, setShipId] = useState('');
  const [scenarioId, setScenarioId] = useState('');

  // Crew state
  const [crew, setCrew] = useState<CrewState>(() => createEmptyCrew(50));
  const [pins, setPins] = useState<PinsState>(() => createEmptyPins(50));

  // Simulation state
  const [simResult, setSimResult] = useState<SimulateStats | null>(null);
  const [loadingSim, setLoadingSim] = useState(false);

  // Optimization state
  const [recommendations, setRecommendations] = useState<CrewRecommendation[]>([]);
  const [loadingOptimize, setLoadingOptimize] = useState(false);
  const [optimizeProgress, setOptimizeProgress] = useState<number | null>(null);
  const [optimizeCrewsDone, setOptimizeCrewsDone] = useState<number | null>(null);
  const [optimizeTotalCrews, setOptimizeTotalCrews] = useState<number | null>(null);
  const [estimate, setEstimate] = useState<OptimizeEstimate | null>(null);
  const [lastOptimizeDurationMs, setLastOptimizeDurationMs] = useState<number | null>(null);

  // Optimization parameters
  const [simsPerCrew, setSimsPerCrew] = useState(5000);
  const [maxCandidates, setMaxCandidates] = useState<number | null>(100);
  const [prioritizeBelowDecksAbility, setPrioritizeBelowDecksAbility] = useState(false);

  // Heuristics state
  const [availableSeeds, setAvailableSeeds] = useState<string[]>([]);
  const [selectedSeeds, setSelectedSeeds] = useState<string[]>([]);
  const [heuristicsOnly, setHeuristicsOnly] = useState(false);
  const [belowDecksStrategy, setBelowDecksStrategy] = useState<'ordered' | 'exploration'>('ordered');

  // Preset saving state
  const [showSavePreset, setShowSavePreset] = useState(false);
  const [savePresetName, setSavePresetName] = useState('');
  const [savingPreset, setSavingPreset] = useState(false);

  // UI state
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Polling ref
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Load preset from location state
  useEffect(() => {
    const preset = (location.state as { preset?: Preset } | null)?.preset;
    if (preset) {
      setShipId(preset.ship);
      setScenarioId(preset.scenario);
      const c = preset.crew;
      const bridge = c?.bridge ?? [];
      setCrew({
        captain: c?.captain ?? null,
        bridge: [bridge[0] ?? null, bridge[1] ?? null],
        belowDeck: c?.below_deck ?? [],
      });
      navigate('.', { replace: true, state: {} });
    }
  }, [location.state, navigate]);

  // Fetch optimize estimate when parameters change
  useEffect(() => {
    const ship = shipId || 'Saladin';
    const hostile = scenarioId || 'Explorer_30';
    if (!ship || !hostile) {
      setEstimate(null);
      return;
    }
    let cancelled = false;
    getOptimizeEstimate(
      {
        ship,
        hostile,
        sims: simsPerCrew,
        max_candidates: maxCandidates ?? undefined,
        prioritize_below_decks_ability: prioritizeBelowDecksAbility || undefined,
      },
      activeProfileId,
    )
      .then((data) => {
        if (!cancelled) setEstimate(data);
      })
      .catch(() => {
        if (!cancelled) setEstimate(null);
      });
    return () => { cancelled = true; };
  }, [shipId, scenarioId, simsPerCrew, maxCandidates, prioritizeBelowDecksAbility, activeProfileId]);

  // Fetch available heuristic seeds
  useEffect(() => {
    fetchHeuristics().then(setAvailableSeeds).catch(() => setAvailableSeeds([]));
  }, []);

  // Sync crew/pins with ship level changes
  useEffect(() => {
    const n = belowDeckSlotCount(shipLevel);
    setCrew((c) => {
      const next = [...c.belowDeck];
      while (next.length < n) next.push(null);
      if (next.length > n) next.length = n;
      return { ...c, belowDeck: next };
    });
    setPins((p) => {
      const next = [...p.belowDeck];
      while (next.length < n) next.push(false);
      if (next.length > n) next.length = n;
      return { ...p, belowDeck: next };
    });
  }, [shipLevel]);

  // Cleanup polling on unmount
  useEffect(() => {
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, []);

  // Handle running a simulation
  const handleRunSim = async () => {
    if (!crew.captain) {
      setError('Select a captain first');
      return;
    }
    setError(null);
    setLoadingSim(true);
    try {
      const res = await simulate(
        {
          ship: shipId || 'Saladin',
          hostile: scenarioId || 'Explorer_30',
          crew: {
            captain: crew.captain,
            bridge: crew.bridge,
            below_deck: crew.belowDeck,
          },
          num_sims: 5000,
        },
        activeProfileId,
      );
      setSimResult(res.stats);
      setRecommendations([]);
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setLoadingSim(false);
    }
  };

  // Handle running optimization
  const handleRunOptimize = async () => {
    setError(null);
    setLoadingOptimize(true);
    setLastOptimizeDurationMs(null);
    setOptimizeProgress(0);
    setOptimizeCrewsDone(0);
    setOptimizeTotalCrews(null);
    try {
      const { job_id } = await optimizeStart(
        {
          ship: shipId || 'Saladin',
          hostile: scenarioId || 'Explorer_30',
          sims: simsPerCrew,
          max_candidates: maxCandidates ?? undefined,
          prioritize_below_decks_ability: prioritizeBelowDecksAbility || undefined,
          heuristics_seeds: selectedSeeds.length > 0 ? selectedSeeds : undefined,
          heuristics_only: heuristicsOnly || undefined,
          below_decks_strategy: belowDecksStrategy !== 'ordered' ? belowDecksStrategy : undefined,
        },
        activeProfileId,
      );
      const poll = () => {
        getOptimizeStatus(job_id, activeProfileId)
          .then((status) => {
            if (status.progress != null) setOptimizeProgress(status.progress);
            if (status.crews_done != null) setOptimizeCrewsDone(status.crews_done);
            if (status.total_crews != null) setOptimizeTotalCrews(status.total_crews);
            if (status.status === 'done' && status.result) {
              if (pollIntervalRef.current) {
                clearInterval(pollIntervalRef.current);
                pollIntervalRef.current = null;
              }
              setRecommendations(status.result.recommendations ?? []);
              setSimResult(null);
              if (status.result.duration_ms != null) setLastOptimizeDurationMs(status.result.duration_ms);
              setLoadingOptimize(false);
              setOptimizeProgress(null);
              setOptimizeCrewsDone(null);
              setOptimizeTotalCrews(null);
            } else if (status.status === 'error') {
              if (pollIntervalRef.current) {
                clearInterval(pollIntervalRef.current);
                pollIntervalRef.current = null;
              }
              setError(status.error ?? 'Optimization failed');
              setLoadingOptimize(false);
              setOptimizeProgress(null);
              setOptimizeCrewsDone(null);
              setOptimizeTotalCrews(null);
            }
          })
          .catch((e) => {
            if (pollIntervalRef.current) {
              clearInterval(pollIntervalRef.current);
              pollIntervalRef.current = null;
            }
            setError(formatApiError(e));
            setLoadingOptimize(false);
            setOptimizeProgress(null);
            setOptimizeCrewsDone(null);
            setOptimizeTotalCrews(null);
          });
      };
      poll();
      pollIntervalRef.current = setInterval(poll, POLL_INTERVAL_MS);
    } catch (e) {
      setError(formatApiError(e));
      setLoadingOptimize(false);
      setOptimizeProgress(null);
      setOptimizeCrewsDone(null);
      setOptimizeTotalCrews(null);
    }
  };

  // Handle saving a preset
  const handleSavePreset = async () => {
    setError(null);
    setSavingPreset(true);
    try {
      await savePreset(
        {
          name: savePresetName || 'Unnamed',
          ship: shipId || 'Saladin',
          scenario: scenarioId || 'Explorer_30',
          crew: {
            captain: crew.captain,
            bridge: crew.bridge,
            below_deck: crew.belowDeck,
          },
        },
        activeProfileId,
      );
      setShowSavePreset(false);
      setSavePresetName('');
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setSavingPreset(false);
    }
  };

  return {
    // Scenario
    shipId,
    setShipId,
    scenarioId,
    setScenarioId,
    shipLevel,
    setShipLevel,
    // Crew
    crew,
    setCrew,
    pins,
    setPins,
    // Simulation
    simResult,
    loadingSim,
    handleRunSim,
    // Optimization
    recommendations,
    loadingOptimize,
    handleRunOptimize,
    optimizeProgress,
    optimizeCrewsDone,
    optimizeTotalCrews,
    estimate,
    lastOptimizeDurationMs,
    // Optimization parameters
    simsPerCrew,
    setSimsPerCrew,
    maxCandidates,
    setMaxCandidates,
    prioritizeBelowDecksAbility,
    setPrioritizeBelowDecksAbility,
    // Heuristics
    availableSeeds,
    selectedSeeds,
    setSelectedSeeds,
    heuristicsOnly,
    setHeuristicsOnly,
    belowDecksStrategy,
    setBelowDecksStrategy,
    // Presets
    showSavePreset,
    setShowSavePreset,
    savePresetName,
    setSavePresetName,
    savingPreset,
    handleSavePreset,
    // UI
    rightPanelCollapsed,
    setRightPanelCollapsed,
    error,
    setError,
  };
}
