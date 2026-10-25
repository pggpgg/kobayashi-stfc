import type { OfficerListItem } from "../lib/api";
import { officerGroupsError, readOfficerGroups } from "../lib/optimizerGroups";
import OfficerNameMultiSelect from "./OfficerNameMultiSelect";

interface Props {
  value: string;
  onChange: (value: string) => void;
  officers: OfficerListItem[];
}

export default function OfficerGroupEditor({
  value,
  onChange,
  officers,
}: Props) {
  const decoded = readOfficerGroups(value);
  const groups = decoded.groups;
  const error = decoded.error ?? officerGroupsError(groups);
  const update = (index: number, patch: Partial<(typeof groups)[number]>) => {
    onChange(
      JSON.stringify(
        groups.map((group, i) =>
          i === index ? { ...group, ...patch } : group,
        ),
      ),
    );
  };
  return (
    <div>
      <p style={{ fontSize: "0.75rem", color: "var(--text-muted)" }}>
        Require a minimum number of officers from each group, in any crew seats.
      </p>
      {error && (
        <p
          role="alert"
          style={{ color: "var(--danger, #e57373)", fontSize: "0.8rem" }}
        >
          {error}
        </p>
      )}
      {decoded.error ? (
        <button type="button" onClick={() => onChange("")}>
          Clear invalid groups
        </button>
      ) : (
        <>
          {groups.map((group, index) => (
            <fieldset
              key={index}
              style={{
                border: "1px solid var(--border)",
                margin: "0 0 0.75rem",
                padding: "0.6rem",
              }}
            >
              <legend>Officer group {index + 1}</legend>
              <OfficerNameMultiSelect
                label="Officers"
                valueNames={group.officers}
                onChangeNames={(names) => update(index, { officers: names })}
                officers={officers}
              />
              <label
                style={{ display: "block", fontSize: "0.8rem", marginTop: 8 }}
              >
                Minimum officers required
                <input
                  type="number"
                  min={1}
                  step={1}
                  max={Math.max(1, group.officers.length)}
                  value={group.min_count || ""}
                  onChange={(event) =>
                    update(index, { min_count: Number(event.target.value) })
                  }
                  style={{ width: "100%", display: "block" }}
                />
              </label>
              <button
                type="button"
                aria-label={`Remove officer group ${index + 1}`}
                onClick={() =>
                  onChange(JSON.stringify(groups.filter((_, i) => i !== index)))
                }
              >
                Remove group
              </button>
            </fieldset>
          ))}
          <button
            type="button"
            onClick={() =>
              onChange(
                JSON.stringify([...groups, { officers: [], min_count: 1 }]),
              )
            }
          >
            Add officer group
          </button>
        </>
      )}
    </div>
  );
}
