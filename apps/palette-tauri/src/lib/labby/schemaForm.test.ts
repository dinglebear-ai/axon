import { describe, expect, it } from "vitest";
import { inertResultPreview, parseRawArguments, schemaFields } from "./schemaForm";

describe("schemaFields", () => {
  it("builds controls for bounded object properties", () => {
    expect(
      schemaFields({
        type: "object",
        additionalProperties: false,
        required: ["query"],
        properties: { query: { type: "string" }, limit: { type: "integer", enum: [1, 5] } },
      }),
    ).toEqual({
      rawOnlyReason: null,
      fields: [
        { name: "query", type: "string", required: true, description: "", enumValues: undefined },
        { name: "limit", type: "integer", required: false, description: "", enumValues: [1, 5] },
      ],
    });
  });
  it.each([
    [{ oneOf: [{ type: "string" }] }, "Schema unions"],
    [{ properties: { password: { type: "string" } } }, "Secret field"],
    [{ properties: { child: { $ref: "#/$defs/child" } } }, "Recursive or referenced"],
    [{ additionalProperties: true, properties: {} }, "Open-ended"],
  ])("fails explicit schema hazards into raw mode", (schema, message) => {
    expect(schemaFields(schema).rawOnlyReason).toContain(message);
  });
});

it("keeps hostile MCP content inert and bounds raw detail", () => {
  const preview = inertResultPreview(
    { content: [{ type: "text", text: "<script>steal()</script>" }], oversized: "x".repeat(100) },
    80,
  );
  expect(preview).toContain("<script>");
  expect(preview).toContain("bounded preview truncated");
  expect(preview).not.toContain("<iframe");
});

describe("parseRawArguments", () => {
  it("accepts only JSON objects", () => {
    expect(parseRawArguments('{"query":"safe"}')).toEqual({ query: "safe" });
    expect(() => parseRawArguments("[]")).toThrow("JSON object");
  });
});
