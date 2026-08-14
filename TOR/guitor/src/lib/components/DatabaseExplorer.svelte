<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { activeConnection, activeConnectionId } from '../stores/connections';
  import { currentSchema } from '../stores/schema';

  interface DbObject {
    name: string;
    kind: string;
    schema: string;
    columns?: ColumnSchema[];
    children?: DbObject[];
  }

  interface ColumnSchema {
    name: string;
    data_type: string;
    is_nullable: boolean;
    is_primary_key: boolean;
  }

  let schemas: DbObject[] = [];
  let expandedSchemas = new Set<string>();
  let expandedTables = new Set<string>();
  let isLoading = false;
  let error: string | null = null;

  $: if ($activeConnectionId) {
    loadSchemas();
  }

  async function loadSchemas() {
    if (!$activeConnectionId) return;

    isLoading = true;
    error = null;

    try {
      schemas = await invoke('list_schemas', { conn_id: $activeConnectionId }) as DbObject[];
    } catch (e) {
      error = `Failed to load schemas: ${e}`;
      console.error(e);
    } finally {
      isLoading = false;
    }
  }

  async function toggleSchema(schemaName: string) {
    if (expandedSchemas.has(schemaName)) {
      expandedSchemas.delete(schemaName);
    } else {
      expandedSchemas.add(schemaName);
      await loadTables(schemaName);
    }
  }

  async function loadTables(schemaName: string) {
    try {
      const [tables, views, functions] = await Promise.all([
        invoke('list_tables', { conn_id: $activeConnectionId, schema: schemaName }) as Promise<DbObject[]>,
        invoke('list_views', { conn_id: $activeConnectionId, schema: schemaName }) as Promise<DbObject[]>,
        invoke('list_functions', { conn_id: $activeConnectionId, schema: schemaName }) as Promise<DbObject[]>,
      ]);

      const schema = schemas.find(s => s.name === schemaName);
      if (schema) {
        schema.children = [
          { name: 'Tables', kind: 'folder', schema: schemaName, children: tables },
          { name: 'Views', kind: 'folder', schema: schemaName, children: views },
          { name: 'Functions', kind: 'folder', schema: schemaName, children: functions },
        ];
      }
    } catch (e) {
      console.error('Failed to load tables:', e);
    }
  }

  async function toggleTable(schemaName: string, tableName: string) {
    const key = `${schemaName}.${tableName}`;
    if (expandedTables.has(key)) {
      expandedTables.delete(key);
    } else {
      expandedTables.add(key);
      // Columns already loaded in loadTables
    }
  }

  function selectTable(table: DbObject) {
    currentSchema.set({
      schema: table.schema,
      table: table.name,
      columns: table.columns || [],
    });
  }

  function getIcon(kind: string): string {
    switch (kind) {
      case 'schema': return '📁';
      case 'table': return '📊';
      case 'view': return '👁️';
      case 'function': return '⚙️';
      case 'folder': return '📂';
      case 'column': return '🔖';
      case 'pk': return '🔑';
      default: return '📄';
    }
  }

  function getTypeColor(kind: string): string {
    switch (kind) {
      case 'schema': return '#60a5fa';
      case 'table': return '#4ade80';
      case 'view': return '#f472b6';
      case 'function': return '#fbbf24';
      default: return '#888';
    }
  }
</script>

<div class="database-explorer">
  <div class="explorer-header">
    <h3>Database Explorer</h3>
    <button class="refresh-btn" onclick={loadSchemas} disabled={isLoading}>
      {#if isLoading}
        ⟳
      {:else}
        ↻
      {/if}
    </button>
  </div>

  {#if !$activeConnection}
    <div class="no-connection">
      <p>No active connection</p>
      <p class="hint">Add a connection to browse schemas</p>
    </div>
  {:else if error}
    <div class="error-message">
      <p>⚠️ {error}</p>
      <button onclick={loadSchemas}>Retry</button>
    </div>
  {:else if schemas.length === 0}
    <div class="empty-state">
      <p>No schemas found</p>
    </div>
  {:else}
    <div class="schema-list">
      {#each schemas as schema (schema.name)}
        <div class="schema-item">
          <div 
            class="schema-header"
            onclick={() => toggleSchema(schema.name)}
          >
            <span class="expand-icon">
              {#if expandedSchemas.has(schema.name)}▼{:else}▶}
            </span>
            <span class="schema-icon">{getIcon('schema')}</span>
            <span class="schema-name" title={schema.name}>{schema.name}</span>
          </div>

          {#if expandedSchemas.has(schema.name) && schema.children}
            <div class="schema-children">
              {#each schema.children as folder (folder.name)}
                {#if folder.children && folder.children.length > 0}
                  <div class="folder-section">
                    <div class="folder-header">
                      <span class="folder-icon">{getIcon('folder')}</span>
                      <span class="folder-name">{folder.name} ({folder.children.length})</span>
                    </div>

                    <div class="folder-items">
                      {#each folder.children as item (item.name)}
                        <div 
                          class="item-row"
                          class:expanded={expandedTables.has(`${schema.name}.${item.name}`)}
                          onclick={() => {
                            if (item.kind === 'table') {
                              toggleTable(schema.name, item.name);
                              selectTable(item);
                            }
                          }}
                        >
                          <span class="item-icon">{getIcon(item.kind)}</span>
                          <span 
                            class="item-name" 
                            style="color: {getTypeColor(item.kind)}"
                            title={item.name}
                          >
                            {item.name}
                          </span>
                          
                          {#if item.kind === 'column'}
                            <span class="column-type">{item.data_type}</span>
                            {#if item.is_primary_key}
                              <span class="pk-badge" title="Primary Key">🔑</span>
                            {/if}
                          {/if}
                        </div>

                        {#if item.kind === 'table' && expandedTables.has(`${schema.name}.${item.name}`) && item.columns}
                          <div class="columns-list">
                            {#each item.columns as column (column.name)}
                              <div class="column-row">
                                <span class="column-icon">{column.is_primary_key ? getIcon('pk') : getIcon('column')}</span>
                                <span class="column-name">{column.name}</span>
                                <span class="column-type">{column.data_type}</span>
                              </div>
                            {/each}
                          </div>
                        {/if}
                      {/each}
                    </div>
                  </div>
                {/if}
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .database-explorer {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #1a1a2e;
    overflow: hidden;
  }

  .explorer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid #2d2d44;
  }

  .explorer-header h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: #e0e0e0;
  }

  .refresh-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 18px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
    transition: all 0.2s;
  }

  .refresh-btn:hover:not(:disabled) {
    color: #e0e0e0;
    background: #252540;
  }

  .refresh-btn:disabled {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .no-connection,
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 20px;
    text-align: center;
    color: #666;
  }

  .hint {
    font-size: 12px;
    margin-top: 8px;
  }

  .error-message {
    padding: 16px;
    background: #2d1a1a;
    border-left: 3px solid #ef4444;
    margin: 12px;
    border-radius: 4px;
  }

  .error-message p {
    margin: 0 0 12px 0;
    color: #f87171;
    font-size: 13px;
  }

  .error-message button {
    padding: 6px 12px;
    background: #ef4444;
    border: none;
    border-radius: 4px;
    color: white;
    cursor: pointer;
    font-size: 12px;
  }

  .schema-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0;
  }

  .schema-item {
    user-select: none;
  }

  .schema-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    cursor: pointer;
    transition: background 0.2s;
  }

  .schema-header:hover {
    background: #252540;
  }

  .expand-icon {
    font-size: 10px;
    color: #666;
    width: 12px;
  }

  .schema-icon {
    font-size: 14px;
  }

  .schema-name {
    font-size: 13px;
    color: #e0e0e0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .schema-children {
    padding-left: 12px;
  }

  .folder-section {
    padding: 4px 0;
  }

  .folder-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 16px;
    font-size: 12px;
    color: #888;
    font-weight: 500;
  }

  .folder-icon {
    font-size: 12px;
  }

  .folder-items {
    padding-left: 16px;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 16px;
    cursor: pointer;
    transition: background 0.2s;
    font-size: 12px;
  }

  .item-row:hover {
    background: #252540;
  }

  .item-row.expanded {
    background: #252540;
  }

  .item-icon {
    font-size: 12px;
  }

  .item-name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .column-type {
    font-size: 11px;
    color: #666;
    margin-left: auto;
  }

  .pk-badge {
    font-size: 10px;
    margin-left: 4px;
  }

  .columns-list {
    padding-left: 28px;
    background: #151525;
  }

  .column-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 16px;
    font-size: 11px;
  }

  .column-icon {
    font-size: 10px;
  }

  .column-name {
    flex: 1;
    color: #aaa;
  }

  .column-type {
    color: #666;
    font-size: 10px;
  }
</style>
