import { useState, useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import WorkspaceHeader from '../components/WorkspaceHeader';
import CrewBuilder from '../components/CrewBuilder';
import OptimizePanel from '../components/OptimizePanel';
import SimResults from '../components/SimResults';
import {
  createEmptyCrew,
  createEmptyPins,
  belowDeckSlotCount,
  type CrewState,
  type PinsState,
} from '../lib/types';
import { simulate, optimizeStart, getOptimizeStatus, savePreset, getOptimizeEstimate, formatApiError } from '../lib/api';
import type { SimulateStats, OptimizeEstimate } from '../lib/api';
import type { CrewRecommendation } from '../lib/api';
import type { Preset } from '../lib/api';

const POLL_INTERVAL_MS = 350;

export default function Workspace() {
  const location = useLocation();
  const navigate = useNavigate();
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(false);
  const [shipLevel, setShipLevel] = useState(50);
  const [shipId, setShipId] = useState('');
  const [scenarioId, setScenarioId] = useState('');
  const [crew, setCrew] = useState<CrewState>(() => createEmptyCrew(50));
  const [pins, setPins] = useState<PinsState>(() => createEmptyPins(50));
  const [simResult, setSimResult] = useState<SimulateStats | null>(null);
  const [recommendations, setRecommendations] = useState<CrewRecommendation[]>([]);
  const [loadingSim, setLoadingSim] = useState(false);
  const [loadingOptimize, setLoadingOptimize] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savePresetName, setSavePresetName] = useState('');
  const [showSavePreset, setShowSavePreset] = useState(false);
  const [savingPreset, setSavingPreset] = useState(false);
  const [simsPerCrew, setSimsPerCrew] = useState(5000);
  const [maxCandidates, setMaxCandidates] = useState<number | null>(null);
  const [prioritizeBelowDecksAbility, setPrioritizeBelowDecksAbility] = useState(false);
  const [estimate, setEstimate] = useState<OptimizeEstimate | null>(null);
  const [lastOptimizeDurationMs, setLastOptimizeDurationMs] = useState<number | null>(null);
  const [optimizeProgress, setOptimizeProgress] = useState<number | null>(null);
  const [optimizeCrewsDone, setOptimizeCrewsDone] = useState<number | null>(null);
  const [optimizeTotalCrews, setOptimizeTotalCrews] = useState<number | null>(null);
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

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

  useEffect(() => {
    const ship = shipId || 'Saladin';
    const hostile = scenarioId || 'Explorer_30';
    if (!ship || !hostile) {
      setEstimate(null);
      return;
    }
    let cancelled = false;
    getOptimizeEstimate({
      ship,
      hostile,
      sims: simsPerCrew,
      max_candidates: maxCandidates ?? undefined,
      prioritize_below_decks_ability: prioritizeBelowDecksAbility || undefined,
    })
      .then((data) => {
        if (!cancelled) setEstimate(data);
      })
      .catch(() => {
        if (!cancelled) setEstimate(null);
      });
    return () => { cancelled = true; };
  }, [shipId, scenarioId, simsPerCrew, maxCandidates, prioritizeBelowDecksAbility]);

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

  const handleRunSim = async () => {
    if (!crew.captain) {
      setError('Select a captain first');
      return;
    }
    setError(null);
    setLoadingSim(true);
    try {
      const res = await simulate({
        ship: shipId || 'Saladin',
        hostile: scenarioId || 'Explorer_30',
        crew: {
          captain: crew.captain,
          bridge: crew.bridge,
          below_deck: crew.belowDeck,
        },
        num_sims: 5000,
      });
      setSimResult(res.stats);
      setRecommendations([]);
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setLoadingSim(false);
    }
  };

  const handleRunOptimize = async () => {
    setError(null);
    setLoadingOptimize(true);
    setLastOptimizeDurationMs(null);
    setOptimizeProgress(0);
    setOptimizeCrewsDone(0);
    setOptimizeTotalCrews(null);
    try {
      const { job_id } = await optimizeStart({
        ship: shipId || 'Saladin',
        hostile: scenarioId || 'Explorer_30',
        sims: simsPerCrew,
        max_candidates: maxCandidates ?? undefined,
        prioritize_below_decks_ability: prioritizeBelowDecksAbility || undefined,
      });
      const poll = () => {
        getOptimizeStatus(job_id)
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

  useEffect(() => {
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, []);

  const handleSavePreset = async () => {
    setError(null);
    setSavingPreset(true);
    try {
      await savePreset({
        name: savePresetName || 'Unnamed',
        ship: shipId || 'Saladin',
        scenario: scenarioId || 'Explorer_30',
        crew: {
          captain: crew.captain,
          bridge: crew.bridge,
          below_deck: crew.belowDeck,
        },
      });
      setShowSavePreset(false);
      setSavePresetName('');
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setSavingPreset(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: '100vh' }}>
      <WorkspaceHeader
        shipId={shipId}
        scenarioId={scenarioId}
        onShipIdChange={setShipId}
        onScenarioIdChange={setScenarioId}
        shipLevel={shipLevel}
        onShipLevelChange={setShipLevel}
        crew={crew}
        simsPerCrew={simsPerCrew}
        onSimsPerCrewChange={setSimsPerCrew}
        estimate={estimate}
        lastOptimizeDurationMs={lastOptimizeDurationMs}
        onRunSim={handleRunSim}
        onRunOptimize={handleRunOptimize}
        onSavePreset={() => setShowSavePreset(true)}
        loadingSim={loadingSim}
        loadingOptimize={loadingOptimize}
        optimizeProgress={optimizeProgress}
        optimizeCrewsDone={optimizeCrewsDone}
        optimizeTotalCrews={optimizeTotalCrews}
      />
      {showSavePreset && (
        <div
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.6)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
          onClick={() => !savingPreset && setShowSavePreset(false)}
        >
          <div
            style={{
              background: 'var(--surface)',
              padding: '1.5rem',
              borderRadius: 8,
              border: '1px solid var(--border)',
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <label style={{ display: 'block', marginBottom: 8 }}>
              Preset name
              <input
                type="text"
                value={savePresetName}
                onChange={(e) => setSavePresetName(e.target.value)}
                placeholder="Unnamed"
                style={{
                  display: 'block',
                  marginTop: 4,
                  padding: '0.5rem',
                  width: 240,
                  background: 'var(--bg)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: 'var(--text)',
                }}
              />
            </label>
            <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
              <button
                type="button"
                onClick={handleSavePreset}
                disabled={savingPreset}
                style={{
                  padding: '0.5rem 1rem',
                  background: 'var(--accent)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'var(--bg)',
                }}
              >
                {savingPreset ? 'Saving…' : 'Save'}
              </button>
              <button
                type="button"
                onClick={() => setShowSavePreset(false)}
                disabled={savingPreset}
                style={{
                  padding: '0.5rem 1rem',
                  background: 'var(--border)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'var(--text)',
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
      {error && (
        <div style={{ padding: '0.5rem 1rem', background: 'var(--error)', color: 'white' }}>
          {error}
        </div>
      )}
      <div
        style={{
          display: 'flex',
          flex: 1,
          minHeight: 0,
          maxWidth: 1400,
          width: '100%',
          alignSelf: 'center',
        }}
      >
        <section
          style={{
            flex: 1,
            minWidth: 0,
            display: 'flex',
            flexDirection: 'column',
            padding: '0 1rem',
          }}
        >
          <CrewBuilder
            shipLevel={shipLevel}
            crew={crew}
            pins={pins}
            onCrewChange={setCrew}
            onPinsChange={setPins}
          />
          <div style={{ flex: 1, minHeight: 200 }}>
            <SimResults
              simResult={simResult}
              recommendations={recommendations}
              loadingSim={loadingSim}
              loadingOptimize={loadingOptimize}
              optimizeProgress={optimizeProgress}
              optimizeCrewsDone={optimizeCrewsDone}
              optimizeTotalCrews={optimizeTotalCrews}
            />
          </div>
        </section>
        <OptimizePanel
          collapsed={rightPanelCollapsed}
          onToggleCollapsed={() => setRightPanelCollapsed(!rightPanelCollapsed)}
          crew={crew}
          loadingOptimize={loadingOptimize}
          optimizeCrewsDone={optimizeCrewsDone}
          optimizeTotalCrews={optimizeTotalCrews}
          maxCandidates={maxCandidates}
          onMaxCandidatesChange={setMaxCandidates}
          prioritizeBelowDecksAbility={prioritizeBelowDecksAbility}
          onPrioritizeBelowDecksAbilityChange={setPrioritizeBelowDecksAbility}
        />
      </div>
    </div>
  );
}
