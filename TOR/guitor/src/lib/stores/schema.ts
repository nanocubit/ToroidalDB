// Schema store for GuiTor - Database exploration
import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface Table {
  name: string;
  schema: string;
  columns: Column[];
  primary_key: string[];
  row_count?: number;
}

export interface Column {
  name: string;
  data_type: string;
  is_nullable: boolean;
  is_primary_key: boolean;
  default_value?: string;
  ordinal_position: number;
}

export interface View {
  name: string;
  schema: string;
  definition: string;
}

export interface Function {
  name: string;
  schema: string;
  arguments: Column[];
  return_type: string;
}

export interface Schema {
  tables: Table[];
  views: View[];
  functions: Function[];
}

export const schemas = writable<Map<string, Schema>>(new Map());
export const loadingSchema = writable<boolean>(false);
export const schemaError = writable<string | null>(null);
export const expandedSchemas = writable<Set<string>>(new Set<string>());
export const expandedTables = writable<Set<string>>(new Set<string>());

export const activeSchema = derived(
  [schemas],
  ([$schemas]) => {
    // Return first schema as default
    const entries = Array.from($schemas.entries());
    return entries.length > 0 ? entries[0][1] : null;
  }
);

export async function loadSchemas(connId: string): Promise<Schema | null> {
  loadingSchema.set(true);
  schemaError.set(null);

  try {
    const schemas_list = await invoke<string[]>('list_schemas', { connId });
    
    const schema: Schema = {
      tables: [],
      views: [],
      functions: [],
    };

    for (const schemaName of schemas_list) {
      const tables = await invoke<Table[]>('list_tables', { connId, schema: schemaName });
      schema.tables.push(...tables);
    }

    schemas.update((map) => {
      map.set(connId, schema);
      return map;
    });

    return schema;
  } catch (error) {
    schemaError.set(String(error));
    return null;
  } finally {
    loadingSchema.set(false);
  }
}

export async function loadTables(connId: string, schema: string): Promise<Table[]> {
  try {
    const tables = await invoke<Table[]>('list_tables', { connId, schema });
    
    schemas.update((map) => {
      const existing = map.get(connId) || { tables: [], views: [], functions: [] };
      existing.tables = tables;
      map.set(connId, existing);
      return map;
    });

    return tables;
  } catch (error) {
    schemaError.set(String(error));
    return [];
  }
}

export function toggleSchemaExpansion(schema: string): void {
  expandedSchemas.update((set) => {
    if (set.has(schema)) {
      set.delete(schema);
    } else {
      set.add(schema);
    }
    return new Set(set);
  });
}

export function toggleTableExpansion(table: string): void {
  expandedTables.update((set) => {
    if (set.has(table)) {
      set.delete(table);
    } else {
      set.add(table);
    }
    return new Set(set);
  });
}

export function getSchema(connId: string): Schema | undefined {
  let result: Schema | undefined;
  schemas.subscribe((map) => {
    result = map.get(connId);
  })();
  return result;
}
