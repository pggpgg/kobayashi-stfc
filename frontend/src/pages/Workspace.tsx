import WorkspaceHeader from '../components/WorkspaceHeader';
import CrewBuilder from '../components/CrewBuilder';
import OptimizePanel from '../components/OptimizePanel';
import SimResults from '../components/SimResults';
import { useWorkspace } from '../lib/useWorkspace';

export default function Workspace() {
  const ws = useWorkspace();

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: '100vh' }}>
      <WorkspaceHeader
        shipId={ws.shipId}
        scenarioId={ws.scenarioId}
        onShipIdChange={ws.setShipId}
        onScenarioIdChange={ws.setScenarioId}
        shipTier={ws.shipTier}
        onShipTierChange={ws.setShipTier}
        shipLevel={ws.shipLevel}
        onShipLevelChange={ws.setShipLevel}
        crew={ws.crew}
        simsPerCrew={ws.simsPerCrew}
        onSimsPerCrewChange={ws.setSimsPerCrew}
        estimate={ws.estimate}
        lastOptimizeDurationMs={ws.lastOptimizeDurationMs}
        onRunSim={ws.handleRunSim}
        onRunOptimize={ws.handleRunOptimize}
        onCancelOptimize={ws.handleCancelOptimize}
        onSavePreset={() => ws.setShowSavePreset(true)}
        loadingSim={ws.loadingSim}
        loadingOptimize={ws.loadingOptimize}
        optimizeProgress={ws.optimizeProgress}
        optimizeCrewsDone={ws.optimizeCrewsDone}
        optimizeTotalCrews={ws.optimizeTotalCrews}
      />
      {ws.showSavePreset && (
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
          onClick={() => !ws.savingPreset && ws.setShowSavePreset(false)}
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
                value={ws.savePresetName}
                onChange={(e) => ws.setSavePresetName(e.target.value)}
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
                onClick={ws.handleSavePreset}
                disabled={ws.savingPreset}
                style={{
                  padding: '0.5rem 1rem',
                  background: 'var(--accent)',
                  border: 'none',
                  borderRadius: 6,
                  color: 'var(--bg)',
                }}
              >
                {ws.savingPreset ? 'Saving…' : 'Save'}
              </button>
              <button
                type="button"
                onClick={() => ws.setShowSavePreset(false)}
                disabled={ws.savingPreset}
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
      {ws.error && (
        <div style={{ padding: '0.5rem 1rem', background: 'var(--error)', color: 'white' }}>
          {ws.error}
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
            shipLevel={ws.shipLevel}
            crew={ws.crew}
            pins={ws.pins}
            onCrewChange={ws.setCrew}
            onPinsChange={ws.setPins}
          />
          <div style={{ flex: 1, minHeight: 200 }}>
            <SimResults
              simResult={ws.simResult}
              recommendations={ws.recommendations}
              loadingSim={ws.loadingSim}
              loadingOptimize={ws.loadingOptimize}
              optimizeProgress={ws.optimizeProgress}
              optimizeCrewsDone={ws.optimizeCrewsDone}
              optimizeTotalCrews={ws.optimizeTotalCrews}
            />
          </div>
        </section>
        <OptimizePanel
          collapsed={ws.rightPanelCollapsed}
          onToggleCollapsed={() => ws.setRightPanelCollapsed(!ws.rightPanelCollapsed)}
          crew={ws.crew}
          loadingOptimize={ws.loadingOptimize}
          optimizeCrewsDone={ws.optimizeCrewsDone}
          optimizeTotalCrews={ws.optimizeTotalCrews}
          maxCandidates={ws.maxCandidates}
          onMaxCandidatesChange={ws.setMaxCandidates}
          prioritizeBelowDecksAbility={ws.prioritizeBelowDecksAbility}
          onPrioritizeBelowDecksAbilityChange={ws.setPrioritizeBelowDecksAbility}
          availableSeeds={ws.availableSeeds}
          selectedSeeds={ws.selectedSeeds}
          onSelectedSeedsChange={ws.setSelectedSeeds}
          heuristicsOnly={ws.heuristicsOnly}
          onHeuristicsOnlyChange={ws.setHeuristicsOnly}
          belowDecksStrategy={ws.belowDecksStrategy}
          onBelowDecksStrategyChange={ws.setBelowDecksStrategy}
          optimizerStrategy={ws.optimizerStrategy}
          onOptimizerStrategyChange={ws.setOptimizerStrategy}
        />
      </div>
    </div>
  );
}
