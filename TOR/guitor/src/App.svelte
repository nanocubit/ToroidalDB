<script lang="ts">
  import { onMount } from 'svelte';
  import { connections, loadConnections, addConnection, removeConnection, DRIVERS } from './lib/stores/connections';
  import { queryHistory } from './lib/stores/query';
  import DatabaseExplorer from './lib/components/DatabaseExplorer.svelte';
  import QueryEditor from './lib/components/QueryEditor.svelte';
  import ResultsTable from './lib/components/ResultsTable.svelte';

  let activeTab = 'query';
  let showConnectionModal = false;
  let showHistory = false;

  let connName = '';
  let connDriver = 'PostgreSQL';
  let connHost = 'localhost';
  let connPort = 5432;
  let connDatabase = 'postgres';
  let connUsername = 'postgres';
  let connPassword = '';

  onMount(async () => {
    await loadConnections();
  });

  $: currentDriver = DRIVERS.find(d => d.name === connDriver);

  async function handleAddConnection() {
    if (!connName || !connHost || !connDatabase || !connUsername) {
      alert('Please fill in all required fields');
      return;
    }

    const conn = await addConnection(
      connName,
      connDriver.toLowerCase(),
      connHost,
      connPort,
      connDatabase,
      connUsername,
      connPassword
    );

    if (conn) {
      showConnectionModal = false;
      connName = '';
      connPassword = '';
    }
  }

  function handleDriverChange() {
    if (currentDriver) {
      connPort = currentDriver.defaultPort;
    }
  }
</script>

<main class="app">
  <header class="header">
    <div class="logo">
      <span class="logo-icon">🌀</span>
      <span class="logo-text">GuiTor</span>
    </div>
    <nav class="nav">
      <button 
        class="nav-btn" 
        class:active={activeTab === 'query'}
        onclick={() => activeTab = 'query'}
      >
        Query
      </button>
      <button 
        class="nav-btn" 
        class:active={activeTab === 'structure'}
        onclick={() => activeTab = 'structure'}
      >
        Structure
      </button>
    </nav>
    <div class="header-actions">
      <button class="action-btn" onclick={() => showConnectionModal = true}>
        + Add Connection
      </button>
      <button class="action-btn" onclick={() => showHistory = !showHistory}>
        History
      </button>
    </div>
  </header>

  <div class="main-content">
    <aside class="sidebar">
      <DatabaseExplorer />
    </aside>

    <main class="content">
      {#if activeTab === 'query'}
        <div class="query-section">
          <div class="editor-panel">
            <QueryEditor />
          </div>
          <div class="results-panel">
            <ResultsTable />
          </div>
        </div>
      {:else}
        <div class="structure-section">
          <p>Select a connection to view its schema</p>
        </div>
      {/if}
    </main>

    {#if showHistory}
      <aside class="history-panel">
        <div class="history-header">
          <h3>Query History</h3>
          <button class="close-btn" onclick={() => showHistory = false}>×</button>
        </div>
        <div class="history-list">
          {#each $queryHistory as entry}
            <div class="history-item" class:error={!entry.success}>
              <div class="history-query">{entry.query?.substring(0, 50) || ''}...</div>
              <div class="history-meta">
                <span class="duration">{entry.duration}ms</span>
                <span class="time">{entry.executedAt?.toLocaleTimeString() || ''}</span>
              </div>
            </div>
          {:else}
            <div class="empty-history">No queries executed yet</div>
          {/each}
        </div>
      </aside>
    {/if}
  </div>
</main>

{#if showConnectionModal}
  <div class="modal-overlay" onclick={() => showConnectionModal = false}>
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>Add New Connection</h2>
        <button class="close-btn" onclick={() => showConnectionModal = false}>×</button>
      </div>
      <div class="modal-body">
        <div class="form-group">
          <label for="name">Connection Name</label>
          <input 
            id="name" 
            type="text" 
            bind:value={connName} 
            placeholder="My Database"
          />
        </div>
        <div class="form-group">
          <label for="driver">Database Type</label>
          <select id="driver" bind:value={connDriver} onchange={handleDriverChange}>
            {#each DRIVERS as driver}
              <option value={driver.name}>{driver.icon} {driver.displayName}</option>
            {/each}
          </select>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="host">Host</label>
            <input id="host" type="text" bind:value={connHost} placeholder="localhost" />
          </div>
          <div class="form-group">
            <label for="port">Port</label>
            <input id="port" type="number" bind:value={connPort} />
          </div>
        </div>
        <div class="form-group">
          <label for="database">Database</label>
          <input id="database" type="text" bind:value={connDatabase} placeholder="postgres" />
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="username">Username</label>
            <input id="username" type="text" bind:value={connUsername} />
          </div>
          <div class="form-group">
            <label for="password">Password</label>
            <input id="password" type="password" bind:value={connPassword} />
          </div>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn secondary" onclick={() => showConnectionModal = false}>Cancel</button>
        <button class="btn primary" onclick={handleAddConnection}>Connect</button>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0f0f1a;
    color: #e0e0e0;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    padding: 0 20px;
    height: 56px;
    background: #1a1a2e;
    border-bottom: 1px solid #2d2d44;
  }

  .logo {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .logo-icon {
    font-size: 24px;
  }

  .logo-text {
    font-size: 18px;
    font-weight: 700;
    background: linear-gradient(45deg, #8b5cf6, #6366f1);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
  }

  .nav {
    display: flex;
    gap: 8px;
    margin-left: 40px;
  }

  .nav-btn {
    padding: 8px 16px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: #888;
    font-size: 14px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .nav-btn:hover,
  .nav-btn.active {
    color: #e0e0e0;
    background: #252540;
  }

  .header-actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .action-btn {
    padding: 8px 16px;
    background: #252540;
    border: 1px solid #3d3d5c;
    border-radius: 6px;
    color: #e0e0e0;
    font-size: 13px;
    cursor: pointer;
  }

  .action-btn:hover {
    background: #3d3d5c;
  }

  .main-content {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  .sidebar {
    width: 280px;
    border-right: 1px solid #2d2d44;
    overflow: hidden;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .query-section {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 16px;
    gap: 16px;
    overflow: hidden;
  }

  .editor-panel {
    flex: 0 0 40%;
    min-height: 200px;
  }

  .results-panel {
    flex: 1;
    min-height: 200px;
  }

  .structure-section {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #666;
  }

  .history-panel {
    width: 300px;
    background: #1a1a2e;
    border-left: 1px solid #2d2d44;
    display: flex;
    flex-direction: column;
  }

  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px;
    border-bottom: 1px solid #2d2d44;
  }

  .history-header h3 {
    margin: 0;
    font-size: 14px;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 20px;
    cursor: pointer;
  }

  .history-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }

  .history-item {
    padding: 10px;
    background: #252540;
    border-radius: 6px;
    margin-bottom: 8px;
    cursor: pointer;
  }

  .history-item:hover {
    background: #3d3d5c;
  }

  .history-item.error {
    border-left: 3px solid #ef4444;
  }

  .history-query {
    font-size: 12px;
    color: #e0e0e0;
    margin-bottom: 4px;
    font-family: monospace;
  }

  .history-meta {
    display: flex;
    gap: 8px;
    font-size: 11px;
    color: #666;
  }

  .empty-history {
    padding: 20px;
    text-align: center;
    color: #666;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: #1a1a2e;
    border-radius: 12px;
    width: 480px;
    max-width: 90vw;
    border: 1px solid #2d2d44;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px;
    border-bottom: 1px solid #2d2d44;
  }

  .modal-header h2 {
    margin: 0;
    font-size: 18px;
  }

  .modal-body {
    padding: 20px;
  }

  .form-group {
    margin-bottom: 16px;
  }

  .form-group label {
    display: block;
    margin-bottom: 6px;
    font-size: 13px;
    color: #888;
  }

  .form-group input,
  .form-group select {
    width: 100%;
    padding: 10px 12px;
    background: #252540;
    border: 1px solid #3d3d5c;
    border-radius: 6px;
    color: #e0e0e0;
    font-size: 14px;
  }

  .form-row {
    display: flex;
    gap: 16px;
  }

  .form-row .form-group {
    flex: 1;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
    padding: 20px;
    border-top: 1px solid #2d2d44;
  }

  .btn {
    padding: 10px 20px;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    cursor: pointer;
  }

  .btn.primary {
    background: linear-gradient(45deg, #6366f1, #8b5cf6);
    color: white;
  }

  .btn.secondary {
    background: #252540;
    color: #e0e0e0;
  }
</style>
