import { useCallback, useEffect, useRef, useState } from "react";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import {
  type CrewRecommendation,
  cancelOptimizeJob,
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
  const [defenderBelowDeckUnlockLevels, setDefenderBelowDeckUnlockLevels] =
    useState<number[]>(() => [...DEFAULT_BELOW_DECK_UNLOCK_LEVELS]);

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

  useEffect(() => {
    const count = belowDeckSlotCount(attackerShipLevel, belowDeckUnlockLevels);
    setAttackerCrew((current) => ({
      ...current,
      belowDeck: Array.from(
        { length: count },
        (_, i) => current.belowDeck[i] ?? null,
      ),
    }));
    setAttackerPins((current) => ({
      ...current,
      belowDeck: Array.from(
        { length: count },
        (_, i) => current.belowDeck[i] ?? false,
      ),
    }));
  }, [attackerShipLevel, belowDeckUnlockLevels]);

  useEffect(() => {
    const count = belowDeckSlotCount(
      defenderShipLevel,
      defenderBelowDeckUnlockLevels,
    );
    setDefenderCrew((current) => ({
      ...current,
      belowDeck: Array.from(
        { length: count },
        (_, i) => current.belowDeck[i] ?? null,
      ),
    }));
    setDefenderPins((current) => ({
      ...current,
      belowDeck: Array.from(
        { length: count },
        (_, i) => current.belowDeck[i] ?? false,
      ),
    }));
  }, [defenderShipLevel, defenderBelowDeckUnlockLevels]);

  const [simsPerCrew, setSimsPerCrew] = useState(5000);
  const [simResult, setSimResult] = useState<SimulateStats | null>(null);
  const [loadingSim, setLoadingSim] = useState(false);
  const [resultWarnings, setResultWarnings] = useState<string[]>([]);
  const [unresolvedOfficers, setUnresolvedOfficers] = useState<string[]>([]);
  const [recommendations, setRecommendations] = useState<CrewRecommendation[]>(
    [],
  );
  const [loadingOptimize, setLoadingOptimize] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generationRef = useRef(0);
  const jobRef = useRef<string | null>(null);
  const startAbortRef = useRef<AbortController | null>(null);
  const invalidateRun = () => {
    generationRef.current += 1;
    startAbortRef.current?.abort();
    startAbortRef.current = null;
    const jobId = jobRef.current;
    jobRef.current = null;
    if (jobId) cancelOptimizeJob(jobId).catch(() => {});
  };

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

  const [attackerSupportBuffs, setAttackerSupportBuffs] = useState<
    SupportBuffId[]
  >([]);
  const [defenderSupportBuffs, setDefenderSupportBuffs] = useState<
    SupportBuffId[]
  >([]);
  const [defenderAllianceDebuffs, setDefenderAllianceDebuffs] = useState<
    SupportBuffId[]
  >([]);
  // Results and active jobs belong to the exact inputs that produced them.
  useEffect(() => {
    setLoadingSim(false);
    setLoadingOptimize(false);
    setSimResult(null);
    setRecommendations([]);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
    return invalidateRun;
  }, [
    activeProfileId,
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
    belowDeckUnlockLevels,
    attackerSupportBuffs,
    defenderSupportBuffs,
    defenderAllianceDebuffs,
  ]);
  const setValidatedAttackerSupportBuffs = (ids: readonly string[]) => {
    setAttackerSupportBuffs(normalizeSupportBuffSelection(ids).ids);
  };
  const setValidatedDefenderSupportBuffs = (ids: readonly string[]) => {
    setDefenderSupportBuffs(normalizeSupportBuffSelection(ids).ids);
  };
  const setValidatedDefenderAllianceDebuffs = (ids: readonly string[]) => {
    setDefenderAllianceDebuffs(normalizeSupportBuffSelection(ids).ids);
  };

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
        attackerSupportBuffs,
        defenderSupportBuffs,
        defenderAllianceDebuffs,
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
      attackerSupportBuffs,
      defenderSupportBuffs,
      defenderAllianceDebuffs,
    });
    if (!params) {
      setError("Invalid PvP simulate parameters.");
      return;
    }
    invalidateRun();
    const generation = generationRef.current;
    setLoadingSim(true);
    setLoadingOptimize(false);
    setError(null);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
    try {
      const res = await simulate(params, activeProfileId);
      if (generation !== generationRef.current) return;
      setSimResult(res.stats);
      setRecommendations([]);
      setResultWarnings(res.warnings ?? []);
      setUnresolvedOfficers(res.unresolved_officers ?? []);
    } catch (e) {
      if (generation !== generationRef.current) return;
      setError(formatApiError(e));
    } finally {
      if (generation === generationRef.current) setLoadingSim(false);
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
    attackerSupportBuffs,
    defenderSupportBuffs,
    defenderAllianceDebuffs,
  ]);

  const handleRunOptimize = useCallback(async () => {
    if (!canRun) {
      setError(
        "Select attacker ship, defender ship, opponent profile, and attacker captain.",
      );
      return;
    }
    invalidateRun();
    const generation = generationRef.current;
    const startAbort = new AbortController();
    startAbortRef.current = startAbort;
    setLoadingSim(false);
    setLoadingOptimize(true);
    setError(null);
    setRecommendations([]);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
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
          attackerSupportBuffs,
          defenderSupportBuffs,
          defenderAllianceDebuffs,
          optimizeCacheKey: cacheKey,
        }),
        ...(warm && warm.length > 0 ? { warm_start_crews: warm } : {}),
      };
      const start = await optimizeStart(
        body as Parameters<typeof optimizeStart>[0],
        activeProfileId,
        { signal: startAbort.signal },
      );
      const jobId = start.job_id;
      if (generation !== generationRef.current) {
        cancelOptimizeJob(jobId).catch(() => {});
        return;
      }
      startAbortRef.current = null;
      jobRef.current = jobId;
      for (;;) {
        await new Promise((r) => setTimeout(r, 450));
        if (generation !== generationRef.current) return;
        const status = await getOptimizeStatus(jobId);
        if (generation !== generationRef.current) return;
        if (status.status === "done" && status.result) {
          jobRef.current = null;
          const recs = status.result.recommendations ?? [];
          setRecommendations(recs);
          setSimResult(null);
          setResultWarnings(status.result.warnings ?? []);
          setUnresolvedOfficers([]);
          saveWarmStartFromRecommendations(cacheKey, recs);
          break;
        }
        if (status.status === "error") {
          jobRef.current = null;
          throw new Error(status.error?.trim() || "Optimize failed");
        }
      }
    } catch (e) {
      if (generation !== generationRef.current) return;
      if (jobRef.current) {
        cancelOptimizeJob(jobRef.current).catch(() => {});
        jobRef.current = null;
      }
      setError(formatApiError(e));
    } finally {
      if (generation === generationRef.current) {
        startAbortRef.current = null;
        setLoadingOptimize(false);
      }
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
    attackerSupportBuffs,
    defenderSupportBuffs,
    defenderAllianceDebuffs,
  ]);

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
    defenderBelowDeckUnlockLevels,
    setDefenderBelowDeckUnlockLevels,
    simsPerCrew,
    setSimsPerCrew,
    simResult,
    resultWarnings,
    unresolvedOfficers,
    recommendations,
    loadingSim,
    loadingOptimize,
    error,
    setError,
    canRun,
    handleRunSim,
    handleRunOptimize,
    attackerSupportBuffs,
    setAttackerSupportBuffs: setValidatedAttackerSupportBuffs,
    defenderSupportBuffs,
    setDefenderSupportBuffs: setValidatedDefenderSupportBuffs,
    defenderAllianceDebuffs,
    setDefenderAllianceDebuffs: setValidatedDefenderAllianceDebuffs,
    ownedOnly,
  };
}
