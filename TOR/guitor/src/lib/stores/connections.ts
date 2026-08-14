// Guitor - Connections Store
import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface Connection {
  id: string;
  name: string;
  driver: 'toroidal' | 'postgresql' | 'mysql' | 'sqlite';
  host: string;
  port: number;
  database: string;
  username?: string;
  password?: string;
  status: 'connected' | 'disconnected' | 'error';
  lastConnected?: Date;
}

export interface DriverInfo {
  id: string;
  name: string;
  displayName: string;
  icon: string;
  port: number;
  defaultPort: number;
}

export const DRIVERS: DriverInfo[] = [
  { id: 'toroidal', name: 'toroidal', displayName: 'ToroidalDB', icon: '🌀', port: 8443, defaultPort: 8443 },
  { id: 'postgresql', name: 'postgresql', displayName: 'PostgreSQL', icon: '🐘', port: 5432, defaultPort: 5432 },
  { id: 'mysql', name: 'mysql', displayName: 'MySQL', icon: '🐬', port: 3306, defaultPort: 3306 },
  { id: 'sqlite', name: 'sqlite', displayName: 'SQLite', icon: '📁', port: 0, defaultPort: 0 },
];

function createConnectionsStore() {
  const { subscribe, set, update } = writable<Connection[]>([]);

  return {
    subscribe,
    set,
    load: async () => {
      try {
        // Load from backend
        const connections = await invoke('list_connections') as Connection[];
        set(connections);
      } catch (e) {
        console.error('Failed to load connections:', e);
        // Fallback to localStorage
        const stored = localStorage.getItem('guitor-connections');
        if (stored) {
          const connections = JSON.parse(stored);
          set(connections);
        }
      }
    },
    add: async (name: string, driver: string, host: string, port: number, database: string, username?: string, password?: string) => {
      try {
        // Connect via backend
        const connId = await invoke('connect_db', {
          name,
          driver,
          host,
          port,
          database,
          username: username || null,
          password: password || null,
        }) as string;

        const connection: Connection = {
          id: connId,
          name,
          driver: driver as any,
          host,
          port,
          database,
          username,
          password,
          status: 'connected',
          lastConnected: new Date(),
        };

        update(connections => {
          const updated = [...connections, connection];
          // Also save to localStorage as backup
          localStorage.setItem('guitor-connections', JSON.stringify(updated));
          return updated;
        });

        return connection;
      } catch (e) {
        console.error('Failed to add connection:', e);
        return null;
      }
    },
    remove: async (id: string) => {
      try {
        await invoke('disconnect_db', { conn_id: id });
      } catch (e) {
        console.error('Failed to disconnect:', e);
      }
      
      update(connections => {
        const updated = connections.filter(c => c.id !== id);
        localStorage.setItem('guitor-connections', JSON.stringify(updated));
        return updated;
      });
    },
    updateStatus: (id: string, status: Connection['status']) => {
      update(connections => {
        return connections.map(c =>
          c.id === id ? { ...c, status, lastConnected: status === 'connected' ? new Date() : c.lastConnected } : c
        );
      });
    },
  };
}

export const connections = createConnectionsStore();
export const activeConnectionId = writable<string | null>(null);

export const activeConnection = derived(
  [connections, activeConnectionId],
  ([$connections, $activeConnectionId]) =>
    $connections.find(c => c.id === $activeConnectionId) || null
);

export const connectedConnections = derived(
  connections,
  $connections => $connections.filter(c => c.status === 'connected')
);

export async function loadConnections() {
  await connections.load();
}

export async function addConnection(name: string, driver: string, host: string, port: number, database: string, username?: string, password?: string) {
  return await connections.add(name, driver, host, port, database, username, password);
}

export async function removeConnection(id: string) {
  await connections.remove(id);
}

export async function testConnection(driver: string, host: string, port: number, database: string, username?: string, password?: string): Promise<boolean> {
  return await invoke('test_connection', { 
    driver, 
    host, 
    port, 
    database,
    username: username || null,
    password: password || null,
  }) as boolean;
}
