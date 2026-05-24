#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const [, , inputPath, outputPath] = process.argv;

if (!inputPath || !outputPath) {
  console.error('usage: node scripts/generate-openapi-types.mjs <openapi.json> <output.ts>');
  process.exit(2);
}

const document = JSON.parse(readFileSync(inputPath, 'utf8'));
const schemas = document.components?.schemas ?? {};

function refName(ref) {
  const prefix = '#/components/schemas/';
  if (!ref.startsWith(prefix)) {
    return 'unknown';
  }
  return `components['schemas']['${ref.slice(prefix.length)}']`;
}

function typeFromTypeName(name) {
  switch (name) {
    case 'string':
      return 'string';
    case 'integer':
    case 'number':
      return 'number';
    case 'boolean':
      return 'boolean';
    case 'null':
      return 'null';
    case 'array':
      return 'unknown[]';
    case 'object':
      return 'Record<string, unknown>';
    default:
      return 'unknown';
  }
}

function union(types) {
  return [...new Set(types)].join(' | ');
}

function typeFor(schema) {
  if (!schema || Object.keys(schema).length === 0) {
    return 'unknown';
  }
  if (schema.$ref) {
    return refName(schema.$ref);
  }
  if (schema.oneOf || schema.anyOf) {
    return union((schema.oneOf ?? schema.anyOf).map(typeFor));
  }
  if (schema.enum) {
    return union(schema.enum.map((value) => JSON.stringify(value)));
  }
  if (Array.isArray(schema.type)) {
    return union(schema.type.map((name) => {
      if (name === 'array') {
        return `${typeFor(schema.items)}[]`;
      }
      if (name === 'object' && schema.properties) {
        return objectType(schema);
      }
      return typeFromTypeName(name);
    }));
  }
  if (schema.type === 'array') {
    return `${typeFor(schema.items)}[]`;
  }
  if (schema.type === 'object' || schema.properties) {
    return objectType(schema);
  }
  return typeFromTypeName(schema.type);
}

function objectType(schema) {
  const properties = schema.properties ?? {};
  const required = new Set(schema.required ?? []);
  const entries = Object.entries(properties);

  if (entries.length === 0) {
    return schema.additionalProperties === false ? 'Record<string, never>' : 'Record<string, unknown>';
  }

  const lines = ['{'];
  for (const [name, propertySchema] of entries) {
    const optional = required.has(name) ? '' : '?';
    lines.push(`            ${JSON.stringify(name)}${optional}: ${typeFor(propertySchema)};`);
  }
  lines.push('        }');
  return lines.join('\n');
}

const lines = [
  '/**',
  ' * This file was generated from apps/web/openapi/axon.json.',
  ' * Do not edit by hand; run npm run openapi:generate.',
  ' */',
  '',
  'export type components = {',
  '    schemas: {',
];

for (const [name, schema] of Object.entries(schemas)) {
  lines.push(`        ${JSON.stringify(name)}: ${typeFor(schema)};`);
}

lines.push('    };', '};', '');

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, lines.join('\n'));
