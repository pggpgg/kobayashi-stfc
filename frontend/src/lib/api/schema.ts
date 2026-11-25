/**
 * Types derived from the bundled OpenAPI spec (`docs/openapi/kobayashi-openapi.yaml`).
 * Regenerate with `npm run gen:api` from `frontend/`.
 */
import type { components } from "./generated";

export type { components, paths } from "./generated";

type Schemas = components["schemas"];

export type ProfileEntry = Schemas["ProfileEntry"];
/** List profiles response (`GET /api/profiles`). */
export type ProfilesResponse = Schemas["ProfilesListResponse"];

export type OfficerListItem =
  Schemas["OfficersListResponse"]["officers"][number];
export type ShipListItem = Schemas["ShipsListResponse"]["ships"][number];
export type HostileListItem =
  Schemas["HostilesListResponse"]["hostiles"][number];
export type ShipTiersLevels = Schemas["ShipTiersLevelsResponse"];

export type MechanicStatus =
  Schemas["DataVersionResponse"]["mechanics"][number];
export type DataVersionResponse = Schemas["DataVersionResponse"];
export type MechanicsCoverageResponse = Schemas["MechanicsCoverageResponse"];
export type MechanicsTierCounts = Schemas["MechanicsTierCounts"];
export type MechanicsFidelityBacklogItem =
  Schemas["MechanicsFidelityBacklogItem"];

export type SimulateCrew = Schemas["SimulateCrew"];
export type SimulateStats = Schemas["SimulateStats"];
export type SimulateResponse = Schemas["SimulateResponse"];
export type ResolvedContextSummary = Schemas["ResolvedContextSummary"];

export type OptimizeStartResponse = Schemas["OptimizeStartResponse"];
export type OptimizeEstimate = Schemas["OptimizeEstimateResponse"];

export type ChainSecondaryObjective = Schemas["ChainSecondaryObjective"];
export type ChainLootSummary = Schemas["ChainLootSummary"];
export type ChainSimulationSummary = Schemas["ChainSimulationSummary"];
export type RefinementSlotChange = Schemas["RefinementSlotChange"];
export type RefinementDetail = Schemas["RefinementDetail"];
export type CrewRecommendation = Schemas["CrewRecommendation"];
export type ChainGrindRequestBody = Schemas["ChainGrindRequest"];
export type OptimizerFunnelTelemetry = Schemas["OptimizerFunnelTelemetry"];
export type WarmStartCrewBody = Schemas["WarmStartCrewDto"];
export type OptimizeResponse = Schemas["OptimizeResponse"];
export type OptimizeStatusResponse = Schemas["OptimizeStatusResponse"];
export type OptimizeCrewConstraintsBody = Schemas["OptimizeConstraintsDto"];
export type OptimizeRequest = Schemas["OptimizeRequest"];

export type CounterfactualMetrics = Schemas["CounterfactualMetrics"];
export type PairedDelta = Schemas["PairedDelta"];
export type SeatReplacementOption = Schemas["SeatReplacementOption"];
export type SeatAblation = Schemas["SeatAblation"];
export type SubstituteSlotChange = Schemas["SubstituteSlotChange"];
export type SubstituteCrewPlan = Schemas["SubstituteCrewPlan"];
export type CrewCounterfactualRequest = Schemas["CrewCounterfactualRequest"];
export type CrewCounterfactualResponse = Schemas["CrewCounterfactualResponse"];

export type FleetSlotRequestDto = Schemas["FleetSlotRequestDto"];
export type FleetAllocateRequest = Schemas["FleetAllocateRequest"];
export type FleetCrewSummary = Schemas["FleetCrewSummary"];
export type FleetSlotAssignment = Schemas["FleetSlotAssignment"];
export type ContestedOfficerReport = Schemas["ContestedOfficerReport"];
export type FleetAllocateResponse = Schemas["FleetAllocateResponse"];
