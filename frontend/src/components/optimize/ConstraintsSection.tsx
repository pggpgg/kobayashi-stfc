import type { OfficerListItem } from "../../lib/api";
import {
  officerGroupsError,
  readOfficerGroups,
} from "../../lib/optimizerGroups";
import type {
  OptimizerSettings,
  UpdateOptimizerSettings,
} from "../../lib/useOptimizerSettings";
import OfficerGroupEditor from "../OfficerGroupEditor";
import OfficerNameMultiSelect from "../OfficerNameMultiSelect";
import { CollapsibleSection, inputStyleCompact, settingUpdater } from "./ui";

interface Props {
  settings: Pick<
    OptimizerSettings,
    | "optimizeMustInclude"
    | "optimizeExclude"
    | "optimizeCaptainMust"
    | "optimizeBridgeMust"
    | "optimizeBelowMust"
    | "optimizeGroupsJson"
  >;
  onChange: UpdateOptimizerSettings;
  officerOptions: OfficerListItem[];
}
export default function ConstraintsSection({
  settings,
  onChange,
  officerOptions,
}: Props) {
  const {
    optimizeMustInclude,
    optimizeExclude,
    optimizeCaptainMust,
    optimizeBridgeMust,
    optimizeBelowMust,
    optimizeGroupsJson,
  } = settings;
  const update = settingUpdater(onChange);
  const groups = readOfficerGroups(optimizeGroupsJson);
  const hasError = Boolean(groups.error ?? officerGroupsError(groups.groups));
  return (
    <CollapsibleSection title="Search constraints" initiallyOpen={hasError}>
      <p style={{ margin: 0, fontSize: "0.75rem", color: "var(--text-muted)" }}>
        Choose required or excluded officers, or require a minimum number from a
        group.
      </p>
      <label style={{ fontSize: "0.8rem" }}>
        Must include (any seat)
        <input
          type="text"
          value={optimizeMustInclude}
          onChange={(e) => update("optimizeMustInclude")(e.target.value)}
          placeholder="Officer A, Officer B"
          style={inputStyleCompact}
        />
      </label>
      <OfficerNameMultiSelect
        label={<span>Exclude</span>}
        officers={officerOptions}
        valueComma={optimizeExclude}
        onChangeComma={update("optimizeExclude")}
      />
      <OfficerNameMultiSelect
        label={<span>Captain must be</span>}
        officers={officerOptions}
        valueComma={optimizeCaptainMust}
        onChangeComma={update("optimizeCaptainMust")}
        placeholder="Search; multiple names mean captain can be any of them"
      />
      <OfficerNameMultiSelect
        label={<span>Bridge must include</span>}
        officers={officerOptions}
        valueComma={optimizeBridgeMust}
        onChangeComma={update("optimizeBridgeMust")}
      />
      <OfficerNameMultiSelect
        label={<span>Below-decks must include</span>}
        officers={officerOptions}
        valueComma={optimizeBelowMust}
        onChangeComma={update("optimizeBelowMust")}
      />
      <OfficerGroupEditor
        value={optimizeGroupsJson}
        onChange={update("optimizeGroupsJson")}
        officers={officerOptions}
      />
    </CollapsibleSection>
  );
}
