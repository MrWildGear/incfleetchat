import { describe, expect, it } from "vitest";
import {
  getToolsSettings,
  setToolsSettings,
  type ToolsSettings,
  type ToolsSettingsInvoke,
} from "./toolsSettings";

describe("toolsSettings", () => {
  it("getToolsSettings invokes get_tools_settings", async () => {
    const snap: ToolsSettings = {
      gamelogs_dir: "C:/logs",
      fc_character: "FC",
      ammo_launchers: 6,
      ammo_per_launcher: 26,
    };
    const calls: { cmd: string; args?: unknown }[] = [];
    const invoke: ToolsSettingsInvoke = async (cmd, args) => {
      calls.push({ cmd, args });
      return snap as never;
    };
    await expect(getToolsSettings(invoke)).resolves.toEqual(snap);
    expect(calls).toEqual([{ cmd: "get_tools_settings", args: undefined }]);
  });

  it("setToolsSettings passes a typed patch", async () => {
    const after: ToolsSettings = {
      gamelogs_dir: null,
      fc_character: "FC",
      ammo_launchers: 7,
      ammo_per_launcher: 20,
    };
    const calls: { cmd: string; args?: unknown }[] = [];
    const invoke: ToolsSettingsInvoke = async (cmd, args) => {
      calls.push({ cmd, args });
      return after as never;
    };
    await expect(
      setToolsSettings(invoke, { ammo_launchers: 7, ammo_per_launcher: 20 }),
    ).resolves.toEqual(after);
    expect(calls).toEqual([
      {
        cmd: "set_tools_settings",
        args: { patch: { ammo_launchers: 7, ammo_per_launcher: 20 } },
      },
    ]);
  });
});
