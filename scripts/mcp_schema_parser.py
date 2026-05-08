"""Rust schema parser for MCP schema documentation generation.

Parses struct and enum definitions from Rust source code and validates
that expected action structs and enums are present.
"""

from __future__ import annotations

import re

from mcp_schema_models import EnumDef, FieldDef, StructDef, STRUCT_TO_ACTION


def parse_schema(source: str) -> tuple[dict[str, StructDef], dict[str, EnumDef]]:
    """Parse struct and enum definitions from Rust source."""
    structs: dict[str, StructDef] = {}
    enums: dict[str, EnumDef] = {}

    # Parse structs
    struct_pattern = re.compile(r"pub\s+struct\s+(\w+)\s*\{([^}]*)\}", re.DOTALL)
    for m in struct_pattern.finditer(source):
        name = m.group(1)
        body = m.group(2)
        fields: list[FieldDef] = []
        pending_aliases: list[str] = []
        for line in body.splitlines():
            stripped = line.strip()
            alias_match = re.search(r'#\[serde\([^]]*alias\s*=\s*"([^"]+)"', stripped)
            if alias_match:
                pending_aliases.append(alias_match.group(1))
                continue
            field_match = re.match(r"pub\s+(\w+)\s*:\s*([^,\n]+)", stripped)
            if not field_match:
                continue
            fname = field_match.group(1)
            ftype = field_match.group(2).strip().rstrip(",").strip()
            fields.append(
                FieldDef(name=fname, rust_type=ftype, aliases=pending_aliases)
            )
            pending_aliases = []
        structs[name] = StructDef(name=name, fields=fields)

    # Parse enums
    enum_pattern = re.compile(r"pub\s+enum\s+(\w+)\s*\{([^}]*)\}", re.DOTALL)

    for m in enum_pattern.finditer(source):
        name = m.group(1)
        body = m.group(2)
        variants: list[str] = []
        for line in body.splitlines():
            line = line.strip()
            if not line or line.startswith("//") or line.startswith("#"):
                continue
            vm = re.match(r"(\w+)", line)
            if vm:
                v = vm.group(1)
                # Skip serde attribute keywords that leak through
                if v in {"pub", "fn", "let", "use", "mod", "impl", "type"}:
                    continue
                variants.append(v)
        enums[name] = EnumDef(name=name, variants=variants)

    return structs, enums


def validate_parsed(
    structs: dict[str, StructDef],
    enums: dict[str, EnumDef],
) -> list[str]:
    """Return a list of validation errors (empty = OK)."""
    errors: list[str] = []

    # Every action struct must exist
    for struct_name, action in STRUCT_TO_ACTION.items():
        if struct_name not in structs:
            errors.append(f"Missing struct {struct_name} for action '{action}'")

    # Every subaction enum referenced by a *request* struct must exist
    for struct_name in STRUCT_TO_ACTION:
        sdef = structs.get(struct_name)
        if not sdef:
            continue
        enum_name = sdef.subaction_enum_name
        if enum_name and enum_name not in enums:
            errors.append(
                f"Struct {struct_name} references enum {enum_name} but it was not found"
            )

    # AxonRequest enum must exist
    if "AxonRequest" not in enums:
        errors.append("Missing AxonRequest enum")

    return errors
