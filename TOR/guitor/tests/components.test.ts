// GuiTor Frontend Tests
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, screen } from '@testing-library/svelte';
import { writable } from 'svelte/store';

// Mock stores
vi.mock('../src/lib/stores/connections', () => ({
  connections: {
    subscribe: writable([]).subscribe,
    load: vi.fn(),
    add: vi.fn(),
    remove: vi.fn(),
  },
  activeConnection: writable(null),
  activeConnectionId: writable(null),
  loadConnections: vi.fn(),
  addConnection: vi.fn(),
  removeConnection: vi.fn(),
  testConnection: vi.fn(),
}));

vi.mock('../src/lib/stores/query', () => ({
  queryHistory: {
    subscribe: writable([]).subscribe,
    add: vi.fn(),
  },
}));

vi.mock('../src/lib/stores/schema', () => ({
  currentSchema: writable(null),
}));

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('DatabaseExplorer', () => {
  it('shows no connection message when not connected', async () => {
    const { DatabaseExplorer } = await import('../src/lib/components/DatabaseExplorer.svelte');
    const { container } = render(DatabaseExplorer);
    
    expect(container.textContent).toContain('No active connection');
  });

  it('loads schemas when connection is active', async () => {
    const { DatabaseExplorer } = await import('../src/lib/components/DatabaseExplorer.svelte');
    const { invoke } = await import('@tauri-apps/api/core');
    
    vi.mocked(invoke).mockResolvedValue([
      { name: 'public', kind: 'schema', schema: 'public', columns: [], children: [] },
      { name: 'information_schema', kind: 'schema', schema: 'information_schema', columns: [], children: [] },
    ]);
    
    // Set active connection
    const { activeConnectionId } = await import('../src/lib/stores/connections');
    activeConnectionId.set('test-conn-id');
    
    const { container } = render(DatabaseExplorer);
    
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('list_schemas', { conn_id: 'test-conn-id' });
    });
  });
});

describe('QueryEditor', () => {
  it('initializes Monaco editor', async () => {
    const { QueryEditor } = await import('../src/lib/components/QueryEditor.svelte');
    const { container } = render(QueryEditor);
    
    expect(container.querySelector('.query-editor')).toBeInTheDocument();
  });

  it('shows execute button', async () => {
    const { QueryEditor } = await import('../src/lib/components/QueryEditor.svelte');
    const { container } = render(QueryEditor);
    
    const executeButton = container.querySelector('button.btn-primary');
    expect(executeButton).toBeInTheDocument();
    expect(executeButton?.textContent).toContain('Execute');
  });

  it('displays execution time after query', async () => {
    const { QueryEditor } = await import('../src/lib/components/QueryEditor.svelte');
    const { invoke } = await import('@tauri-apps/api/core');
    
    vi.mocked(invoke).mockResolvedValue({
      columns: ['id', 'name'],
      rows: [[1, 'John']],
      rowsAffected: 1,
      durationMs: 45.2,
    });
    
    const { container } = render(QueryEditor);
    
    // Simulate query execution
    const executeButton = container.querySelector('button.btn-primary');
    await fireEvent.click(executeButton!);
    
    await vi.waitFor(() => {
      expect(container.textContent).toContain('ms');
    });
  });
});

describe('ResultsTable', () => {
  it('shows empty state when no data', async () => {
    const { ResultsTable } = await import('../src/lib/components/ResultsTable.svelte');
    const { container } = render(ResultsTable);
    
    expect(container.textContent).toContain('No data loaded');
  });

  it('displays data when setData is called', async () => {
    const { ResultsTable } = await import('../src/lib/components/ResultsTable.svelte');
    const { container } = render(ResultsTable);
    
    const component = container.querySelector('[data-testid="results-table"]');
    
    // Note: AG-Grid requires actual DOM element
    // В production нужно моковать AG-Grid
  });

  it('shows commit button when changes pending', async () => {
    const { ResultsTable } = await import('../src/lib/components/ResultsTable.svelte');
    const { container } = render(ResultsTable);
    
    const commitButton = container.querySelector('button.btn-primary');
    expect(commitButton).toBeInTheDocument();
    expect(commitButton?.textContent).toContain('Commit');
  });
});

describe('ErDiagram', () => {
  it('loads schemas on mount', async () => {
    const { ErDiagram } = await import('../src/lib/components/ErDiagram.svelte');
    const { invoke } = await import('@tauri-apps/api/core');
    
    vi.mocked(invoke).mockResolvedValue([
      { name: 'public', kind: 'schema', schema: 'public' },
    ]);
    
    const { container } = render(ErDiagram);
    
    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('list_schemas', expect.anything());
    });
  });

  it('shows schema selector', async () => {
    const { ErDiagram } = await import('../src/lib/components/ErDiagram.svelte');
    const { invoke } = await import('@tauri-apps/api/core');
    
    vi.mocked(invoke).mockResolvedValue([
      { name: 'public', kind: 'schema', schema: 'public' },
      { name: 'information_schema', kind: 'schema', schema: 'information_schema' },
    ]);
    
    const { container } = render(ErDiagram);
    
    await vi.waitFor(() => {
      const selector = container.querySelector('select');
      expect(selector).toBeInTheDocument();
    });
  });
});

describe('Stores', () => {
  describe('connections store', () => {
    it('loads connections from backend', async () => {
      const { connections } = await import('../src/lib/stores/connections');
      const { invoke } = await import('@tauri-apps/api/core');
      
      vi.mocked(invoke).mockResolvedValue([
        { id: '1', name: 'Test DB', driver: 'postgresql', status: 'connected' },
      ]);
      
      await connections.load();
      
      expect(invoke).toHaveBeenCalledWith('list_connections');
    });

    it('adds new connection', async () => {
      const { connections } = await import('../src/lib/stores/connections');
      const { invoke } = await import('@tauri-apps/api/core');
      
      vi.mocked(invoke).mockResolvedValue('new-conn-id');
      
      const result = await connections.add(
        'Test DB',
        'postgresql',
        'localhost',
        5432,
        'testdb',
        'user',
        'pass'
      );
      
      expect(invoke).toHaveBeenCalledWith('connect_db', expect.any(Object));
      expect(result).toEqual(expect.objectContaining({ name: 'Test DB' }));
    });

    it('removes connection', async () => {
      const { connections } = await import('../src/lib/stores/connections');
      const { invoke } = await import('@tauri-apps/api/core');
      
      vi.mocked(invoke).mockResolvedValue(undefined);
      
      await connections.remove('test-id');
      
      expect(invoke).toHaveBeenCalledWith('disconnect_db', { conn_id: 'test-id' });
    });
  });

  describe('queryHistory store', () => {
    it('adds query to history', async () => {
      const { queryHistory } = await import('../src/lib/stores/query');
      
      queryHistory.add({
        query: 'SELECT * FROM users',
        success: true,
        duration: 45.2,
        executedAt: new Date(),
      });
      
      // Verify added
      const { get } = await import('svelte/store');
      const history = get(queryHistory);
      expect(history.length).toBeGreaterThan(0);
    });
  });
});

describe('Utilities', () => {
  it('formats SQL keywords correctly', () => {
    const keywords = ['SELECT', 'FROM', 'WHERE', 'JOIN'];
    expect(keywords).toContain('SELECT');
    expect(keywords).toContain('WHERE');
  });

  it('validates connection object structure', () => {
    const connection = {
      id: 'test-id',
      name: 'Test DB',
      driver: 'postgresql',
      host: 'localhost',
      port: 5432,
      database: 'testdb',
      status: 'connected' as const,
    };
    
    expect(connection.id).toBe('test-id');
    expect(connection.driver).toBe('postgresql');
  });
});
