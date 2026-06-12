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
const pathsDocument = document.paths ?? {};

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

function operationName(method, path) {
  const slug = path
    .replace(/^\//, '')
    .replace(/\{(\*?)([^}]+)\}/g, '$2')
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
  return `${method.toLowerCase()}_${slug || 'root'}`;
}

function mediaTypeSchema(content) {
  const entries = Object.entries(content ?? {});
  if (entries.length === 0) {
    return 'unknown';
  }
  const json = content['application/json'];
  if (json?.schema) {
    return typeFor(json.schema);
  }
  const [, media] = entries[0];
  return typeFor(media?.schema);
}

function requestBodyType(operation) {
  const content = operation.requestBody?.content;
  if (!content) {
    return 'never';
  }
  return mediaTypeSchema(content);
}

function parametersType(operation, location) {
  const params = (operation.parameters ?? []).filter((parameter) => parameter.in === location);
  if (params.length === 0) {
    return 'Record<string, never>';
  }
  const required = new Set(params.filter((parameter) => parameter.required).map((parameter) => parameter.name));
  const fields = [];
  for (const parameter of params) {
    const optional = required.has(parameter.name) ? '' : '?';
    fields.push(`${JSON.stringify(parameter.name)}${optional}: ${typeFor(parameter.schema)}`);
  }
  return `{ ${fields.join('; ')} }`;
}

function responsesType(operation) {
  const entries = Object.entries(operation.responses ?? {});
  if (entries.length === 0) {
    return 'Record<string, never>';
  }
  const fields = [];
  for (const [status, response] of entries) {
    const concrete = response?.$ref ? refName(response.$ref) : mediaTypeSchema(response?.content);
    fields.push(`${JSON.stringify(status)}: ${concrete}`);
  }
  return `{ ${fields.join('; ')} }`;
}

function securityType(operation) {
  const names = new Set();
  for (const requirement of operation.security ?? []) {
    for (const name of Object.keys(requirement)) {
      names.add(JSON.stringify(name));
    }
  }
  return names.size === 0 ? 'never' : [...names].join(' | ');
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

const operationExports = [];
const pathOperations = new Map();
for (const [path, pathItem] of Object.entries(pathsDocument)) {
  const methods = [];
  for (const method of ['get', 'post', 'put', 'patch', 'delete']) {
    const operation = pathItem?.[method];
    if (!operation) {
      continue;
    }
    const opName = operation.operationId ?? operationName(method, path);
    operationExports.push([opName, path, method]);
    methods.push(`${method}: operations[${JSON.stringify(opName)}]`);
  }
  pathOperations.set(path, methods);
}

lines.push('export type paths = {');
for (const [path, methods] of pathOperations) {
  lines.push(`    ${JSON.stringify(path)}: { ${methods.join('; ')} };`);
}
lines.push('};', '');

lines.push('export type operations = {');
for (const [opName, path, method] of operationExports) {
  const operation = pathsDocument[path][method];
  lines.push(
    `    ${JSON.stringify(opName)}: { method: ${JSON.stringify(method)}; path: ${JSON.stringify(path)}; operationId: ${JSON.stringify(opName)}; parameters: { query: ${parametersType(operation, 'query')}; path: ${parametersType(operation, 'path')} }; requestBody: ${requestBodyType(operation)}; responses: ${responsesType(operation)}; security: ${securityType(operation)} };`,
  );
}
lines.push('};', '');

mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, lines.join('\n'));
