<script lang="ts">
  import { onMount, onDestroy, createEventDispatch } from 'svelte';
  import { GridApi, ColDef, CellValueChangedEvent } from 'ag-grid-community';
  import { activeConnection } from '../stores/connections';
  import { currentSchema } from '../stores/schema';

  interface QueryResult {
    columns: string[];
    rows: any[][];
    rowsAffected: number;
    durationMs: number;
  }

  let gridApi: GridApi | null = null;
  let gridContainer: HTMLDivElement;
  let currentData: any[] = [];
  let isLoading = false;
  let isEditing = false;
  let pendingChanges: Map<number, Map<string, any>> = new Map();

  const dispatch = createEventDispatch<{
    cellEdit: { row: number; column: string; oldValue: any; newValue: any };
    commit: { changes: Array<{ row: number; column: string; value: any }> };
    export: { format: string };
  }>();

  onMount(() => {
    // AG-Grid будет инициализирован при получении данных
  });

  onDestroy(() => {
    gridApi?.destroy();
  });

  function initializeGrid(columnDefs: ColDef[], rowData: any[]) {
    if (!gridContainer) return;

    if (gridApi) {
      gridApi.destroy();
    }

    const gridOptions = {
      defaultColDef: {
        sortable: true,
        filter: true,
        resizable: true,
        editable: true,
        minWidth: 100,
        flex: 1,
      },
      columnDefs,
      rowData,
      domLayout: 'normal',
      pagination: true,
      paginationPageSize: 100,
      rowSelection: 'multiple',
      enableRangeSelection: true,
      enableCellTextSelection: true,
      stopEditingWhenCellsLoseFocus: true,
      onCellValueChanged: handleCellValueChanged,
      onRowDataUpdated: () => {
        gridApi?.sizeColumnsToFit();
      },
    };

    gridApi = new GridApi(gridOptions);
    gridApi.gridOptions = gridOptions;
    gridApi.setGridOption('columnDefs', columnDefs);
    gridApi.setGridOption('rowData', rowData);
  }

  function handleCellValueChanged(event: CellValueChangedEvent) {
    const rowIndex = event.rowIndex ?? 0;
    const column = event.column.getColId();
    const oldValue = event.oldValue;
    const newValue = event.newValue;

    if (!pendingChanges.has(rowIndex)) {
      pendingChanges.set(rowIndex, new Map());
    }
    pendingChanges.get(rowIndex)?.set(column, { oldValue, newValue });

    dispatch('cellEdit', {
      row: rowIndex,
      column,
      oldValue,
      newValue,
    });
  }

  export function setData(result: QueryResult) {
    isLoading = false;
    currentData = result.rows.map((row, i) => {
      const obj: any = { __rowIndex: i };
      result.columns.forEach((col, j) => {
        obj[col] = row[j];
      });
      return obj;
    });

    const columnDefs: ColDef[] = [
      {
        headerName: '#',
        field: '__rowIndex',
        width: 60,
        sortable: false,
        filter: false,
        editable: false,
        cellStyle: { color: '#888', textAlign: 'center' },
      },
      ...result.columns.map((col) => ({
        headerName: col,
        field: col,
        editable: true,
      })),
    ];

    initializeGrid(columnDefs, currentData);
  }

  export function setLoading(loading: boolean) {
    isLoading = loading;
  }

  export function getSelectedRows(): any[] {
    if (!gridApi) return [];
    return gridApi.getSelectedRows();
  }

  export function commitChanges() {
    const changes: Array<{ row: number; column: string; value: any }> = [];

    pendingChanges.forEach((rowChanges, rowIndex) => {
      rowChanges.forEach((change, column) => {
        changes.push({
          row: rowIndex,
          column,
          value: change.newValue,
        });
      });
    });

    if (changes.length > 0) {
      dispatch('commit', { changes });
      pendingChanges.clear();
    }

    return changes;
  }

  export function revertChanges() {
    if (!gridApi) return;

    pendingChanges.forEach((rowChanges, rowIndex) => {
      rowChanges.forEach((change, column) => {
        const rowNode = gridApi!.getDisplayedRowAtIndex(rowIndex);
        if (rowNode) {
          rowNode.setDataValue(column, change.oldValue);
        }
      });
    });

    pendingChanges.clear();
  }

  export function exportData(format: 'csv' | 'excel' | 'json') {
    if (!gridApi) return;

    switch (format) {
      case 'csv':
        gridApi.exportDataAsCsv({
          fileName: `export_${new Date().toISOString()}.csv`,
        });
        break;
      case 'excel':
        // Requires ag-grid-enterprise
        console.log('Excel export requires enterprise license');
        break;
      case 'json':
        const jsonData = gridApi.getRowModel().rows.map((row: any) => row.data);
        const blob = new Blob([JSON.stringify(jsonData, null, 2)], {
          type: 'application/json',
        });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `export_${new Date().toISOString()}.json`;
        a.click();
        URL.revokeObjectURL(url);
        break;
    }

    dispatch('export', { format });
  }

  export function refreshData() {
    if (!gridApi) return;
    gridApi.refreshCells({ force: true });
  }

  export function addRow() {
    if (!gridApi) return;
    const newRow = { __rowIndex: currentData.length };
    currentData.forEach((col) => {
      newRow[col] = null;
    });
    gridApi.applyTransaction({ add: [newRow] });
  }

  export function deleteSelectedRows() {
    if (!gridApi) return;
    const selectedRows = gridApi.getSelectedRows();
    gridApi.applyTransaction({ remove: selectedRows });
  }

  export function getColumnTypes(): Map<string, string> {
    const types = new Map<string, string>();
    if (!gridApi) return types;

    const columns = gridApi.getColumnDefs() as ColDef[];
    columns.forEach((col: any) => {
      if (col.field && col.field !== '__rowIndex') {
        types.set(col.field, col.cellDataType || 'text');
      }
    });

    return types;
  }

  export function getPendingChanges() {
    return pendingChanges;
  }

  export function getRowCount(): number {
    return gridApi ? gridApi.getDisplayedRowCount() : 0;
  }
</script>

<div class="results-table">
  <div class="table-toolbar">
    <div class="toolbar-group">
      <button
        class="btn btn-primary"
        onclick={commitChanges}
        disabled={pendingChanges.size === 0}
        title="Commit changes to database"
      >
        💾 Commit ({pendingChanges.size})
      </button>
      <button
        class="btn btn-secondary"
        onclick={revertChanges}
        disabled={pendingChanges.size === 0}
        title="Revert changes"
      >
        ↩ Revert
      </button>
    </div>

    <div class="toolbar-group">
      <button
        class="btn btn-secondary"
        onclick={() => addRow()}
        title="Add new row"
      >
        ➕ Add Row
      </button>
      <button
        class="btn btn-danger"
        onclick={deleteSelectedRows}
        title="Delete selected rows"
      >
        🗑 Delete
      </button>
    </div>

    <div class="toolbar-group">
      <button
        class="btn btn-secondary"
        onclick={() => exportData('csv')}
        title="Export as CSV"
      >
        📄 CSV
      </button>
      <button
        class="btn btn-secondary"
        onclick={() => exportData('json')}
        title="Export as JSON"
      >
        📋 JSON
      </button>
    </div>

    <div class="toolbar-info">
      {#if isLoading}
        <span class="loading">⏳ Loading...</span>
      {:else if gridApi}
        <span class="row-count">
          📊 {getRowCount()} rows
        </span>
      {/if}
    </div>
  </div>

  <div
    class="ag-theme-alpine-dark grid-container"
    bind:this={gridContainer}
  ></div>

  {#if !gridApi && !isLoading}
    <div class="empty-results">
      <p>No data loaded</p>
      <p class="hint">Execute a query to see results</p>
    </div>
  {/if}
</div>

<style>
  .results-table {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #1e1e2e;
    border-radius: 8px;
    overflow: hidden;
  }

  .table-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    background: #252540;
    border-bottom: 1px solid #3d3d5c;
    gap: 12px;
    flex-wrap: wrap;
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
  }

  .btn-secondary {
    background: #3d3d5c;
    color: #e0e0e0;
  }

  .btn-secondary:hover {
    background: #4d4d6c;
  }

  .btn-danger {
    background: #ef4444;
    color: white;
  }

  .btn-danger:hover {
    background: #dc2626;
  }

  .toolbar-info {
    margin-left: auto;
    font-size: 12px;
    color: #888;
  }

  .loading {
    color: #60a5fa;
  }

  .row-count {
    color: #4ade80;
  }

  .grid-container {
    flex: 1;
    min-height: 0;
    --ag-background-color: #1e1e2e;
    --ag-foreground-color: #e0e0e0;
    --ag-border-color: #3d3d5c;
    --ag-header-background-color: #252540;
    --ag-row-hover-color: #2d2d44;
    --ag-cell-horizontal-border: 1px solid #3d3d5c;
  }

  .empty-results {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #666;
  }

  .hint {
    font-size: 12px;
    margin-top: 8px;
  }
</style>
