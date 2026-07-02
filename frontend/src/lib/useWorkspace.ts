import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useProfile } from "../contexts/ProfileContext";
import { useWorkspaceMode } from "../contexts/WorkspaceModeContext";
import {
  API_ERROR_CPU_BUSY,
  ApiError,
  type CrewRecommendation,
  cancelOptimizeJob,
  fetchHeuristics,
  fetchOfficerEligibility,
  fetchOfficers,
  formatApiError,
  getOptimizeEstimate,
  getOptimizeStatus,
  getOptimizeStreamUrl,
  type OfficerEligibilityResponse,
  type OptimizeEstimate,
  type OptimizeStatusResponse,
  optimizeStart,
  type Preset,
  type SimulateStats,
  savePreset,
  simulate,
} from "./api";
import {
  clearPersistedOptimizeJob,
  persistOptimizeJob,
  profileMatchesPersisted,
  readPersistedOptimizeJob,
} from "./optimizeJobStorage";
import {
  buildOptimizeWarmStartKey,
  loadWarmStartCrews,
  saveWarmStartFromRecommendations,
} from "./optimizeWarmStart";
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
import {
  buildWorkspaceOptimizeStartBody,
  buildWorkspaceSimulateParams,
} from "./workspaceRequests";

const POLL_INTERVAL_MS = 350;
/** Max automatic SSE reconnect attempts before falling back to HTTP polling only. */
const MAX_SSE_RECONNECT_ATTEMPTS = 8;
const SSE_BACKOFF_BASE_MS = 500;
const SSE_BACKOFF_CAP_MS = 30_000;

export type OptimizeStreamMode = "sse" | "reconnecting" | "polling";

type OfficerSeat = "captain" | "bridge" | "below_decks" | "officer";

function seatAllowsRole(
  officerSeat: string | null | undefined,
  role: "captain_or_bridge" | "below_decks",
): boolean {
  if (!officerSeat) {
    return true;
  }
  const seat = officerSeat.toLowerCase() as OfficerSeat | string;
  switch (seat) {
    case "captain":
    case "bridge":
    case "officer":
      return role === "captain_or_bridge";
    case "below_decks":
      return role === "below_decks";
    default:
      return true;
  }
}

export function useWorkspace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { activeProfileId } = useProfile();
  const { ownedOnly } = useWorkspaceMode();

  // Scenario state
  const [shipTier, setShipTier] = useState(1);
  const [shipLevel, setShipLevel] = useState(50);
  const [shipId, setShipId] = useState("");
  const [scenarioId, setScenarioId] = useState("");
  /** Combat scenario for officer eligibility (empty = let the server infer from the target). */
  const [enemyType, setEnemyType] = useState("");
  /** Officer eligibility matrix for live crew-builder badges (fetched once; null until loaded). */
  const [eligibility, setEligibility] =
    useState<OfficerEligibilityResponse | null>(null);
  /** Per-ship below-decks unlock levels from API `crew_slots`; default matches typical STFC progression. */
  const [belowDeckUnlockLevels, setBelowDeckUnlockLevels] = useState<number[]>(
    () => [...DEFAULT_BELOW_DECK_UNLOCK_LEVELS],
  );

  // Crew state
  const [crew, setCrew] = useState<CrewState>(() =>
    createEmptyCrew(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );
  const [pins, setPins] = useState<PinsState>(() =>
    createEmptyPins(50, DEFAULT_BELOW_DECK_UNLOCK_LEVELS),
  );

  // Simulation state
  const [simResult, setSimResult] = useState<SimulateStats | null>(null);
  const [loadingSim, setLoadingSim] = useState(false);
  const [resultWarnings, setResultWarnings] = useState<string[]>([]);
  const [unresolvedOfficers, setUnresolvedOfficers] = useState<string[]>([]);

  // Optimization state
  const [recommendations, setRecommendations] = useState<CrewRecommendation[]>(
    [],
  );
  const [loadingOptimize, setLoadingOptimize] = useState(false);
  const [optimizeProgress, setOptimizeProgress] = useState<number | null>(null);
  const [optimizeCrewsDone, setOptimizeCrewsDone] = useState<number | null>(
    null,
  );
  const [optimizeTotalCrews, setOptimizeTotalCrews] = useState<number | null>(
    null,
  );
  const [optimizePhase, setOptimizePhase] = useState<string | null>(null);
  const [optimizeEtaSeconds, setOptimizeEtaSeconds] = useState<number | null>(
    null,
  );
  const [optimizeThroughput, setOptimizeThroughput] = useState<number | null>(
    null,
  );
  const [optimizePreview, setOptimizePreview] = useState<
    CrewRecommendation[] | null
  >(null);
  const [estimate, setEstimate] = useState<OptimizeEstimate | null>(null);
  const [lastOptimizeDurationMs, setLastOptimizeDurationMs] = useState<
    number | null
  >(null);
  const [lastOptimizeEffectiveStrategy, setLastOptimizeEffectiveStrategy] =
    useState<string | null>(null);
  /** True after last completed optimize reused profile disk cache for tiered scout/confirm. */
  const [cachedWarmStartBadge, setCachedWarmStartBadge] = useState(false);

  // Optimization parameters
  const [simsPerCrew, setSimsPerCrew] = useState(5000);
  const [maxCandidates, setMaxCandidates] = useState<number | null>(100);
  const [belowDecksPoolMode, setBelowDecksPoolMode] =
    useState<import("./optimizeWarmStart").BelowDecksPoolMode>("strict");

  // Optimizer strategy
  const [optimizerStrategy, setOptimizerStrategy] =
    useState<import("./api").OptimizerStrategyType>("tiered");
  /** Tiered: scout sims per crew; null = omit (server default 500). */
  const [tieredScoutSims, setTieredScoutSims] = useState<number | null>(null);
  /** Tiered: top K for confirmation; null = omit (server default 20). */
  const [tieredTopK, setTieredTopK] = useState<number | null>(null);
  /** Tiered: random exploration slice fraction (0, 0.5]; null = omit (off). */
  const [tieredRandomExplorationPct, setTieredRandomExplorationPct] = useState<
    number | null
  >(null);
  /** Analytical prefilter: learned pair co-occurrence prior (default on). */
  const [enableLearnedPairPrior, setEnableLearnedPairPrior] = useState(true);

  /** Novelty (MMR): optional text fields; blank lambda = omit (pure strength order). */
  const [noveltyLambdaText, setNoveltyLambdaText] = useState("");
  const [noveltyDiverseTopText, setNoveltyDiverseTopText] = useState("");
  const [noveltyPoolText, setNoveltyPoolText] = useState("");
  /** MMR: penalize overlap with optimize_history top crews (profile + warm-start cache key). */
  const [noveltyHistoryAnchors, setNoveltyHistoryAnchors] = useState(false);

  // Alliance / ship support buffs (UI + request payload; combat application TBD)
  const [selectedSupportBuffs, setSelectedSupportBuffs] = useState<
    SupportBuffId[]
  >([]);
  const setValidatedSelectedSupportBuffs = (ids: readonly string[]) => {
    setSelectedSupportBuffs(normalizeSupportBuffSelection(ids).ids);
  };

  // Heuristics state
  const [availableSeeds, setAvailableSeeds] = useState<string[]>([]);
  const [selectedSeeds, setSelectedSeeds] = useState<string[]>([]);
  const [heuristicsOnly, setHeuristicsOnly] = useState(false);
  /** Merge selected heuristic seeds into main optimize warm-start (API `fast_discovery`). */
  const [fastDiscovery, setFastDiscovery] = useState(false);
  const [belowDecksStrategy, setBelowDecksStrategy] = useState<
    "ordered" | "exploration"
  >("ordered");

  /** Chain grind: N sequential wins, hull carries, shields full each link. */
  const [chainGrindEnabled, setChainGrindEnabled] = useState(false);
  const [chainKillsTarget, setChainKillsTarget] = useState(3);
  const [chainSecondary, setChainSecondary] = useState<
    "min_hull_damage" | "max_loot_per_hull_proxy"
  >("min_hull_damage");

  // Optimize constraints (comma-separated lists; groups JSON optional)
  const [optimizeMustInclude, setOptimizeMustInclude] = useState("");
  const [optimizeExclude, setOptimizeExclude] = useState("");
  const [optimizeCaptainMust, setOptimizeCaptainMust] = useState("");
  const [optimizeBridgeMust, setOptimizeBridgeMust] = useState("");
  const [optimizeBelowMust, setOptimizeBelowMust] = useState("");
  const [optimizeGroupsJson, setOptimizeGroupsJson] = useState("");

  const optimizeWarmStartCacheKey = () =>
    buildOptimizeWarmStartKey({
      profileId: activeProfileId,
      shipId,
      scenarioId,
      shipTier,
      shipLevel,
      maxCandidates,
      constraintsFingerprint: [
        optimizeMustInclude,
        optimizeExclude,
        optimizeCaptainMust,
        optimizeBridgeMust,
        optimizeBelowMust,
        optimizeGroupsJson,
      ].join("\u001f"),
      defenderOpponent: "Hostile",
      supportBuffIds: selectedSupportBuffs,
      chainGrindEnabled: chainGrindEnabled,
      chainKillsTarget: chainKillsTarget,
      chainSecondary: chainSecondary,
      belowDecksPoolMode,
      belowDecksSlots: belowDeckSlotCount(shipLevel, belowDeckUnlockLevels),
      fastDiscovery,
      enableLearnedPairPrior,
    });

  // Preset saving state
  const [showSavePreset, setShowSavePreset] = useState(false);
  const [savePresetName, setSavePresetName] = useState("");
  const [savingPreset, setSavingPreset] = useState(false);

  // UI state
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(false);
  const [error, setErrorState] = useState<string | null>(null);
  /** When `error` is set from an API failure, distinguishes capacity warnings from hard failures. */
  const [errorSeverity, setErrorSeverity] = useState<
    "error" | "warning" | null
  >(null);
  /** Non-error feedback (e.g. after cancel). */
  const [workspaceInfo, setWorkspaceInfo] = useState<string | null>(null);
  const [officerSeatById, setOfficerSeatById] = useState<Map<string, string>>(
    new Map(),
  );
  const [rosterLoadState, setRosterLoadState] = useState<
    "idle" | "loading" | "done" | "error"
  >("idle");
  const estimateDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const setError = (message: string | null) => {
    setErrorState(message);
    setErrorSeverity(message === null ? null : "error");
  };

  const setErrorFromApiException = (e: unknown) => {
    setErrorState(formatApiError(e));
    setErrorSeverity(
      e instanceof ApiError && e.code === API_ERROR_CPU_BUSY
        ? "warning"
        : "error",
    );
  };
  /** How progress updates are delivered while optimizing. */
  const [optimizeStreamMode, setOptimizeStreamMode] =
    useState<OptimizeStreamMode | null>(null);

  // Polling ref, SSE ref, and current job id (for cancel + cleanup on unmount)
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const currentOptimizeJobIdRef = useRef<string | null>(null);
  const sseReconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const sseAttemptRef = useRef(0);
  const usePollingOnlyRef = useRef(false);

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
      navigate(".", { replace: true, state: {} });
    }
  }, [location.state, navigate]);

  // Close SSE, reconnect timers, and polling on unmount
  useEffect(() => {
    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      if (sseReconnectTimerRef.current) {
        clearTimeout(sseReconnectTimerRef.current);
        sseReconnectTimerRef.current = null;
      }
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, []);

  // Fetch optimize estimate when parameters change (debounced 350ms to avoid
  // firing on every keystroke while typing sims/tier values).
  useEffect(() => {
    const ship = shipId || "Saladin";
    const hostile = scenarioId || "2918121098";
    if (!ship || !hostile) {
      setEstimate(null);
      return;
    }

    if (estimateDebounceRef.current) {
      clearTimeout(estimateDebounceRef.current);
    }

    estimateDebounceRef.current = setTimeout(() => {
      estimateDebounceRef.current = null;
      getOptimizeEstimate(
        {
          ship,
          hostile,
          sims: simsPerCrew,
          max_candidates: maxCandidates ?? undefined,
          below_decks_pool_mode:
            belowDecksPoolMode !== "strict" ? belowDecksPoolMode : undefined,
          ship_tier: shipTier > 0 ? shipTier : undefined,
          ship_level: shipLevel > 0 ? shipLevel : undefined,
        },
        activeProfileId,
      )
        .then((data) => setEstimate(data))
        .catch(() => setEstimate(null));
    }, 350);

    return () => {
      if (estimateDebounceRef.current) {
        clearTimeout(estimateDebounceRef.current);
        estimateDebounceRef.current = null;
      }
    };
  }, [
    shipId,
    scenarioId,
    simsPerCrew,
    maxCandidates,
    belowDecksPoolMode,
    shipTier,
    shipLevel,
    activeProfileId,
  ]);

  // Fetch available heuristic seeds
  useEffect(() => {
    fetchHeuristics()
      .then(setAvailableSeeds)
      .catch(() => setAvailableSeeds([]));
  }, []);

  // Fetch the officer eligibility matrix once (static, profile-independent) for crew badges.
  useEffect(() => {
    fetchOfficerEligibility()
      .then(setEligibility)
      .catch(() => setEligibility(null));
  }, []);

  useEffect(() => {
    let cancelled = false;
    setRosterLoadState("loading");
    fetchOfficers(ownedOnly, activeProfileId)
      .then((list) => {
        if (cancelled) {
          return;
        }
        const next = new Map<string, string>();
        for (const officer of list) {
          if (officer.id.trim()) {
            next.set(officer.id, officer.slot ?? "");
          }
        }
        setOfficerSeatById(next);
        setRosterLoadState("done");
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setOfficerSeatById(new Map());
        setRosterLoadState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [ownedOnly, activeProfileId]);

  useEffect(() => {
    if (optimizerStrategy === "genetic" && fastDiscovery)
      setFastDiscovery(false);
    // Deliberately omit fastDiscovery from deps to avoid self-referencing
    // re-trigger when the effect itself sets fastDiscovery to false.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [optimizerStrategy]);

  useEffect(() => {
    if (heuristicsOnly && fastDiscovery) setFastDiscovery(false);
    // Deliberately omit fastDiscovery from deps to avoid self-referencing
    // re-trigger when the effect itself sets fastDiscovery to false.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [heuristicsOnly]);

  // Sync crew/pins with ship level and per-ship below-decks unlock schedule
  useEffect(() => {
    const n = belowDeckSlotCount(shipLevel, belowDeckUnlockLevels);
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
  }, [shipLevel, belowDeckUnlockLevels]);

  // Handle running a simulation
  const rosterAnchoringError = (): string | null => {
    const bdSlots = belowDeckSlotCount(shipLevel, belowDeckUnlockLevels);
    const selectedCrewIds = [
      crew.captain,
      ...crew.bridge,
      ...crew.belowDeck.slice(0, bdSlots),
    ]
      .filter(
        (id): id is string => typeof id === "string" && id.trim().length > 0,
      )
      .map((id) => id.trim());

    const uniqueCrewIds = new Set(selectedCrewIds);
    if (uniqueCrewIds.size !== selectedCrewIds.length) {
      return "Crew slots contain duplicate officers. Each officer can only appear once.";
    }

    if (
      ownedOnly &&
      selectedCrewIds.length > 0 &&
      rosterLoadState === "loading"
    ) {
      return "Roster data is still loading. Wait for owned officers to finish loading, then retry.";
    }

    if (
      ownedOnly &&
      selectedCrewIds.length > 0 &&
      rosterLoadState === "error"
    ) {
      return "Could not load owned officers for this profile. Retry after syncing roster data.";
    }

    const validateSeat = (
      officerId: string | null,
      role: "captain_or_bridge" | "below_decks",
      label: string,
    ): string | null => {
      if (!officerId) {
        return null;
      }
      const slot = officerSeatById.get(officerId);
      if (ownedOnly && !slot) {
        return `${label} officer is not in the active profile roster.`;
      }
      if (slot && !seatAllowsRole(slot, role)) {
        return `${label} officer is not legal for that seat.`;
      }
      return null;
    };

    const captainErr = validateSeat(
      crew.captain,
      "captain_or_bridge",
      "Captain",
    );
    if (captainErr) {
      return captainErr;
    }
    const bridgeOneErr = validateSeat(
      crew.bridge[0],
      "captain_or_bridge",
      "Bridge 1",
    );
    if (bridgeOneErr) {
      return bridgeOneErr;
    }
    const bridgeTwoErr = validateSeat(
      crew.bridge[1],
      "captain_or_bridge",
      "Bridge 2",
    );
    if (bridgeTwoErr) {
      return bridgeTwoErr;
    }
    for (let i = 0; i < bdSlots; i += 1) {
      const err = validateSeat(
        crew.belowDeck[i] ?? null,
        "below_decks",
        `Below deck ${i + 1}`,
      );
      if (err) {
        return err;
      }
    }

    return null;
  };

  const handleRunSim = async () => {
    const preflightError = rosterAnchoringError();
    if (preflightError) {
      setError(preflightError);
      return;
    }
    const simParams = buildWorkspaceSimulateParams({
      shipId,
      scenarioId,
      crew,
      simsPerCrew,
      shipTier,
      shipLevel,
      supportBuffs: selectedSupportBuffs,
      enemyType: enemyType || undefined,
    });
    if (!simParams) {
      setError("Select a captain first");
      return;
    }
    setError(null);
    setWorkspaceInfo(null);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
    setLoadingSim(true);
    try {
      const res = await simulate(simParams, activeProfileId);
      setSimResult(res.stats);
      setRecommendations([]);
      setResultWarnings(res.warnings ?? []);
      setUnresolvedOfficers(res.unresolved_officers ?? []);
    } catch (e) {
      setErrorFromApiException(e);
    } finally {
      setLoadingSim(false);
    }
  };

  const resetResults = () => {
    setSimResult(null);
    setRecommendations([]);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
    setLastOptimizeDurationMs(null);
    setLastOptimizeEffectiveStrategy(null);
  };

  const applyOptimizeDone = (status: OptimizeStatusResponse) => {
    clearPersistedOptimizeJob();
    setOptimizeStreamMode(null);
    currentOptimizeJobIdRef.current = null;
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    if (sseReconnectTimerRef.current) {
      clearTimeout(sseReconnectTimerRef.current);
      sseReconnectTimerRef.current = null;
    }
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
    usePollingOnlyRef.current = false;
    sseAttemptRef.current = 0;
    if (status.status === "done" && status.result) {
      setRecommendations(status.result.recommendations ?? []);
      setResultWarnings(status.result.warnings ?? []);
      setUnresolvedOfficers([]);
      setLastOptimizeEffectiveStrategy(
        status.result.scenario?.effective_strategy ?? null,
      );
      saveWarmStartFromRecommendations(
        optimizeWarmStartCacheKey(),
        status.result.recommendations ?? [],
      );
      const hits = status.result.scenario?.optimize_history_confirm_hits ?? 0;
      setCachedWarmStartBadge(typeof hits === "number" && hits > 0);
      setSimResult(null);
      if (status.result.duration_ms != null)
        setLastOptimizeDurationMs(status.result.duration_ms);
    } else if (status.status === "error") {
      const detail = status.error?.trim() || "Unknown error";
      setError(`Optimization failed: ${detail}`);
      setCachedWarmStartBadge(false);
    }
    setLoadingOptimize(false);
    setOptimizeProgress(null);
    setOptimizeCrewsDone(null);
    setOptimizeTotalCrews(null);
    setOptimizePhase(null);
    setOptimizeEtaSeconds(null);
    setOptimizeThroughput(null);
    setOptimizePreview(null);
  };

  const applyRunningOptimizeStatus = (status: OptimizeStatusResponse) => {
    if (status.progress != null) setOptimizeProgress(status.progress);
    if (status.crews_done != null) setOptimizeCrewsDone(status.crews_done);
    if (status.total_crews != null) setOptimizeTotalCrews(status.total_crews);
    setOptimizePhase(status.phase ?? null);
    setOptimizeEtaSeconds(
      status.eta_seconds !== undefined && status.eta_seconds !== null
        ? status.eta_seconds
        : null,
    );
    setOptimizeThroughput(
      status.throughput_crews_per_sec !== undefined &&
        status.throughput_crews_per_sec !== null
        ? status.throughput_crews_per_sec
        : null,
    );
    setOptimizePreview(status.progress_preview ?? null);
  };

  /** Subscribe to job progress (SSE with reconnect + backoff, then polling). */
  const beginOptimizeTracking = (jobId: string) => {
    currentOptimizeJobIdRef.current = jobId;
    persistOptimizeJob(jobId, activeProfileId);
    sseAttemptRef.current = 0;
    usePollingOnlyRef.current = false;
    if (sseReconnectTimerRef.current) {
      clearTimeout(sseReconnectTimerRef.current);
      sseReconnectTimerRef.current = null;
    }
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }

    const poll = () => {
      getOptimizeStatus(jobId)
        .then((status) => {
          applyRunningOptimizeStatus(status);
          if (status.status === "done" || status.status === "error") {
            applyOptimizeDone(status);
          }
        })
        .catch((e) => {
          clearPersistedOptimizeJob();
          currentOptimizeJobIdRef.current = null;
          if (pollIntervalRef.current) {
            clearInterval(pollIntervalRef.current);
            pollIntervalRef.current = null;
          }
          if (sseReconnectTimerRef.current) {
            clearTimeout(sseReconnectTimerRef.current);
            sseReconnectTimerRef.current = null;
          }
          const detail = formatApiError(e);
          setErrorState(
            `Could not read optimization status (${detail}). The job may still be running on the server — refresh the page to reconnect, or start a new optimization.`,
          );
          setErrorSeverity(
            e instanceof ApiError && e.code === API_ERROR_CPU_BUSY
              ? "warning"
              : "error",
          );
          setLoadingOptimize(false);
          setOptimizeStreamMode(null);
          setOptimizeProgress(null);
          setOptimizeCrewsDone(null);
          setOptimizeTotalCrews(null);
          setOptimizePhase(null);
          setOptimizeEtaSeconds(null);
          setOptimizeThroughput(null);
          setOptimizePreview(null);
        });
    };

    const startPollingOnly = () => {
      if (pollIntervalRef.current) return;
      usePollingOnlyRef.current = true;
      setOptimizeStreamMode("polling");
      poll();
      pollIntervalRef.current = setInterval(poll, POLL_INTERVAL_MS);
    };

    const openSse = () => {
      if (
        usePollingOnlyRef.current ||
        currentOptimizeJobIdRef.current !== jobId
      )
        return;
      if (typeof EventSource === "undefined") {
        startPollingOnly();
        return;
      }
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
        eventSourceRef.current = null;
      }
      const streamUrl = getOptimizeStreamUrl(jobId);
      const es = new EventSource(streamUrl);
      eventSourceRef.current = es;
      es.onmessage = (event) => {
        sseAttemptRef.current = 0;
        setOptimizeStreamMode("sse");
        try {
          const status = JSON.parse(event.data) as OptimizeStatusResponse;
          applyRunningOptimizeStatus(status);
          if (status.status === "done" || status.status === "error") {
            es.close();
            eventSourceRef.current = null;
            applyOptimizeDone(status);
          }
        } catch {
          /* ignore malformed chunk */
        }
      };
      es.onerror = () => {
        es.close();
        eventSourceRef.current = null;
        if (
          usePollingOnlyRef.current ||
          currentOptimizeJobIdRef.current !== jobId
        )
          return;
        sseAttemptRef.current += 1;
        const attempt = sseAttemptRef.current;
        if (attempt > MAX_SSE_RECONNECT_ATTEMPTS) {
          startPollingOnly();
          return;
        }
        setOptimizeStreamMode("reconnecting");
        const delayMs = Math.min(
          SSE_BACKOFF_CAP_MS,
          SSE_BACKOFF_BASE_MS * 2 ** (attempt - 1),
        );
        sseReconnectTimerRef.current = setTimeout(() => {
          sseReconnectTimerRef.current = null;
          if (
            currentOptimizeJobIdRef.current !== jobId ||
            usePollingOnlyRef.current
          )
            return;
          openSse();
        }, delayMs);
      };
    };

    openSse();
  };

  // Resume in-flight job after refresh (same tab session + matching profile).
  useEffect(() => {
    const persisted = readPersistedOptimizeJob();
    if (
      !persisted ||
      !profileMatchesPersisted(activeProfileId, persisted.profileId)
    ) {
      return;
    }
    if (currentOptimizeJobIdRef.current === persisted.jobId) {
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const status = await getOptimizeStatus(persisted.jobId);
        if (cancelled) return;
        if (status.status === "done" || status.status === "error") {
          clearPersistedOptimizeJob();
          applyRunningOptimizeStatus(status);
          applyOptimizeDone(status);
          return;
        }
        setLoadingOptimize(true);
        applyRunningOptimizeStatus(status);
        beginOptimizeTracking(persisted.jobId);
      } catch {
        if (!cancelled) clearPersistedOptimizeJob();
      }
    })();

    return () => {
      cancelled = true;
    };
    // Re-check when profile id stabilizes after load (persisted job is per-profile).
  }, [activeProfileId]);

  // Handle running optimization
  const handleRunOptimize = async () => {
    const preflightError = rosterAnchoringError();
    if (preflightError) {
      setError(preflightError);
      return;
    }
    setError(null);
    setWorkspaceInfo(null);
    setCachedWarmStartBadge(false);
    setResultWarnings([]);
    setUnresolvedOfficers([]);
    setLoadingOptimize(true);
    setLastOptimizeDurationMs(null);
    setOptimizeProgress(0);
    setOptimizeCrewsDone(0);
    setOptimizeTotalCrews(null);
    setOptimizePhase(null);
    setOptimizeEtaSeconds(null);
    setOptimizeThroughput(null);
    setOptimizePreview(null);
    setOptimizeStreamMode(null);
    try {
      const { job_id } = await optimizeStart(
        buildWorkspaceOptimizeStartBody({
          shipId,
          scenarioId,
          simsPerCrew,
          maxCandidates,
          optimizerStrategy,
          belowDecksPoolMode,
          selectedSeeds,
          heuristicsOnly,
          belowDecksStrategy,
          shipTier,
          shipLevel,
          belowDecksSlots: belowDeckSlotCount(shipLevel, belowDeckUnlockLevels),
          supportBuffs: selectedSupportBuffs,
          enemyType: enemyType || undefined,
          optimizeConstraints: {
            mustIncludeComma: optimizeMustInclude,
            excludeComma: optimizeExclude,
            captainMust: optimizeCaptainMust,
            bridgeMustComma: optimizeBridgeMust,
            belowMustComma: optimizeBelowMust,
            groupsJson: optimizeGroupsJson,
          },
          chainGrind: chainGrindEnabled
            ? {
                enabled: true,
                kills_target: Math.min(50, Math.max(1, chainKillsTarget)),
                secondary:
                  chainSecondary === "max_loot_per_hull_proxy"
                    ? chainSecondary
                    : undefined,
              }
            : undefined,
          warmStartCrews:
            loadWarmStartCrews(optimizeWarmStartCacheKey()) ?? undefined,
          tieredScoutSims,
          tieredTopK,
          tieredRandomExplorationPct,
          fastDiscovery:
            fastDiscovery && selectedSeeds.length > 0 ? true : undefined,
          noveltyLambdaText,
          noveltyDiverseTopText,
          noveltyPoolText,
          noveltyHistoryAnchors:
            noveltyHistoryAnchors &&
            activeProfileId != null &&
            activeProfileId !== ""
              ? true
              : undefined,
          optimizeCacheKey:
            activeProfileId != null && activeProfileId !== ""
              ? optimizeWarmStartCacheKey()
              : undefined,
          enableLearnedPairPrior,
        }),
        activeProfileId,
        {
          onCpuBusyWait: ({ waitMs, attempt }) => {
            const sec = Math.max(1, Math.round(waitMs / 1000));
            setWorkspaceInfo(
              `Server CPU is busy (another simulation or optimization is running). Automatically retrying in ~${sec}s (attempt ${attempt})…`,
            );
          },
        },
      );
      setWorkspaceInfo(null);
      beginOptimizeTracking(job_id);
    } catch (e) {
      clearPersistedOptimizeJob();
      setWorkspaceInfo(null);
      setErrorFromApiException(e);
      setLoadingOptimize(false);
      setOptimizeStreamMode(null);
      setOptimizeProgress(null);
      setOptimizeCrewsDone(null);
      setOptimizeTotalCrews(null);
      setOptimizePhase(null);
      setOptimizeEtaSeconds(null);
      setOptimizeThroughput(null);
      setOptimizePreview(null);
    }
  };

  const handleCancelOptimize = () => {
    setCachedWarmStartBadge(false);
    const jobId = currentOptimizeJobIdRef.current;
    clearPersistedOptimizeJob();
    currentOptimizeJobIdRef.current = null;
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    if (sseReconnectTimerRef.current) {
      clearTimeout(sseReconnectTimerRef.current);
      sseReconnectTimerRef.current = null;
    }
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
    usePollingOnlyRef.current = false;
    sseAttemptRef.current = 0;
    if (jobId) {
      cancelOptimizeJob(jobId).catch(() => {});
    }
    setWorkspaceInfo(
      "Optimization cancelled. The server may still finish this job in the background; wait a moment before starting another run on the same scenario if you see odd results.",
    );
    setLoadingOptimize(false);
    setOptimizeStreamMode(null);
    setOptimizeProgress(null);
    setOptimizeCrewsDone(null);
    setOptimizeTotalCrews(null);
    setOptimizePhase(null);
    setOptimizeEtaSeconds(null);
    setOptimizeThroughput(null);
    setOptimizePreview(null);
  };

  // Handle saving a preset
  const handleSavePreset = async () => {
    setError(null);
    setWorkspaceInfo(null);
    setSavingPreset(true);
    try {
      await savePreset(
        {
          name: savePresetName || "Unnamed",
          ship: shipId || "Saladin",
          scenario: scenarioId || "2918121098",
          crew: {
            captain: crew.captain,
            bridge: crew.bridge,
            below_deck: crew.belowDeck,
          },
        },
        activeProfileId,
      );
      setShowSavePreset(false);
      setSavePresetName("");
    } catch (e) {
      setErrorFromApiException(e);
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
    enemyType,
    setEnemyType,
    eligibility,
    shipTier,
    setShipTier,
    shipLevel,
    setShipLevel,
    belowDeckUnlockLevels,
    setBelowDeckUnlockLevels,
    // Crew
    crew,
    setCrew,
    pins,
    setPins,
    // Simulation
    simResult,
    resetResults,
    resultWarnings,
    unresolvedOfficers,
    loadingSim,
    handleRunSim,
    // Optimization
    recommendations,
    loadingOptimize,
    handleRunOptimize,
    handleCancelOptimize,
    optimizeProgress,
    optimizeCrewsDone,
    optimizeTotalCrews,
    optimizePhase,
    optimizeEtaSeconds,
    optimizeThroughput,
    optimizePreview,
    estimate,
    lastOptimizeDurationMs,
    lastOptimizeEffectiveStrategy,
    cachedWarmStartBadge,
    // Optimization parameters
    simsPerCrew,
    setSimsPerCrew,
    maxCandidates,
    setMaxCandidates,
    belowDecksPoolMode,
    setBelowDecksPoolMode,
    selectedSupportBuffs,
    setSelectedSupportBuffs: setValidatedSelectedSupportBuffs,
    // Heuristics
    availableSeeds,
    selectedSeeds,
    setSelectedSeeds,
    heuristicsOnly,
    setHeuristicsOnly,
    fastDiscovery,
    setFastDiscovery,
    belowDecksStrategy,
    setBelowDecksStrategy,
    chainGrindEnabled,
    setChainGrindEnabled,
    chainKillsTarget,
    setChainKillsTarget,
    chainSecondary,
    setChainSecondary,
    optimizerStrategy,
    setOptimizerStrategy,
    tieredScoutSims,
    setTieredScoutSims,
    tieredTopK,
    setTieredTopK,
    tieredRandomExplorationPct,
    setTieredRandomExplorationPct,
    enableLearnedPairPrior,
    setEnableLearnedPairPrior,
    noveltyLambdaText,
    setNoveltyLambdaText,
    noveltyDiverseTopText,
    setNoveltyDiverseTopText,
    noveltyPoolText,
    setNoveltyPoolText,
    noveltyHistoryAnchors,
    setNoveltyHistoryAnchors,
    optimizeMustInclude,
    setOptimizeMustInclude,
    optimizeExclude,
    setOptimizeExclude,
    optimizeCaptainMust,
    setOptimizeCaptainMust,
    optimizeBridgeMust,
    setOptimizeBridgeMust,
    optimizeBelowMust,
    setOptimizeBelowMust,
    optimizeGroupsJson,
    setOptimizeGroupsJson,
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
    errorSeverity,
    setError,
    workspaceInfo,
    setWorkspaceInfo,
    optimizeStreamMode,
    activeProfileId,
  };
}
