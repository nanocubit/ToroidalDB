<script lang="ts">
  import { onMount, onDestroy, createEventDispatcher } from 'svelte';
  import * as monaco from 'monaco-editor';
  import { activeConnection } from '../stores/connections';
  import { queryHistory } from '../stores/query';

  let editorContainer: HTMLDivElement;
  let editor: monaco.editor.IStandaloneCodeEditor;
  let isExecuting = false;
  let executionTime = 0;

  const dispatch = createEventDispatch<{
    execute: { sql: string };
    save: { sql: string };
  }>();

  let autocompleteItems: monaco.languages.CompletionItem[] = [];

  onMount(async () => {
    // Initialize Monaco Editor
    editor = monaco.editor.create(editorContainer, {
      value: '-- Write your SQL query here\nSELECT * FROM users LIMIT 100;',
      language: 'sql',
      theme: 'vs-dark',
      automaticLayout: true,
      minimap: { enabled: true },
      fontSize: 14,
      wordWrap: 'on',
      scrollBeyondLastLine: false,
      padding: { top: 10, bottom: 10 },
      suggest: {
        showKeywords: true,
        showFields: true,
        showTables: true,
      },
    });

    // Register SQL completion provider
    registerSqlCompletion();

    // Add keyboard shortcuts
    editor.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      () => {
        executeQuery();
      }
    );

    // Add command for formatting
    editor.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyF,
      () => {
        formatSql();
      }
    );
  });

  onDestroy(() => {
    editor?.dispose();
  });

  function registerSqlCompletion() {
    monaco.languages.registerCompletionItemProvider('sql', {
      triggerCharacters: [' ', '.', ',', '(', '['],
      provideCompletionItems: async (model, position) => {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        // SQL Keywords
        const keywords = [
          'SELECT', 'FROM', 'WHERE', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'OUTER',
          'ON', 'GROUP BY', 'ORDER BY', 'HAVING', 'LIMIT', 'OFFSET',
          'INSERT INTO', 'UPDATE', 'DELETE', 'CREATE', 'ALTER', 'DROP',
          'TABLE', 'VIEW', 'INDEX', 'FUNCTION', 'PROCEDURE',
          'AND', 'OR', 'NOT', 'IN', 'BETWEEN', 'LIKE', 'IS NULL', 'IS NOT NULL',
          'ASC', 'DESC', 'DISTINCT', 'COUNT', 'SUM', 'AVG', 'MIN', 'MAX',
          'CASE', 'WHEN', 'THEN', 'ELSE', 'END', 'AS', 'CAST', 'CONVERT',
          'UNION', 'INTERSECT', 'EXCEPT', 'ALL', 'ANY', 'SOME', 'EXISTS',
          'PRIMARY KEY', 'FOREIGN KEY', 'REFERENCES', 'UNIQUE', 'CHECK',
          'DEFAULT', 'AUTO_INCREMENT', 'NOT NULL', 'NULL',
        ];

        const suggestions: monaco.languages.CompletionItem[] = keywords.map(kw => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          detail: 'SQL Keyword',
          insertText: kw,
          range,
          sortText: 'A' + kw,
        }));

        // Add table/column suggestions from active connection
        if ($activeConnection) {
          try {
            // Fetch schemas and tables
            const schemas = await window.__TAURI__.invoke('list_schemas', {
              connId: $activeConnection.id,
            }) as any[];

            for (const schema of schemas) {
              suggestions.push({
                label: schema.name,
                kind: monaco.languages.CompletionItemKind.Module,
                detail: 'Schema',
                insertText: schema.name,
                range,
                sortText: 'B' + schema.name,
              });
            }

            // Fetch tables for first schema
            if (schemas.length > 0) {
              const tables = await window.__TAURI__.invoke('list_tables', {
                connId: $activeConnection.id,
                schema: schemas[0].name,
              }) as any[];

              for (const table of tables) {
                suggestions.push({
                  label: table.name,
                  kind: monaco.languages.CompletionItemKind.Class,
                  detail: 'Table',
                  insertText: table.name,
                  range,
                  sortText: 'C' + table.name,
                });

                // Add columns
                for (const column of table.columns || []) {
                  suggestions.push({
                    label: column.name,
                    kind: monaco.languages.CompletionItemKind.Field,
                    detail: `${column.data_type}${column.is_primary_key ? ' (PK)' : ''}`,
                    insertText: column.name,
                    range,
                    sortText: 'D' + column.name,
                  });
                }
              }
            }
          } catch (e) {
            console.error('Failed to fetch autocomplete:', e);
          }
        }

        return { suggestions };
      },
    });
  }

  async function executeQuery() {
    const sql = editor.getValue();
    if (!sql.trim() || !$activeConnection) return;

    isExecuting = true;
    const startTime = Date.now();

    try {
      const result = await window.__TAURI__.invoke('execute_query', {
        connId: $activeConnection.id,
        sql,
      }) as QueryResult;

      executionTime = result.duration_ms;

      // Add to history
      queryHistory.add({
        query: sql,
        success: true,
        duration: result.duration_ms,
        rowsAffected: result.rows_affected,
        executedAt: new Date(),
      });

      dispatch('execute', { sql });
    } catch (e) {
      executionTime = 0;
      
      queryHistory.add({
        query: sql,
        success: false,
        duration: 0,
        error: String(e),
        executedAt: new Date(),
      });

      console.error('Query execution failed:', e);
    } finally {
      isExecuting = false;
    }
  }

  function formatSql() {
    // Basic SQL formatting (can be enhanced with sql-formatter library)
    const sql = editor.getValue();
    const formatted = sql
      .replace(/\b(SELECT|FROM|WHERE|JOIN|LEFT|RIGHT|INNER|OUTER|ON|GROUP BY|ORDER BY|HAVING|LIMIT|INSERT INTO|UPDATE|DELETE|CREATE|ALTER|DROP)\b/gi, '\n$1')
      .replace(/\b(AND|OR)\b/gi, '\n  $1')
      .trim();
    
    editor.setValue(formatted);
  }

  function getSelectedText(): string {
    const selection = editor.getSelection();
    if (!selection) return '';
    return editor.getModel()?.getValueInRange(selection) || '';
  }

  function insertText(text: string) {
    const position = editor.getPosition();
    if (!position) return;
    
    editor.executeEdits('source', [{
      range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column),
      text,
    }]);
  }

  export function getEditor() {
    return editor;
  }

  export interface QueryResult {
    columns: string[];
    rows: any[][];
    rowsAffected: number;
    durationMs: number;
  }
</script>

<div class="query-editor">
  <div class="editor-toolbar">
    <div class="toolbar-group">
      <button 
        class="btn btn-primary" 
        onclick={executeQuery}
        disabled={isExecuting || !$activeConnection}
        title="Execute Query (Ctrl+Enter)"
      >
        {#if isExecuting}
          <span class="spinner"></span>
          Running...
        {:else}
          ▶ Execute
        {/if}
      </button>
      
      <button 
        class="btn btn-secondary" 
        onclick={formatSql}
        title="Format SQL (Ctrl+Shift+F)"
      >
        Format
      </button>
    </div>

    <div class="toolbar-info">
      {#if $activeConnection}
        <span class="connection-badge">
          Connected: {$activeConnection.name}
        </span>
      {:else}
        <span class="connection-badge error">
          No connection
        </span>
      {/if}
      
      {#if executionTime > 0}
        <span class="execution-time">
          ⏱ {executionTime.toFixed(2)}ms
        </span>
      {/if}
    </div>
  </div>

  <div class="editor-container" bind:this={editorContainer}></div>
</div>

<style>
  .query-editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #1e1e2e;
    border-radius: 8px;
    overflow: hidden;
  }

  .editor-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #252540;
    border-bottom: 1px solid #3d3d5c;
  }

  .toolbar-group {
    display: flex;
    gap: 8px;
  }

  .btn {
    padding: 8px 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    transition: all 0.2s;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: linear-gradient(45deg, #6366f1, #8b5cf6);
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    background: linear-gradient(45deg, #5558e3, #7c5fe3);
    transform: translateY(-1px);
  }

  .btn-secondary {
    background: #3d3d5c;
    color: #e0e0e0;
  }

  .btn-secondary:hover {
    background: #4d4d6c;
  }

  .toolbar-info {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    color: #888;
  }

  .connection-badge {
    padding: 4px 10px;
    background: #2d5a2d;
    border-radius: 4px;
    color: #4ade80;
  }

  .connection-badge.error {
    background: #5a2d2d;
    color: #f87171;
  }

  .execution-time {
    padding: 4px 10px;
    background: #2d3a5a;
    border-radius: 4px;
    color: #60a5fa;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255,255,255,0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .editor-container {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
</style>
