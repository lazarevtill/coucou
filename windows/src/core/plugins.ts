// Switched-on built-ins and plugins, as the settings hold them: one map, keyed
// by id. Built-ins are `integration_*`; plugins from the folder also carry the
// hash of the manifest the user approved.

export interface PluginSetting {
  enabled: boolean;
  approvedHash?: string;
}

export type PluginMap = Record<string, PluginSetting>;

/** Pills next to Mochi: at most this many built-ins at once. */
export const MAX_BUILTINS = 4;

export function isOn(map: PluginMap, id: string): boolean {
  return map[id]?.enabled === true;
}

/** A copy of `map` with `id` switched on or off; an approval stays as it was. */
export function withOn(map: PluginMap, id: string, on: boolean): PluginMap {
  return { ...map, [id]: { ...map[id], enabled: on } };
}

/** The built-ins switched on, sorted. */
export function builtinsOn(map: PluginMap): string[] {
  return Object.keys(map)
    .filter((id) => id.startsWith("integration_") && isOn(map, id))
    .sort();
}
