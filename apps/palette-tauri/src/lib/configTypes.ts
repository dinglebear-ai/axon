export type ConfigFieldType = "text" | "secret" | "bool" | "int" | "float" | "enum" | "list";

export interface ConfigField {
  key: string;
  type: ConfigFieldType;
  def: string | number | boolean | string[];
  desc: string;
  env?: string;
  options?: string[];
}

export interface EnvGroup {
  id: string;
  label: string;
  icon: string;
  note: string;
  vars: ConfigField[];
}

export interface TomlConfigGroup {
  id: string;
  section: string;
  label: string;
  icon: string;
  note: string;
  knobs: ConfigField[];
}
