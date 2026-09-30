import { afterEach, describe, expect, it } from "bun:test";
import { nativeBridge } from "./native";

afterEach(() => {
  window.localStorage.removeItem("kivo-dev-settings");
});

describe("Voz bridge simulation", () => {
  it("persists Voz as a distinct engine choice in the browser harness", async () => {
    await nativeBridge.updateSettings({ speechEngine: "voz" });
    expect((await nativeBridge.getSettings()).speechEngine).toBe("voz");
  });

  it("simulates optional model install, progress, ready, and removal without SDK downloads", async () => {
    const phases: string[] = [];
    const stop = await nativeBridge.on("voz-model-status", (event) => phases.push(event.phase));
    await nativeBridge.downloadVozModel();
    expect((await nativeBridge.getVozModelStatus()).phase).toBe("ready");
    expect(phases).toContain("downloading");
    expect(phases).toContain("preparing");
    await nativeBridge.deleteVozModel();
    expect((await nativeBridge.getVozModelStatus()).phase).toBe("notDownloaded");
    stop();
  });
});
