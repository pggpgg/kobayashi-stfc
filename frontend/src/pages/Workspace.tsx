import CrewBuilder from "../components/CrewBuilder";
import OptimizePanel from "../components/OptimizePanel";
import SavePresetModal from "../components/SavePresetModal";
import SimResults from "../components/SimResults";
import WorkspaceHeader from "../components/WorkspaceHeader";
import { belowDeckSlotCount } from "../lib/types";
import { useWorkspace } from "../lib/useWorkspace";

export default function Workspace() {
  const ws = useWorkspace();
  const compareWorkspace =
    ws.shipId && ws.scenarioId
      ? {
          ship: ws.shipId,
          hostile: ws.scenarioId,
          shipTier: ws.shipTier,
          shipLevel: ws.shipLevel,
          numSims: ws.simsPerCrew,
          belowDecksSlots: belowDeckSlotCount(ws.shipLevel),
          profileId: ws.activeProfileId,
          ...(ws.selectedSupportBuffs.length > 0
            ? { supportBuffs: ws.selectedSupportBuffs }
            : {}),
        }
      : null;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        minHeight: "100vh",
      }}
    >
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
        optimizePhase={ws.optimizePhase}
        optimizeEtaSeconds={ws.optimizeEtaSeconds}
        optimizeStreamMode={ws.optimizeStreamMode}
        selectedSupportBuffs={ws.selectedSupportBuffs}
        onSelectedSupportBuffsChange={ws.setSelectedSupportBuffs}
      />
      <SavePresetModal
        open={ws.showSavePreset}
        savePresetName={ws.savePresetName}
        onSavePresetNameChange={ws.setSavePresetName}
        savingPreset={ws.savingPreset}
        onSave={ws.handleSavePreset}
        onClose={() => ws.setShowSavePreset(false)}
      />
      {ws.workspaceInfo && (
        <div
          style={{
            padding: "0.5rem 1rem",
            background: "rgba(201, 162, 39, 0.2)",
            borderBottom: "1px solid var(--warning)",
            color: "var(--text)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
          }}
        >
          <span style={{ fontSize: "0.9rem" }}>{ws.workspaceInfo}</span>
          <button
            type="button"
            onClick={() => ws.setWorkspaceInfo(null)}
            style={{
              flexShrink: 0,
              padding: "0.25rem 0.6rem",
              fontSize: "0.8rem",
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              color: "var(--text)",
            }}
          >
            Dismiss
          </button>
        </div>
      )}
      {ws.error && (
        <div
          style={{
            padding: "0.5rem 1rem",
            background: "var(--error)",
            color: "white",
          }}
          role="alert"
        >
          {ws.error}
        </div>
      )}
      <div
        style={{
          display: "flex",
          flex: 1,
          minHeight: 0,
          maxWidth: 1400,
          width: "100%",
          alignSelf: "center",
        }}
      >
        <section
          style={{
            flex: 1,
            minWidth: 0,
            display: "flex",
            flexDirection: "column",
            padding: "0 1rem",
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
              optimizePhase={ws.optimizePhase}
              optimizeEtaSeconds={ws.optimizeEtaSeconds}
              optimizeThroughput={ws.optimizeThroughput}
              optimizePreview={ws.optimizePreview}
              compareWorkspace={compareWorkspace}
            />
          </div>
        </section>
        <OptimizePanel
          collapsed={ws.rightPanelCollapsed}
          onToggleCollapsed={() =>
            ws.setRightPanelCollapsed(!ws.rightPanelCollapsed)
          }
          crew={ws.crew}
          loadingOptimize={ws.loadingOptimize}
          optimizeCrewsDone={ws.optimizeCrewsDone}
          optimizeTotalCrews={ws.optimizeTotalCrews}
          optimizePhase={ws.optimizePhase}
          optimizeEtaSeconds={ws.optimizeEtaSeconds}
          optimizeThroughput={ws.optimizeThroughput}
          maxCandidates={ws.maxCandidates}
          onMaxCandidatesChange={ws.setMaxCandidates}
          prioritizeBelowDecksAbility={ws.prioritizeBelowDecksAbility}
          onPrioritizeBelowDecksAbilityChange={
            ws.setPrioritizeBelowDecksAbility
          }
          availableSeeds={ws.availableSeeds}
          selectedSeeds={ws.selectedSeeds}
          onSelectedSeedsChange={ws.setSelectedSeeds}
          heuristicsOnly={ws.heuristicsOnly}
          onHeuristicsOnlyChange={ws.setHeuristicsOnly}
          belowDecksStrategy={ws.belowDecksStrategy}
          onBelowDecksStrategyChange={ws.setBelowDecksStrategy}
          optimizerStrategy={ws.optimizerStrategy}
          onOptimizerStrategyChange={ws.setOptimizerStrategy}
          optimizeMustInclude={ws.optimizeMustInclude}
          onOptimizeMustIncludeChange={ws.setOptimizeMustInclude}
          optimizeExclude={ws.optimizeExclude}
          onOptimizeExcludeChange={ws.setOptimizeExclude}
          optimizeCaptainMust={ws.optimizeCaptainMust}
          onOptimizeCaptainMustChange={ws.setOptimizeCaptainMust}
          optimizeBridgeMust={ws.optimizeBridgeMust}
          onOptimizeBridgeMustChange={ws.setOptimizeBridgeMust}
          optimizeBelowMust={ws.optimizeBelowMust}
          onOptimizeBelowMustChange={ws.setOptimizeBelowMust}
          optimizeGroupsJson={ws.optimizeGroupsJson}
          onOptimizeGroupsJsonChange={ws.setOptimizeGroupsJson}
        />
      </div>
    </div>
  );
}
