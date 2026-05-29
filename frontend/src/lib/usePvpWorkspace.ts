import { useCallback, useEffect, useState } from "react";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import {
  type CrewRecommendation,
  formatApiError,
  getOptimizeStatus,
  optimizeStart,
  type SimulateStats,
  simulate,
} from "./api";
import {
  buildOptimizeWarmStartKey,
  loadWarmStartCrews,
  saveWarmStartFromRecommendations,
} from "./optimizeWarmStart";
import {
  buildPvpDefenderFingerprint,
  buildPvpOptimizeStartBody,
  buildPvpSimulateParams,
} from "./pvpRequests";
import {
  normalizeSupportBuffSelection,
  type SupportBuffId,
} from "./supportBuffs";
import {
  belowDeckSlotCount,
  type CrewState,
  createEmptyCrew,
  createEmptyPins,
  DEFAULT_BELOW_DECK_UNLOCK_LEVELS,
  type PinsState,
} from "./types";

export function usePvpWorkspace() {
  const { activeProfileId, profiles } = useProfile();
  const { ownedOnly } = useWorkspaceMode();

  const [attackerShipId, setAttackerShipId] = useState("");
  const [attackerShipTier, setAttackerShipTier] = useState(1);
  const [attackerShipLevel, setAttackerShipLevel] = useState(50);
  const [defenderShipId, setDefenderShipId] = useState("");
  const [defenderShipTier, setDefenderShipTier] = useState(1);
  const [defenderShipLevel, setDefenderShipLevel] = useState(50);
  const [opponentProfileId, setOpponentProfileId] = useState("");
  const [belowDeckUnlockLevels, setBelowDeckUnlockLevels] = useState<number[]>(
    () => [...DEFAULT_BELOW_DECK_UNLOCK_LEVELS],
  );

  const [attackerCrew, setAttackerCrew] = useState<CrewState>(() =>
    createEmptyCrew(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );
  const [defenderCrew, setDefenderCrew] = useState<CrewState>(() =>
    createEmptyCrew(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );
  const [attackerPins, setAttackerPins] = useState<PinsState>(() =>
    createEmptyPins(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );
  const [defenderPins, setDefenderPins] = useState<PinsState>(() =>
    createEmptyPins(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );

  const [simsPerCrew, setSimsPerCrew] = useState(5000);
  const [simResult, setSimResult] = useState<SimulateStats | null>(null);
  const [loadingSim, setLoadingSim] = useState(false);
  const [recommendations, setRecommendations] = useState<CrewRecommendation[]>(
    [],
  );
  const [loadingOptimize, setLoadingOptimize] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!opponentProfileId && profiles.length > 0) {
      const preferred =
        (activeProfileId
          ? profiles.find((p) => p.id !== activeProfileId)
          : undefined) ?? profiles[0];
      if (preferred) setOpponentProfileId(preferred.id);
    }
  }, [profiles, activeProfileId, opponentProfileId]);

  const canRun =
    Boolean(attackerShipId && defenderShipId && opponentProfileId.trim()) &&
    Boolean(attackerCrew.captain);

  const optimizeCacheKey = () =>
    buildOptimizeWarmStartKey({
      profileId: activeProfileId,
      shipId: attackerShipId,
      scenarioId: `pvp:${buildPvpDefenderFingerprint({
        defenderShipId,
        defenderShipTier,
        defenderShipLevel,
        opponentProfileId,
        defenderCrew,
      })}`,
      shipTier: attackerShipTier,
      shipLevel: attackerShipLevel,
      maxCandidates: 100,
      constraintsFingerprint: "pvp",
      defenderOpponent: "Player",
      belowDecksSlots: belowDeckSlotCount(
        attackerShipLevel,
        belowDeckUnlockLevels,
      ),
    });

  const handleRunSim = useCallback(async () => {
    if (!canRun) {
      setError(
        "Select attacker ship, defender ship, opponent profile, and attacker captain.",
      );
      return;
    }
    const params = buildPvpSimulateParams({
      attackerShipId,
      attackerShipTier,
      attackerShipLevel,
      attackerCrew,
      defenderShipId,
      defenderShipTier,
      defenderShipLevel,
      defenderCrew,
      opponentProfileId,
      simsPerCrew,
    });
    if (!params) {
      setError("Invalid PvP simulate parameters.");
      return;
    }
    setLoadingSim(true);
    setError(null);
    try {
      const res = await simulate(params, activeProfileId);
      setSimResult(res.stats);
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setLoadingSim(false);
    }
  }, [
    canRun,
    attackerShipId,
    attackerShipTier,
    attackerShipLevel,
    attackerCrew,
    defenderShipId,
    defenderShipTier,
    defenderShipLevel,
    defenderCrew,
    opponentProfileId,
    simsPerCrew,
    activeProfileId,
  ]);

  const handleRunOptimize = useCallback(async () => {
    if (!canRun) {
      setError(
        "Select attacker ship, defender ship, opponent profile, and attacker captain.",
      );
      return;
    }
    setLoadingOptimize(true);
    setError(null);
    setRecommendations([]);
    try {
      const cacheKey = optimizeCacheKey();
      const warm = loadWarmStartCrews(cacheKey);
      const body = {
        ...buildPvpOptimizeStartBody({
          attackerShipId,
          attackerShipTier,
          attackerShipLevel,
          defenderShipId,
          defenderShipTier,
          defenderShipLevel,
          opponentProfileId,
          defenderCrew,
          simsPerCrew,
          maxCandidates: 100,
          optimizerStrategy: "tiered",
          belowDecksSlots: belowDeckSlotCount(
            attackerShipLevel,
            belowDeckUnlockLevels,
          ),
          optimizeCacheKey: cacheKey,
        }),
        ...(warm && warm.length > 0 ? { warm_start_crews: warm } : {}),
      };
      const start = await optimizeStart(
        body as Parameters<typeof optimizeStart>[0],
        activeProfileId,
      );
      const jobId = start.job_id;
      for (;;) {
        await new Promise((r) => setTimeout(r, 450));
        const status = await getOptimizeStatus(jobId);
        if (status.status === "done" && status.result) {
          const recs = status.result.recommendations ?? [];
          setRecommendations(recs);
          saveWarmStartFromRecommendations(cacheKey, recs);
          break;
        }
        if (status.status === "error") {
          throw new Error(status.error?.trim() || "Optimize failed");
        }
      }
    } catch (e) {
      setError(formatApiError(e));
    } finally {
      setLoadingOptimize(false);
    }
  }, [
    canRun,
    attackerShipId,
    attackerShipTier,
    attackerShipLevel,
    defenderShipId,
    defenderShipTier,
    defenderShipLevel,
    defenderCrew,
    opponentProfileId,
    simsPerCrew,
    activeProfileId,
    belowDeckUnlockLevels,
    optimizeCacheKey,
  ]);

  const [selectedSupportBuffs, setSelectedSupportBuffs] = useState<
    SupportBuffId[]
  >([]);
  const setValidatedSelectedSupportBuffs = (ids: readonly string[]) => {
    setSelectedSupportBuffs(normalizeSupportBuffSelection(ids).ids);
  };

  return {
    activeProfileId,
    profiles,
    opponentProfileId,
    setOpponentProfileId,
    attackerShipId,
    setAttackerShipId,
    attackerShipTier,
    setAttackerShipTier,
    attackerShipLevel,
    setAttackerShipLevel,
    defenderShipId,
    setDefenderShipId,
    defenderShipTier,
    setDefenderShipTier,
    defenderShipLevel,
    setDefenderShipLevel,
    attackerCrew,
    setAttackerCrew,
    defenderCrew,
    setDefenderCrew,
    attackerPins,
    setAttackerPins,
    defenderPins,
    setDefenderPins,
    belowDeckUnlockLevels,
    setBelowDeckUnlockLevels,
    simsPerCrew,
    setSimsPerCrew,
    simResult,
    recommendations,
    loadingSim,
    loadingOptimize,
    error,
    setError,
    canRun,
    handleRunSim,
    handleRunOptimize,
    selectedSupportBuffs,
    setSelectedSupportBuffs: setValidatedSelectedSupportBuffs,
    ownedOnly,
  };
}
