// Pure schema traversal utilities for MCP tool input schemas (JSON Schema draft-07).
// Supports the common subset: string, boolean, integer, number, object, array, enum.
// Schemas with unsupported constructs ($ref, oneOf, anyOf, allOf) are marked unknown.
// Max recursion depth: 5 (R-L mitigation — beyond this, caller should use raw fallback).

export const MAX_SCHEMA_DEPTH = 5;

export type FieldType =
  | 'string'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'object'
  | 'array'
  | 'enum'
  | 'unknown';

export interface FieldDescriptor {
  key: string;
  type: FieldType;
  description: string;
  required: boolean;
  default: unknown;
  /** For type === 'enum'. */
  enumValues: string[];
  /** For type === 'object': child field descriptors. */
  properties: FieldDescriptor[];
  /** For type === 'array': descriptor of a single item. */
  itemType: FieldType;
}

type Schema = Record<string, unknown>;

function isSchema(v: unknown): v is Schema {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

/** Classify a single schema node into a FieldType. */
export function classifyType(schema: unknown): FieldType {
  if (!isSchema(schema)) return 'unknown';
  // unsupported compositions
  if ('$ref' in schema || 'oneOf' in schema || 'anyOf' in schema || 'allOf' in schema) {
    return 'unknown';
  }
  const t = schema['type'];
  if (Array.isArray(schema['enum'])) return 'enum';
  if (t === 'string') return 'string';
  if (t === 'integer') return 'integer';
  if (t === 'number') return 'number';
  if (t === 'boolean') return 'boolean';
  if (t === 'object') return 'object';
  if (t === 'array') return 'array';
  return 'unknown';
}

/**
 * Convert a JSON Schema `properties` object into a flat list of FieldDescriptors.
 * Returns an empty list for null/undefined input or when schema has no properties.
 */
export function schemaToFields(
  schema: unknown,
  depth = 0,
): FieldDescriptor[] {
  if (!isSchema(schema) || depth > MAX_SCHEMA_DEPTH) return [];

  const required = new Set(
    Array.isArray(schema['required']) ? (schema['required'] as string[]) : [],
  );
  const props = schema['properties'];
  if (!isSchema(props)) return [];

  return Object.entries(props).map(([key, propSchema]): FieldDescriptor => {
    const type = classifyType(propSchema);
    const ps = isSchema(propSchema) ? propSchema : {};
    const enumValues =
      Array.isArray(ps['enum'])
        ? (ps['enum'] as unknown[]).map(String)
        : [];

    let properties: FieldDescriptor[] = [];
    let itemType: FieldType = 'unknown';

    if (type === 'object' && depth + 1 <= MAX_SCHEMA_DEPTH) {
      properties = schemaToFields(propSchema, depth + 1);
    }
    if (type === 'array' && isSchema(ps['items'])) {
      itemType = classifyType(ps['items']);
    }

    return {
      key,
      type,
      description: typeof ps['description'] === 'string' ? ps['description'] : '',
      required: required.has(key),
      default: ps['default'] ?? undefined,
      enumValues,
      properties,
      itemType,
    };
  });
}

/**
 * Assemble a flat `Record<string, unknown>` from a values map keyed by field key.
 * Omits keys whose value is `''` and that are not required (treats empty string as absent).
 */
export function assembleArgs(
  fields: FieldDescriptor[],
  values: Map<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of fields) {
    const v = values.get(field.key);
    if (v === '' && !field.required) continue;
    if (v !== undefined) out[field.key] = v;
  }
  return out;
}

/**
 * Return true when the schema contains any unsupported constructs
 * that require the raw-JSON fallback to be shown.
 */
export function needsRawFallback(schema: unknown): boolean {
  if (!isSchema(schema)) return true;
  const t = schema['type'];
  if ('$ref' in schema || 'oneOf' in schema || 'anyOf' in schema || 'allOf' in schema) {
    return true;
  }
  // If there are no properties and it's an object, the schema is empty — raw fallback.
  if (t === 'object' && !isSchema(schema['properties'])) return true;
  return false;
}
