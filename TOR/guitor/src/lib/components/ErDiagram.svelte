<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import cytoscape from 'cytoscape';

  interface DbObject {
    name: string;
    kind: string;
    schema: string;
    columns?: ColumnSchema[];
  }

  interface ColumnSchema {
    name: string;
    data_type: string;
    is_primary_key: boolean;
  }

  interface ForeignKey {
    from_table: string;
    from_column: string;
    to_table: string;
    to_column: string;
  }

  let cyContainer: HTMLDivElement;
  let cy: cytoscape.Core | null = null;
  let isLoading = false;
  let error: string | null = null;
  let selectedSchema = '';
  let schemas: string[] = [];

  onMount(async () => {
    await loadSchemas();
  });

  onDestroy(() => {
    cy?.destroy();
  });

  async function loadSchemas() {
    if (!$activeConnectionId) return;

    try {
      schemas = (await invoke('list_schemas', { conn_id: $activeConnectionId }) as DbObject[])
        .map(s => s.name);
      
      if (schemas.length > 0) {
        selectedSchema = schemas[0];
        await generateDiagram();
      }
    } catch (e) {
      error = `Failed to load schemas: ${e}`;
    }
  }

  async function generateDiagram() {
    if (!selectedSchema || !$activeConnectionId) return;

    isLoading = true;
    error = null;

    try {
      // Load tables
      const tables = await invoke('list_tables', {
        conn_id: $activeConnectionId,
        schema: selectedSchema,
      }) as DbObject[];

      // Load foreign keys (from information_schema)
      const foreignKeys = await loadForeignKeys(selectedSchema);

      // Initialize Cytoscape
      initializeCytoscape(tables, foreignKeys);
    } catch (e) {
      error = `Failed to generate diagram: ${e}`;
    } finally {
      isLoading = false;
    }
  }

  async function loadForeignKeys(schema: string): Promise<ForeignKey[]> {
    try {
      const result = await invoke('execute_query', {
        conn_id: $activeConnectionId,
        sql: `
          SELECT 
            tc.table_name AS from_table,
            kcu.column_name AS from_column,
            ccu.table_name AS to_table,
            ccu.column_name AS to_column
          FROM information_schema.table_constraints AS tc
          JOIN information_schema.key_column_usage AS kcu
            ON tc.constraint_name = kcu.constraint_name
          JOIN information_schema.constraint_column_usage AS ccu
            ON ccu.constraint_name = tc.constraint_name
          WHERE tc.constraint_type = 'FOREIGN KEY'
            AND tc.table_schema = '${schema}'
        `,
      }) as any;

      return result.rows.map((row: any[]) => ({
        from_table: row[0],
        from_column: row[1],
        to_table: row[2],
        to_column: row[3],
      }));
    } catch (e) {
      console.error('Failed to load foreign keys:', e);
      return [];
    }
  }

  function initializeCytoscape(tables: DbObject[], foreignKeys: ForeignKey[]) {
    if (cy) {
      cy.destroy();
    }

    // Create nodes (tables)
    const elements: any[] = tables.map(table => ({
      data: {
        id: table.name,
        label: table.name,
        type: 'table',
        columns: table.columns || [],
      },
      position: { x: 0, y: 0 }, // Will be laid out automatically
    }));

    // Create edges (foreign keys)
    foreignKeys.forEach(fk => {
      elements.push({
        data: {
          id: `${fk.from_table}.${fk.from_column}-${fk.to_table}.${fk.to_column}`,
          source: fk.from_table,
          target: fk.to_table,
          label: `${fk.from_column} → ${fk.to_column}`,
          type: 'foreign_key',
        },
      });
    });

    cy = cytoscape({
      container: cyContainer,
      elements,
      style: [
        {
          selector: 'node[type = "table"]',
          style: {
            'background-color': '#252540',
            'border-color': '#6366f1',
            'border-width': 2,
            'label': 'data(label)',
            'color': '#e0e0e0',
            'font-size': '14px',
            'font-weight': 'bold',
            'text-valign': 'top',
            'text-halign': 'center',
            'width': 'label',
            'height': 'label',
            'padding': '10px',
            'shape': 'rectangle',
          },
        },
        {
          selector: 'edge[type = "foreign_key"]',
          style: {
            'width': 2,
            'line-color': '#60a5fa',
            'target-arrow-color': '#60a5fa',
            'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            'label': 'data(label)',
            'font-size': '10px',
            'color': '#888',
            'text-rotation': 'autorotate',
          },
        },
      ],
      layout: {
        name: 'cose',
        animate: true,
        animationDuration: 1000,
        nodeOverlap: 20,
        idealEdgeLength: 100,
        edgeElasticity: 100,
        nestingFactor: 5,
        gravity: 80,
        numIter: 1000,
        initialTemp: 200,
        coolingFactor: 0.95,
        minEnergyThreshold: 1e-6,
      },
      zoom: 1,
      minZoom: 0.5,
      maxZoom: 2,
    });

    // Add click handler
    cy.on('tap', 'node', (event) => {
      const node = event.target;
      const tableData = node.data();
      
      // Emit event or show details
      console.log('Table selected:', tableData);
    });

    // Auto-fit
    cy.fit();
  }

  function exportDiagram(format: 'png' | 'svg' | 'json') {
    if (!cy) return;

    switch (format) {
      case 'png':
        const pngBase64 = cy.png({ full: true });
        downloadImage(pngBase64, 'er_diagram.png');
        break;
      case 'svg':
        const svgString = cy.svg({ full: true });
        downloadFile(svgString, 'er_diagram.svg', 'image/svg+xml');
        break;
      case 'json':
        const json = cy.json();
        downloadFile(JSON.stringify(json, null, 2), 'er_diagram.json', 'application/json');
        break;
    }
  }

  function downloadImage(base64: string, filename: string) {
    const link = document.createElement('a');
    link.href = base64;
    link.download = filename;
    link.click();
  }

  function downloadFile(content: string, filename: string, type: string) {
    const blob = new Blob([content], { type });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
  }

  function fitDiagram() {
    cy?.fit();
  }

  function refreshLayout() {
    if (!cy) return;
    
    const layout = cy.layout({
      name: 'cose',
      animate: true,
      animationDuration: 1000,
    });
    layout.run();
  }
</script>

<div class="er-diagrams">
  <div class="diagram-toolbar">
    <div class="toolbar-group">
      <select bind:value={selectedSchema} onchange={generateDiagram}>
        {#each schemas as schema}
          <option value={schema}>{schema}</option>
        {/each}
      </select>
      
      <button class="btn btn-secondary" onclick={refreshLayout} title="Refresh layout">
        🔄 Layout
      </button>
      
      <button class="btn btn-secondary" onclick={fitDiagram} title="Fit to screen">
        🎯 Fit
      </button>
    </div>

    <div class="toolbar-group">
      <button class="btn btn-secondary" onclick={() => exportDiagram('png')} title="Export as PNG">
        📷 PNG
      </button>
      <button class="btn btn-secondary" onclick={() => exportDiagram('svg')} title="Export as SVG">
        📐 SVG
      </button>
      <button class="btn btn-secondary" onclick={() => exportDiagram('json')} title="Export as JSON">
        💾 JSON
      </button>
    </div>
  </div>

  {#if isLoading}
    <div class="loading-overlay">
      <div class="spinner"></div>
      <p>Generating ER Diagram...</p>
    </div>
  {/if}

  {#if error}
    <div class="error-message">
      ⚠️ {error}
      <button onclick={generateDiagram}>Retry</button>
    </div>
  {/if}

  <div class="cy-container" bind:this={cyContainer}></div>

  {#if !isLoading && !error && (!cy || cy.elements().length === 0)}
    <div class="empty-state">
      <p>No tables found in schema</p>
      <p class="hint">Select a different schema or create tables</p>
    </div>
  {/if}
</div>

<style>
  .er-diagrams {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #1e1e2e;
    border-radius: 8px;
    overflow: hidden;
    position: relative;
  }

  .diagram-toolbar {
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
    align-items: center;
  }

  select {
    padding: 8px 12px;
    background: #3d3d5c;
    border: 1px solid #4d4d6c;
    border-radius: 6px;
    color: #e0e0e0;
    font-size: 13px;
    cursor: pointer;
  }

  .btn {
    padding: 8px 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    background: #3d3d5c;
    color: #e0e0e0;
    transition: all 0.2s;
  }

  .btn:hover {
    background: #4d4d6c;
  }

  .cy-container {
    flex: 1;
    min-height: 0;
    background: #1a1a2e;
  }

  .loading-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 4px solid rgba(255, 255, 255, 0.3);
    border-top-color: #6366f1;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .loading-overlay p {
    margin-top: 16px;
    color: #e0e0e0;
  }

  .error-message {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: #2d1a1a;
    border: 1px solid #ef4444;
    padding: 20px;
    border-radius: 8px;
    text-align: center;
  }

  .error-message button {
    margin-top: 12px;
    padding: 8px 16px;
    background: #ef4444;
    border: none;
    border-radius: 6px;
    color: white;
    cursor: pointer;
  }

  .empty-state {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    text-align: center;
    color: #666;
  }

  .hint {
    font-size: 12px;
    margin-top: 8px;
  }
</style>
