export type ToolsSettings = {
  gamelogs_dir: string | null;
  fc_character: string | null;
  ammo_launchers: number;
  ammo_per_launcher: number;
};

export type ToolsSettingsPatch = {
  gamelogs_dir?: string | null;
  fc_character?: string | null;
  ammo_launchers?: number;
  ammo_per_launcher?: number;
};

export type ToolsSettingsInvoke = <T>(
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export async function getToolsSettings(
  invoke: ToolsSettingsInvoke,
): Promise<ToolsSettings> {
  return invoke<ToolsSettings>("get_tools_settings");
}

export async function setToolsSettings(
  invoke: ToolsSettingsInvoke,
  patch: ToolsSettingsPatch,
): Promise<ToolsSettings> {
  return invoke<ToolsSettings>("set_tools_settings", { patch });
}
