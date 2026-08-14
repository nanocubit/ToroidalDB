// Guitor - Query Store
import { writable } from 'svelte/store';

export interface QueryResult {
  columns: string[];
  types: string[];
  rows: any[][];
}

export interface QueryHistoryItem {
  id: string;
  connectionId: string;
  query: string;
  executedAt: Date;
  duration: number;
  rowCount: number;
  success: boolean;
  error?: string;
}

function createQueryStore() {
  const { subscribe, set, update } = writable<{
    result: QueryResult | null;
    executing: boolean;
    error: string | null;
  }>({
    result: null,
    executing: false,
    error: null,
  });

  return {
    subscribe,
    set,
    setResult: (result: QueryResult) => {
      update(state => ({ ...state, result, executing: false, error: null }));
    },
    setExecuting: (executing: boolean) => {
      update(state => ({ ...state, executing }));
    },
    setError: (error: string) => {
      update(state => ({ ...state, error, executing: false }));
    },
    clear: () => {
      update(state => ({ ...state, result: null, error: null }));
    },
  };
}

export const queryResults = createQueryStore();
export const queryResult = writable<QueryResult | null>(null);
export const isExecuting = writable<boolean>(false);
export const queryError = writable<string | null>(null);

export const queryHistory = writable<QueryHistoryItem[]>([]);

export const currentQuery = writable<string>('');

export function setQuery(query: string) {
  currentQuery.set(query);
}

export function clearResult() {
  queryResult.set(null);
  queryError.set(null);
}

export async function executeQuery(connectionId: string, query: string): Promise<void> {
  isExecuting.set(true);
  queryError.set(null);
  
  try {
    // Import invoke from Tauri
    const { invoke } = await import('@tauri-apps/api/core');
    
    const result = await invoke('execute_query', { connectionId, query }) as QueryResult | null;
    queryResult.set(result);
    
    // Add to history
    queryHistory.update(history => [{
      id: crypto.randomUUID(),
      connectionId,
      query,
      executedAt: new Date(),
      duration: 0,
      rowCount: result?.rows?.length || 0,
      success: true,
    }, ...history].slice(0, 1000));
  } catch (e: any) {
    queryError.set(e.message || String(e));
    queryResult.set(null);
    
    queryHistory.update(history => [{
      id: crypto.randomUUID(),
      connectionId,
      query,
      executedAt: new Date(),
      duration: 0,
      rowCount: 0,
      success: false,
      error: e.message || String(e),
    }, ...history].slice(0, 1000));
  } finally {
    isExecuting.set(false);
  }
}

export function loadHistory() {
  try {
    const stored = localStorage.getItem('guitor-query-history');
    if (stored) {
      const history = JSON.parse(stored);
      queryHistory.set(history.map((h: any) => ({ ...h, executedAt: new Date(h.executedAt) })));
    }
  } catch (e) {
    console.error('Failed to load query history:', e);
  }
}

export function clearHistory() {
  localStorage.removeItem('guitor-query-history');
  queryHistory.set([]);
}
