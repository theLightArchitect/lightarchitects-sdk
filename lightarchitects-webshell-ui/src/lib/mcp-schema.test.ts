import { describe, it, expect } from 'vitest';
import {
  classifyType,
  schemaToFields,
  assembleArgs,
  needsRawFallback,
  MAX_SCHEMA_DEPTH,
} from './mcp-schema';

describe('classifyType', () => {
  it('classifies string', () => expect(classifyType({ type: 'string' })).toBe('string'));
  it('classifies integer', () => expect(classifyType({ type: 'integer' })).toBe('integer'));
  it('classifies number', () => expect(classifyType({ type: 'number' })).toBe('number'));
  it('classifies boolean', () => expect(classifyType({ type: 'boolean' })).toBe('boolean'));
  it('classifies object', () => expect(classifyType({ type: 'object' })).toBe('object'));
  it('classifies array', () => expect(classifyType({ type: 'array' })).toBe('array'));
  it('classifies enum (overrides type)', () =>
    expect(classifyType({ type: 'string', enum: ['a', 'b'] })).toBe('enum'));
  it('classifies $ref as unknown', () =>
    expect(classifyType({ $ref: '#/defs/foo' })).toBe('unknown'));
  it('classifies oneOf as unknown', () =>
    expect(classifyType({ oneOf: [] })).toBe('unknown'));
  it('classifies null input as unknown', () =>
    expect(classifyType(null)).toBe('unknown'));
});

describe('schemaToFields', () => {
  it('returns empty for schema with no properties', () => {
    expect(schemaToFields({ type: 'object' })).toEqual([]);
  });

  it('produces a string field', () => {
    const fields = schemaToFields({
      type: 'object',
      required: ['name'],
      properties: { name: { type: 'string', description: 'Your name' } },
    });
    expect(fields).toHaveLength(1);
    expect(fields[0]).toMatchObject({
      key: 'name',
      type: 'string',
      required: true,
      description: 'Your name',
    });
  });

  it('marks non-required field as optional', () => {
    const fields = schemaToFields({
      type: 'object',
      properties: { opt: { type: 'string' } },
    });
    expect(fields[0].required).toBe(false);
  });

  it('extracts enum values', () => {
    const fields = schemaToFields({
      type: 'object',
      properties: { color: { type: 'string', enum: ['red', 'green', 'blue'] } },
    });
    expect(fields[0].type).toBe('enum');
    expect(fields[0].enumValues).toEqual(['red', 'green', 'blue']);
  });

  it('extracts boolean field with default', () => {
    const fields = schemaToFields({
      type: 'object',
      properties: { enabled: { type: 'boolean', default: true } },
    });
    expect(fields[0].type).toBe('boolean');
    expect(fields[0].default).toBe(true);
  });

  it('recursively extracts object properties', () => {
    const fields = schemaToFields({
      type: 'object',
      properties: {
        nested: {
          type: 'object',
          properties: { inner: { type: 'integer' } },
        },
      },
    });
    expect(fields[0].type).toBe('object');
    expect(fields[0].properties).toHaveLength(1);
    expect(fields[0].properties[0].key).toBe('inner');
    expect(fields[0].properties[0].type).toBe('integer');
  });

  it('stops recursion at MAX_SCHEMA_DEPTH', () => {
    // Build a deeply nested schema beyond the limit
    const deep = schemaToFields({ type: 'object', properties: { a: { type: 'string' } } }, MAX_SCHEMA_DEPTH + 1);
    expect(deep).toEqual([]);
  });

  it('extracts array itemType', () => {
    const fields = schemaToFields({
      type: 'object',
      properties: { tags: { type: 'array', items: { type: 'string' } } },
    });
    expect(fields[0].type).toBe('array');
    expect(fields[0].itemType).toBe('string');
  });

  it('returns empty for null schema', () => {
    expect(schemaToFields(null)).toEqual([]);
  });
});

describe('assembleArgs', () => {
  const fields = schemaToFields({
    type: 'object',
    required: ['req'],
    properties: {
      req: { type: 'string' },
      opt: { type: 'string' },
    },
  });

  it('includes required fields even if empty', () => {
    const values = new Map<string, unknown>([['req', ''], ['opt', '']]);
    const args = assembleArgs(fields, values);
    expect(args).toHaveProperty('req', '');
    expect(args).not.toHaveProperty('opt');
  });

  it('omits optional empty strings', () => {
    const values = new Map<string, unknown>([['req', 'hello'], ['opt', '']]);
    expect(assembleArgs(fields, values)).toEqual({ req: 'hello' });
  });

  it('includes optional non-empty values', () => {
    const values = new Map<string, unknown>([['req', 'a'], ['opt', 'b']]);
    expect(assembleArgs(fields, values)).toEqual({ req: 'a', opt: 'b' });
  });
});

describe('needsRawFallback', () => {
  it('returns false for a normal object schema', () => {
    expect(needsRawFallback({ type: 'object', properties: { x: { type: 'string' } } })).toBe(false);
  });
  it('returns true for $ref', () =>
    expect(needsRawFallback({ $ref: '#/defs/foo' })).toBe(true));
  it('returns true for oneOf', () =>
    expect(needsRawFallback({ oneOf: [] })).toBe(true));
  it('returns true for object with no properties', () =>
    expect(needsRawFallback({ type: 'object' })).toBe(true));
  it('returns true for null', () =>
    expect(needsRawFallback(null)).toBe(true));
});
