import { useEffect, useMemo, useState } from "react";
import CrewBuilder from "../components/CrewBuilder";
import GuidedModePanel, {
  type GuidedStep,
} from "../components/GuidedModePanel";
import OptimizePanel from "../components/OptimizePanel";
import SavePresetModal from "../components/SavePresetModal";
import SimResults from "../components/SimResults";
import WorkspaceHeader from "../components/WorkspaceHeader";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import type { OfficerListItem } from "../lib/api";
import { fetchOfficers } from "../lib/api";
import { belowDeckSlotCount } from "../lib/types";
import { useWorkspace } from "../lib/useWorkspace";

export default function Workspace() {
  const ws = useWorkspace();
  const { activeProfileId } = useProfile();
  const { mode, ownedOnly, setMode } = useWorkspaceMode();
  const [officerOptions, setOfficerOptions] = useState<OfficerListItem[]>([]);
  const [guidedStep, setGuidedStep] = useState<GuidedStep>(0);

  useEffect(() => {
    if (mode !== "guided") setGuidedStep(0);
  }, [mode]);

  useEffect(() => {
    let cancelled = false;
    fetchOfficers(ownedOnly, activeProfileId).then((list) => {
      if (!cancelled) setOfficerOptions(list);
    });
    return () => {
      cancelled = true;
    };
  }, [ownedOnly, activeProfileId]);
  const compareWorkspace = useMemo(
    () =>
      ws.shipId && ws.scenarioId
        ? {
            ship: ws.shipId,
            hostile: ws.scenarioId,
            shipTier: ws.shipTier,
            shipLevel: ws.shipLevel,
            numSims: ws.simsPerCrew,
            belowDecksSlots: belowDeckSlotCount(
              ws.shipLevel,
              ws.belowDeckUnlockLevels,
            ),
            profileId: ws.activeProfileId,
            ...(ws.selectedSupportBuffs.length > 0
              ? { supportBuffs: ws.selectedSupportBuffs }
              : {}),
          }
        : null,
    [
      ws.shipId,
      ws.scenarioId,
      ws.shipTier,
      ws.shipLevel,
      ws.simsPerCrew,
      ws.belowDeckUnlockLevels,
      ws.activeProfileId,
      ws.selectedSupportBuffs,
    ],
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        minHeight: "100vh",
      }}
    >
      {mode === "guided" && (
        <GuidedModePanel
          step={guidedStep}
          onStepChange={setGuidedStep}
          shipSelected={Boolean(ws.shipId)}
          targetSelected={Boolean(ws.scenarioId)}
          crewReady={Boolean(
            ws.crew.captain && ws.crew.bridge[0] && ws.crew.bridge[1],
          )}
          running={ws.loadingSim || ws.loadingOptimize}
          optimizing={ws.loadingOptimize}
          hasResults={Boolean(ws.simResult || ws.recommendations.length > 0)}
          onRunSim={ws.handleRunSim}
          onRunOptimize={ws.handleRunOptimize}
          onCancelOptimize={ws.handleCancelOptimize}
          onRestart={ws.resetResults}
          onExit={() => setMode("roster")}
        />
      )}
      {(mode !== "guided" || guidedStep === 0) && (
        <WorkspaceHeader
          shipId={ws.shipId}
          scenarioId={ws.scenarioId}
          onShipIdChange={ws.setShipId}
          onScenarioIdChange={ws.setScenarioId}
          enemyType={ws.enemyType}
          onEnemyTypeChange={ws.setEnemyType}
          shipTier={ws.shipTier}
          onShipTierChange={ws.setShipTier}
          shipLevel={ws.shipLevel}
          onShipLevelChange={ws.setShipLevel}
          onBelowDeckUnlockLevelsChange={ws.setBelowDeckUnlockLevels}
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
      )}
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
            background:
              ws.errorSeverity === "warning"
                ? "rgba(201, 162, 39, 0.22)"
                : "var(--error)",
            color: ws.errorSeverity === "warning" ? "var(--text)" : "white",
            borderBottom:
              ws.errorSeverity === "warning"
                ? "1px solid var(--warning)"
                : undefined,
          }}
          role="alert"
        >
          {ws.errorSeverity === "warning" && (
            <span style={{ fontWeight: 600, marginRight: "0.5rem" }}>
              Server busy:
            </span>
          )}
          {ws.error}
        </div>
      )}
      <div
        className="workspace-body"
        hidden={mode === "guided" && guidedStep !== 1 && guidedStep !== 3}
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
          {(mode !== "guided" || guidedStep === 1) && (
            <CrewBuilder
              guided={mode === "guided"}
              belowDecksSlots={belowDeckSlotCount(
                ws.shipLevel,
                ws.belowDeckUnlockLevels,
              )}
              crew={ws.crew}
              pins={ws.pins}
              onCrewChange={ws.setCrew}
              onPinsChange={ws.setPins}
              officerOptions={officerOptions}
              enemyType={ws.enemyType}
              eligibility={ws.eligibility}
            />
          )}
          {(mode !== "guided" || guidedStep === 3) && (
            <div id="guided-results" style={{ flex: 1, minHeight: 200 }}>
              <SimResults
                simResult={ws.simResult}
                recommendations={ws.recommendations}
                warnings={ws.resultWarnings}
                unresolvedOfficers={ws.unresolvedOfficers}
                loadingSim={ws.loadingSim}
                loadingOptimize={ws.loadingOptimize}
                optimizeProgress={ws.optimizeProgress}
                optimizeCrewsDone={ws.optimizeCrewsDone}
                optimizeTotalCrews={ws.optimizeTotalCrews}
                optimizePhase={ws.optimizePhase}
                optimizeEtaSeconds={ws.optimizeEtaSeconds}
                optimizeThroughput={ws.optimizeThroughput}
                optimizePreview={ws.optimizePreview}
                optimizeEffectiveStrategy={ws.lastOptimizeEffectiveStrategy}
                compareWorkspace={compareWorkspace}
              />
            </div>
          )}
        </section>
        {mode !== "guided" && (
          <OptimizePanel
            collapsed={ws.rightPanelCollapsed}
            onToggleCollapsed={() =>
              ws.setRightPanelCollapsed(!ws.rightPanelCollapsed)
            }
            officerOptions={officerOptions}
            crew={ws.crew}
            loadingOptimize={ws.loadingOptimize}
            optimizeCrewsDone={ws.optimizeCrewsDone}
            optimizeTotalCrews={ws.optimizeTotalCrews}
            optimizePhase={ws.optimizePhase}
            optimizeEtaSeconds={ws.optimizeEtaSeconds}
            optimizeThroughput={ws.optimizeThroughput}
            maxCandidates={ws.maxCandidates}
            onMaxCandidatesChange={ws.setMaxCandidates}
            belowDecksPoolMode={ws.belowDecksPoolMode}
            onBelowDecksPoolModeChange={ws.setBelowDecksPoolMode}
            availableSeeds={ws.availableSeeds}
            selectedSeeds={ws.selectedSeeds}
            onSelectedSeedsChange={ws.setSelectedSeeds}
            heuristicsOnly={ws.heuristicsOnly}
            onHeuristicsOnlyChange={ws.setHeuristicsOnly}
            fastDiscovery={ws.fastDiscovery}
            onFastDiscoveryChange={ws.setFastDiscovery}
            belowDecksStrategy={ws.belowDecksStrategy}
            onBelowDecksStrategyChange={ws.setBelowDecksStrategy}
            optimizerStrategy={ws.optimizerStrategy}
            onOptimizerStrategyChange={ws.setOptimizerStrategy}
            enableLearnedPairPrior={ws.enableLearnedPairPrior}
            onEnableLearnedPairPriorChange={ws.setEnableLearnedPairPrior}
            tieredScoutSims={ws.tieredScoutSims}
            onTieredScoutSimsChange={ws.setTieredScoutSims}
            tieredTopK={ws.tieredTopK}
            onTieredTopKChange={ws.setTieredTopK}
            tieredRandomExplorationPct={ws.tieredRandomExplorationPct}
            onTieredRandomExplorationPctChange={
              ws.setTieredRandomExplorationPct
            }
            localRefinement={ws.localRefinement}
            onLocalRefinementChange={ws.setLocalRefinement}
            noveltyLambdaText={ws.noveltyLambdaText}
            onNoveltyLambdaTextChange={ws.setNoveltyLambdaText}
            noveltyDiverseTopText={ws.noveltyDiverseTopText}
            onNoveltyDiverseTopTextChange={ws.setNoveltyDiverseTopText}
            noveltyPoolText={ws.noveltyPoolText}
            onNoveltyPoolTextChange={ws.setNoveltyPoolText}
            noveltyHistoryAnchors={ws.noveltyHistoryAnchors}
            onNoveltyHistoryAnchorsChange={ws.setNoveltyHistoryAnchors}
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
            chainGrindEnabled={ws.chainGrindEnabled}
            onChainGrindEnabledChange={ws.setChainGrindEnabled}
            chainKillsTarget={ws.chainKillsTarget}
            onChainKillsTargetChange={ws.setChainKillsTarget}
            chainSecondary={ws.chainSecondary}
            onChainSecondaryChange={ws.setChainSecondary}
            cachedWarmStartBadge={ws.cachedWarmStartBadge}
          />
        )}
      </div>
    </div>
  );
}
