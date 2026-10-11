import { afterEach, describe, expect, it } from "bun:test";
import { nativeBridge } from "./native";

afterEach(() => {
  void nativeBridge.updateSettings({ speechEngine: "system", desertAntUsageAccepted: false });
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
    await nativeBridge.updateSettings({ desertAntUsageAccepted: true });
    await nativeBridge.downloadVozModel();
    expect((await nativeBridge.getVozModelStatus()).phase).toBe("ready");
    expect(phases).toContain("downloading");
    expect(phases).toContain("preparing");
    await nativeBridge.deleteVozModel();
    expect((await nativeBridge.getVozModelStatus()).phase).toBe("notDownloaded");
    stop();
  });
});

it("blocks download and dictation until the usage disclosure is acknowledged", async () => {
  await nativeBridge.updateSettings({ speechEngine: "voz", desertAntUsageAccepted: false });
  await expect(nativeBridge.downloadVozModel()).rejects.toThrow("Acknowledge");
  await expect(nativeBridge.startDictation()).rejects.toThrow("Acknowledge");
});
