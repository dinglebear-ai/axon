import type { JsonSchema } from "../clients/labbyClient";

export type SchemaField = {
  name: string;
  type: "string" | "number" | "integer" | "boolean" | "object" | "array";
  required: boolean;
  description: string;
  enumValues?: unknown[];
};

export function schemaFields(schema: JsonSchema | null): {
  fields: SchemaField[];
  rawOnlyReason: string | null;
} {
  if (!schema)
    return { fields: [], rawOnlyReason: "This tool has no form schema; use validated raw JSON." };
  const encoded = JSON.stringify(schema);
  if (encoded.includes('"$ref"') || encoded.includes('"$recursiveRef"'))
    return { fields: [], rawOnlyReason: "Recursive or referenced schemas require raw JSON." };
  if (schema.oneOf || schema.anyOf || schema.allOf)
    return { fields: [], rawOnlyReason: "Schema unions require raw JSON." };
  if (schema.additionalProperties && schema.additionalProperties !== false)
    return { fields: [], rawOnlyReason: "Open-ended object schemas require raw JSON." };
  const properties = schema.properties;
  if (!properties || typeof properties !== "object" || Array.isArray(properties))
    return { fields: [], rawOnlyReason: "Unsupported schema shape; use raw JSON." };
  const required = new Set(
    Array.isArray(schema.required)
      ? schema.required.filter((v): v is string => typeof v === "string")
      : [],
  );
  const fields: SchemaField[] = [];
  for (const [name, value] of Object.entries(properties as Record<string, unknown>)) {
    if (!value || typeof value !== "object" || Array.isArray(value))
      return { fields: [], rawOnlyReason: `Field ${name} has an unsupported schema.` };
    const field = value as Record<string, unknown>;
    if (field.writeOnly || /secret|token|password|credential/i.test(name))
      return {
        fields: [],
        rawOnlyReason: `Secret field ${name} is not rendered as a form control; use reviewed raw JSON.`,
      };
    const type = field.type;
    if (
      !(["string", "number", "integer", "boolean", "object", "array"] as unknown[]).includes(type)
    )
      return { fields: [], rawOnlyReason: `Field ${name} has unsupported type ${String(type)}.` };
    const enumValues = Array.isArray(field.enum) ? field.enum : undefined;
    if (enumValues && enumValues.length > 100)
      return { fields: [], rawOnlyReason: `Field ${name} has too many enum values; use raw JSON.` };
    fields.push({
      name,
      type: type as SchemaField["type"],
      required: required.has(name),
      description: typeof field.description === "string" ? field.description : "",
      enumValues,
    });
  }
  return { fields, rawOnlyReason: null };
}

export function parseRawArguments(value: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(value);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    throw new Error("Arguments must be a JSON object.");
  return parsed as Record<string, unknown>;
}

export function inertResultPreview(value: unknown, maxCharacters = 64 * 1024): string {
  const serialized = JSON.stringify(value, null, 2) ?? "null";
  return serialized.length <= maxCharacters
    ? serialized
    : `${serialized.slice(0, maxCharacters)}\n… bounded preview truncated …`;
}
