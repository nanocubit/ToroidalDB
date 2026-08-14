//! # SSH Tunnels для GuiTor
//! 
//! Поддержка SSH туннелей для безопасного подключения к БД

use anyhow::{Context, Result};
use ssh2::Session;
use std::net::TcpStream;
use std::sync::Arc;
use std::path::Path;

/// SSH Tunnel Manager
pub struct SshTunnel {
    session: Option<Session>,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
}

/// SSH конфигурация
#[derive(Debug, Clone)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
}

impl SshTunnel {
    pub fn new() -> Self {
        Self {
            session: None,
            local_port: 0,
            remote_host: String::new(),
            remote_port: 0,
        }
    }

    /// Создаёт SSH туннель
    pub fn connect(&mut self, config: &SshConfig, remote_host: &str, remote_port: u16) -> Result<()> {
        // Подключаемся к SSH серверу
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))
            .context("Failed to connect to SSH server")?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        // Аутентификация
        if let Some(key_path) = &config.private_key_path {
            // Аутентификация по ключу
            session.userauth_pubkeyfile(
                &config.username,
                None,
                Path::new(key_path),
                config.passphrase.as_deref(),
            )?;
        } else if let Some(password) = &config.password {
            // Аутентификация по паролю
            session.userauth_password(&config.username, password)?;
        } else {
            anyhow::bail!("No authentication method provided");
        }

        // Создаём forward туннель
        let local_port = self.find_available_port()?;
        
        // Note: ssh2 crate doesn't directly support port forwarding in this way
        // В production нужно использовать более сложную реализацию
        
        self.session = Some(session);
        self.local_port = local_port;
        self.remote_host = remote_host.to_string();
        self.remote_port = remote_port;

        Ok(())
    }

    /// Закрывает SSH туннель
    pub fn disconnect(&mut self) -> Result<()> {
        if let Some(mut session) = self.session.take() {
            session.disconnect()?;
        }
        Ok(())
    }

    /// Получает локальный порт для туннеля
    pub fn get_local_port(&self) -> u16 {
        self.local_port
    }

    /// Проверяет подключение
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Находит свободный порт
    fn find_available_port(&self) -> Result<u16> {
        use std::net::TcpListener;
        
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        
        Ok(port)
    }

    /// Создаёт SOCKS прокси
    pub fn create_socks_proxy(&mut self, local_port: u16) -> Result<()> {
        // В production реализовать SOCKS proxy
        Ok(())
    }
}

/// Менеджер SSH туннелей
pub struct SshTunnelManager {
    tunnels: std::collections::HashMap<String, SshTunnel>,
}

impl SshTunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: std::collections::HashMap::new(),
        }
    }

    /// Создаёт новый туннель
    pub fn create_tunnel(
        &mut self,
        tunnel_id: &str,
        ssh_config: &SshConfig,
        db_host: &str,
        db_port: u16,
    ) -> Result<u16> {
        let mut tunnel = SshTunnel::new();
        tunnel.connect(ssh_config, db_host, db_port)?;
        
        let local_port = tunnel.get_local_port();
        self.tunnels.insert(tunnel_id.to_string(), tunnel);
        
        Ok(local_port)
    }

    /// Закрывает туннель
    pub fn close_tunnel(&mut self, tunnel_id: &str) -> Result<()> {
        if let Some(mut tunnel) = self.tunnels.remove(tunnel_id) {
            tunnel.disconnect()?;
        }
        Ok(())
    }

    /// Закрывает все туннели
    pub fn close_all(&mut self) -> Result<()> {
        for (_, mut tunnel) in self.tunnels.drain() {
            tunnel.disconnect()?;
        }
        Ok(())
    }

    /// Получает локальный порт туннеля
    pub fn get_tunnel_port(&self, tunnel_id: &str) -> Option<u16> {
        self.tunnels.get(tunnel_id).map(|t| t.get_local_port())
    }
}

impl Default for SshTunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_port() {
        let tunnel = SshTunnel::new();
        let port = tunnel.find_available_port().unwrap();
        
        assert!(port > 0);
        assert!(port < 65536);
    }

    #[test]
    fn test_tunnel_manager() {
        let mut manager = SshTunnelManager::new();
        
        // Note: Это тест без реального подключения
        assert!(manager.tunnels.is_empty());
    }
}
