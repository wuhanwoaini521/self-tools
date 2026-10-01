import { describe, expect, it, vi } from "vitest";
import { allThemes, getTheme } from "./theme/ThemeManager";
import "./theme/themes";

describe("Theme Registry", () => {
  it("includes all registered themes including Nord and Catppuccin", () => {
    const themes = allThemes();
    const ids = themes.map((t) => t.id);

    expect(ids).toContain("default");
    expect(ids).toContain("warm-editorial");
    expect(ids).toContain("warm-editorial-dark");
    expect(ids).toContain("nord");
    expect(ids).toContain("catppuccin-macchiato");
  });

  it("provides preview colors for every theme", () => {
    const themes = allThemes();
    for (const theme of themes) {
      expect(theme.previewColors).toBeDefined();
      expect(theme.previewColors?.length).toBe(3);
    }
  });

  it("correctly falls back to default on unknown theme", () => {
    const theme = getTheme("unknown-custom-theme");
    expect(theme.id).toBe("default");
  });

  it("retrieves Nord and Catppuccin definitions", () => {
    const nord = getTheme("nord");
    expect(nord.name).toBe("Nord");
    expect(nord.appearance).toBe("dark");

    const catppuccin = getTheme("catppuccin-macchiato");
    expect(catppuccin.name).toBe("Catppuccin Macchiato");
    expect(catppuccin.appearance).toBe("dark");
  });
});
