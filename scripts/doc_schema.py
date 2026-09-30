"""Strict readers for generated documentation contracts, not Rust-text inference."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


class ContractError(ValueError):
    """A malformed or stale input with a source-level remedy."""


def cell(value: Any) -> str:
    """Keep schema-controlled strings inside one Markdown table cell."""
    return str(value).replace("&", "&amp;").replace("|", "&#124;").replace(chr(96), "&#96;").replace("[", "&#91;").replace("]", "&#93;").replace("<", "&lt;").replace(">", "&gt;").replace("\r", "").replace("\n", "<br>")


def load_contract(root: Path, relative: str, producer: str) -> dict[str, Any]:
    try:
        value = json.loads((root / relative).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ContractError(f"{relative}: cannot read JSON contract ({error}); run cargo xtask generated-contracts refresh") from error
    if not isinstance(value, dict):
        raise ContractError(f"{relative}: expected a JSON object")
    meta = value.get("x-axon", {})
    if not isinstance(meta, dict) or meta.get("generated_by") != producer:
        raise ContractError(f"{relative}: expected producer {producer!r}; regenerate the owning schema")
    inputs = meta.get("source_inputs")
    if not isinstance(inputs, list) or not inputs:
        raise ContractError(f"{relative}: missing source-input provenance; regenerate the owning schema")
    seen: dict[str, str] = {}
    for item in inputs:
        if not isinstance(item, dict) or not all(isinstance(item.get(k), str) for k in ("path", "checksum")):
            raise ContractError(f"{relative}: malformed source-input provenance")
        source = (root / item["path"]).resolve()
        if Path(item["path"]).is_absolute() or not source.is_relative_to(root.resolve()):
            raise ContractError(f"{relative}: source-input path must stay inside the repository")
        if seen.setdefault(item["path"], item["checksum"]) != item["checksum"]:
            raise ContractError(f"{relative}: conflicting provenance for {item['path']}")
        try:
            digest = "sha256:" + hashlib.sha256(source.read_bytes()).hexdigest()
        except OSError as error:
            raise ContractError(f"{relative}: missing source input {item['path']}; restore the source and regenerate") from error
        if digest != item["checksum"]:
            raise ContractError(f"{relative}: stale source input {item['path']}; rebuild xtask and run cargo xtask generated-contracts refresh")
    return value


def resolve_schema(document: dict[str, Any], schema: Any) -> dict[str, Any] | bool:
    if isinstance(schema, bool):
        return schema
    if not isinstance(schema, dict):
        raise ContractError("schema must be an object or boolean")
    seen: set[str] = set()
    while "$ref" in schema:
        ref = schema["$ref"]
        if not isinstance(ref, str) or not ref.startswith("#/") or ref in seen:
            raise ContractError(f"unsupported or cyclic schema reference: {ref!r}")
        seen.add(ref)
        target: Any = document
        for part in ref[2:].split("/"):
            key = part.replace("~1", "/").replace("~0", "~")
            if not isinstance(target, dict) or key not in target:
                raise ContractError(f"missing schema reference {ref}; regenerate the owning schema")
            target = target[key]
        if not isinstance(target, dict):
            raise ContractError(f"reference {ref} must identify an object schema")
        schema = {**target, **{k: v for k, v in schema.items() if k != "$ref"}}
    return schema


def operations(document: dict[str, Any]) -> list[dict[str, Any]]:
    meta = document.get("x-axon", {})
    items = meta.get("operations")
    if not isinstance(items, list) or not items:
        raise ContractError("MCP contract has no operation catalog; regenerate cargo xtask schemas mcp")
    names: set[str] = set()
    for item in items:
        if not isinstance(item, dict) or not all(isinstance(item.get(k), str) and item[k] for k in ("name", "action", "description", "task_support")):
            raise ContractError("MCP operation is missing its name, action, description, scope, or task support")
        if "required_scope" not in item or (item["required_scope"] is not None and not isinstance(item["required_scope"], str)):
            raise ContractError(f"{item['name']}: invalid scope metadata; null means no extra operation scope, not disabled transport auth")
        if item["name"] in names:
            raise ContractError(f"duplicate MCP operation: {item['name']}")
        names.add(item["name"])
        schema = resolve_schema(document, item.get("inputSchema"))
        if not isinstance(schema, dict) or schema.get("type") != "object":
            raise ContractError(f"{item['name']}: expected an object input schema")
        if item.get("subaction") is not None and not isinstance(item["subaction"], str):
            raise ContractError(f"{item['name']}: invalid subaction")
    if meta.get("operation_count") != len(items) or meta.get("live_action_count") != len({item["action"] for item in items}):
        raise ContractError("MCP operation/action counts disagree with the canonical catalog")
    return sorted(items, key=lambda item: item["name"])


def fields_table(document: dict[str, Any], raw: Any) -> list[str]:
    schema = resolve_schema(document, raw)
    if not isinstance(schema, dict):
        raise ContractError("field table requires an object schema")
    properties = schema.get("properties", {})
    required = schema.get("required", [])
    if not isinstance(properties, dict) or not isinstance(required, list) or any(k not in properties for k in required):
        raise ContractError("invalid properties/required field contract")
    lines = ["| Field | Required by schema | Shape / constraints | Description |", "|---|---|---|---|"]
    for name, raw_field in sorted(properties.items()):
        field = resolve_schema(document, raw_field)
        if isinstance(field, bool):
            shape, description = ("any JSON" if field else "forbidden"), ""
        else:
            shape = json.dumps({k: v for k, v in field.items() if k not in ("description", "title", "$schema", "$defs")}, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
            description = field.get("description", "")
        lines.append(f"| {cell(name)} | {'yes' if name in required else 'no'} | {cell(shape)} | {cell(description)} |")
    if not properties:
        lines.append("| (no fields) | n/a | Empty argument object | |")
    return lines
