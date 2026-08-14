use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: u64,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub action: AuditAction,
    pub resource: String,
    pub status: AuditStatus,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: Option<serde_json::Value>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AuditAction {
    Login,
    Logout,
    Query,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Grant,
    Revoke,
    ConfigChange,
    Admin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuditStatus {
    Success,
    Failure,
    Denied,
}

pub struct AuditLogger {
    entries: Arc<RwLock<VecDeque<AuditEntry>>>,
    max_entries: usize,
}

impl AuditLogger {
    pub fn new(max_entries: usize) -> Self {
        AuditLogger {
            entries: Arc::new(RwLock::new(VecDeque::with_capacity(max_entries))),
            max_entries,
        }
    }

    pub fn log(&self, entry: AuditEntry) {
        let mut entries = self.entries.write().unwrap();

        if entries.len() >= self.max_entries {
            entries.pop_front();
        }

        entries.push_back(entry);
    }

    pub fn log_query(
        &self,
        user_id: Option<&str>,
        query: &str,
        status: AuditStatus,
        duration_ms: u64,
    ) {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            user_id: user_id.map(String::from),
            username: None,
            action: AuditAction::Query,
            resource: query.to_string(),
            status,
            ip_address: None,
            user_agent: None,
            details: None,
            duration_ms: Some(duration_ms),
        };

        self.log(entry);
    }

    pub fn log_login(&self, username: &str, success: bool, ip: Option<&str>) {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            user_id: None,
            username: Some(username.to_string()),
            action: AuditAction::Login,
            resource: "auth".to_string(),
            status: if success {
                AuditStatus::Success
            } else {
                AuditStatus::Failure
            },
            ip_address: ip.map(String::from),
            user_agent: None,
            details: None,
            duration_ms: None,
        };

        self.log(entry);
    }

    pub fn log_mutation(
        &self,
        user_id: Option<&str>,
        action: AuditAction,
        resource: &str,
        status: AuditStatus,
    ) {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            user_id: user_id.map(String::from),
            username: None,
            action,
            resource: resource.to_string(),
            status,
            ip_address: None,
            user_agent: None,
            details: None,
            duration_ms: None,
        };

        self.log(entry);
    }

    pub fn get_entries(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().unwrap();
        entries.iter().rev().take(limit).cloned().collect()
    }

    pub fn get_entries_for_user(&self, user_id: &str, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .filter(|e| e.user_id.as_deref() == Some(user_id))
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get_entries_by_action(&self, action: AuditAction, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .filter(|e| e.action == action)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get_failed_logins(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .filter(|e| {
                matches!(e.action, AuditAction::Login) && matches!(e.status, AuditStatus::Failure)
            })
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }

    pub fn count(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    pub fn export_json(&self) -> String {
        let entries = self.entries.read().unwrap();
        serde_json::to_string_pretty(&*entries).unwrap_or_default()
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_query() {
        let logger = AuditLogger::new(100);

        logger.log_query(
            Some("user1"),
            "SELECT * FROM users",
            AuditStatus::Success,
            50,
        );

        let entries = logger.get_entries(10);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].action, AuditAction::Query));
    }

    #[test]
    fn test_audit_log_login() {
        let logger = AuditLogger::new(100);

        logger.log_login("admin", true, Some("192.168.1.1"));

        let entries = logger.get_entries(10);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].action, AuditAction::Login));
    }

    #[test]
    fn test_audit_get_failed_logins() {
        let logger = AuditLogger::new(100);

        logger.log_login("admin", false, Some("192.168.1.1"));
        logger.log_login("admin", false, Some("192.168.1.2"));
        logger.log_login("admin", true, Some("192.168.1.1"));

        let failed = logger.get_failed_logins(10);
        assert_eq!(failed.len(), 2);
    }

    #[test]
    fn test_audit_export() {
        let logger = AuditLogger::new(100);

        logger.log_query(Some("user1"), "SELECT 1", AuditStatus::Success, 10);

        let json = logger.export_json();
        assert!(json.contains("Query"));
    }

    #[test]
    fn test_audit_clear() {
        let logger = AuditLogger::new(100);

        logger.log_query(Some("user1"), "SELECT 1", AuditStatus::Success, 10);
        assert_eq!(logger.count(), 1);

        logger.clear();
        assert_eq!(logger.count(), 0);
    }
}
