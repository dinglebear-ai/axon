import type { EnvGroup, TomlConfigGroup } from "./configTypes";

export function deriveConfigModel(envGroups: EnvGroup[], configGroups: TomlConfigGroup[]) {
  const ENV_DEFAULTS = Object.fromEntries(
    envGroups.flatMap((group) => group.vars.map((field) => [field.key, field.def])),
  );
  const CONFIG_DEFAULTS = Object.fromEntries(
    configGroups.flatMap((group) =>
      group.knobs.map((knob) => [
        `${group.section.replace(/^\[/, "").replace(/\]$/, "")}.${knob.key}`,
        knob.def,
      ]),
    ),
  );
  const ENV_COUNT = envGroups.reduce((count, group) => count + group.vars.length, 0);
  const CONFIG_COUNT = configGroups.reduce((count, group) => count + group.knobs.length, 0);
  return { ENV_DEFAULTS, CONFIG_DEFAULTS, ENV_COUNT, CONFIG_COUNT };
}
