import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import SavePresetModal from "./SavePresetModal";

describe("SavePresetModal", () => {
  it("renders nothing when closed", () => {
    const { container } = render(
      <SavePresetModal
        open={false}
        savePresetName=""
        onSavePresetNameChange={vi.fn()}
        savingPreset={false}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("calls onSave when name is non-empty", () => {
    const onSave = vi.fn();
    render(
      <SavePresetModal
        open
        savePresetName="My crew"
        onSavePresetNameChange={vi.fn()}
        savingPreset={false}
        onSave={onSave}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    expect(onSave).toHaveBeenCalled();
  });

  it("closes on Escape when not saving", () => {
    const onClose = vi.fn();
    render(
      <SavePresetModal
        open
        savePresetName=""
        onSavePresetNameChange={vi.fn()}
        savingPreset={false}
        onSave={vi.fn()}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
