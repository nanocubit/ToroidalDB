use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TwoPhaseCommit {
    coordinator: Arc<RwLock<Coordinator>>,
    participants: Arc<RwLock<HashMap<String, Participant>>>,
    config: TwoPCConfig,
}

#[derive(Clone, Debug)]
pub struct TwoPCConfig {
    pub prepare_timeout: Duration,
    pub commit_timeout: Duration,
    pub max_retries: u32,
}

impl Default for TwoPCConfig {
    fn default() -> Self {
        TwoPCConfig {
            prepare_timeout: Duration::from_secs(30),
            commit_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

#[derive(Clone, Debug)]
enum CoordinatorState {
    Initial,
    Preparing,
    Prepared,
    Committing,
    Committed,
    Aborting,
    Aborted,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
enum ParticipantState {
    Initial,
    Prepared,
    Committed,
    Aborted,
    Failed,
}

#[derive(Clone, Debug)]
struct Coordinator {
    transactions: HashMap<String, TwoPCTransaction>,
    state: CoordinatorState,
}

#[derive(Debug)]
struct Participant {
    id: String,
    endpoint: String,
    state: ParticipantState,
    last_response: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct TwoPCTransaction {
    pub id: String,
    pub participants: Vec<String>,
    pub status: TransactionStatus,
    pub created_at: Instant,
    pub updated_at: Instant,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug)]
pub enum TransactionStatus {
    Pending,
    Preparing,
    Prepared,
    Committing,
    Committed,
    Aborting,
    Aborted,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Operation {
    pub op_type: OperationType,
    pub table: String,
    pub key: String,
    pub value: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub enum OperationType {
    Insert,
    Update,
    Delete,
}

impl TwoPhaseCommit {
    pub fn new(config: TwoPCConfig) -> Self {
        TwoPhaseCommit {
            coordinator: Arc::new(RwLock::new(Coordinator {
                transactions: HashMap::new(),
                state: CoordinatorState::Initial,
            })),
            participants: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub fn register_participant(&self, id: &str, endpoint: &str) {
        let mut participants = self.participants.write().unwrap();
        participants.insert(
            id.to_string(),
            Participant {
                id: id.to_string(),
                endpoint: endpoint.to_string(),
                state: ParticipantState::Initial,
                last_response: None,
            },
        );
    }

    pub fn begin_transaction(&self, participant_ids: Vec<String>) -> Result<String, TwoPCError> {
        let transaction_id = uuid::Uuid::new_v4().to_string();

        let transaction = TwoPCTransaction {
            id: transaction_id.clone(),
            participants: participant_ids.clone(),
            status: TransactionStatus::Pending,
            created_at: Instant::now(),
            updated_at: Instant::now(),
            operations: Vec::new(),
        };

        {
            let mut coordinator = self.coordinator.write().unwrap();
            coordinator
                .transactions
                .insert(transaction_id.clone(), transaction);
        }

        let mut participants = self.participants.write().unwrap();
        for pid in &participant_ids {
            if let Some(p) = participants.get_mut(pid) {
                p.state = ParticipantState::Initial;
            }
        }

        Ok(transaction_id)
    }

    pub fn add_operation(
        &self,
        transaction_id: &str,
        operation: Operation,
    ) -> Result<(), TwoPCError> {
        let mut coordinator = self.coordinator.write().unwrap();

        if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
            tx.operations.push(operation);
            tx.updated_at = Instant::now();
            Ok(())
        } else {
            Err(TwoPCError::TransactionNotFound)
        }
    }

    pub fn prepare(&self, transaction_id: &str) -> Result<bool, TwoPCError> {
        {
            let mut coordinator = self.coordinator.write().unwrap();

            if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                tx.status = TransactionStatus::Preparing;
                tx.updated_at = Instant::now();
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        }

        let participants = {
            let coordinator = self.coordinator.read().unwrap();
            if let Some(tx) = coordinator.transactions.get(transaction_id) {
                tx.participants.clone()
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        };

        let mut all_prepared = true;

        for participant_id in &participants {
            let prepared = self.simulate_prepare(participant_id)?;

            if !prepared {
                all_prepared = false;
                self.abort_transaction(transaction_id)?;
                return Ok(false);
            }
        }

        {
            let mut coordinator = self.coordinator.write().unwrap();
            if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                tx.status = TransactionStatus::Prepared;
                tx.updated_at = Instant::now();
            }
        }

        Ok(true)
    }

    fn simulate_prepare(&self, participant_id: &str) -> Result<bool, TwoPCError> {
        // Эмуляция сетевого вызова к участнику для фазы prepare
        // В реальной реализации здесь был бы HTTP/gRPC вызов к участнику

        let participants = self.participants.read().unwrap();
        if let Some(participant) = participants.get(participant_id) {
            // Проверяем, доступен ли участник
            if participant.state == ParticipantState::Failed {
                return Err(TwoPCError::ParticipantFailed);
            }

            // Эмулируем успешную подготовку
            // В реальной реализации здесь был бы ответ от участника
            drop(participants);

            let mut participants_write = self.participants.write().unwrap();
            if let Some(p) = participants_write.get_mut(participant_id) {
                p.state = ParticipantState::Prepared;
                p.last_response = Some(Instant::now());
            }

            Ok(true)
        } else {
            Err(TwoPCError::ParticipantFailed)
        }
    }

    fn simulate_commit(&self, participant_id: &str) -> Result<bool, TwoPCError> {
        // Эмуляция сетевого вызова к участнику для фазы commit
        // В реальной реализации здесь был бы HTTP/gRPC вызов к участнику

        let participants = self.participants.read().unwrap();
        if let Some(participant) = participants.get(participant_id) {
            // Проверяем, готов ли участник к коммиту
            if participant.state != ParticipantState::Prepared
                && participant.state != ParticipantState::Initial
            {
                return Err(TwoPCError::InvalidState);
            }

            drop(participants);

            // Эмулируем успешный коммит
            let mut participants_write = self.participants.write().unwrap();
            if let Some(p) = participants_write.get_mut(participant_id) {
                p.state = ParticipantState::Committed;
                p.last_response = Some(Instant::now());
            }

            Ok(true)
        } else {
            Err(TwoPCError::ParticipantFailed)
        }
    }

    fn simulate_abort(&self, participant_id: &str) -> Result<(), TwoPCError> {
        // Эмуляция сетевого вызова к участнику для фазы abort
        // В реальной реализации здесь был бы HTTP/gRPC вызов к участнику

        let participants = self.participants.read().unwrap();
        if let Some(participant) = participants.get(participant_id) {
            // Проверяем состояние участника
            if participant.state == ParticipantState::Committed {
                // Уже закоммичено, нельзя отменить
                return Err(TwoPCError::InvalidState);
            }

            drop(participants);

            // Эмулируем успешный аборт
            let mut participants_write = self.participants.write().unwrap();
            if let Some(p) = participants_write.get_mut(participant_id) {
                p.state = ParticipantState::Aborted;
                p.last_response = Some(Instant::now());
            }

            Ok(())
        } else {
            Err(TwoPCError::ParticipantFailed)
        }
    }

    pub fn commit(&self, transaction_id: &str) -> Result<(), TwoPCError> {
        {
            let mut coordinator = self.coordinator.write().unwrap();

            if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                tx.status = TransactionStatus::Committing;
                tx.updated_at = Instant::now();
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        }

        let participants = {
            let coordinator = self.coordinator.read().unwrap();
            if let Some(tx) = coordinator.transactions.get(transaction_id) {
                tx.participants.clone()
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        };

        let mut all_committed = true;

        for participant_id in &participants {
            let committed = self.simulate_commit(participant_id)?;

            if !committed {
                all_committed = false;
                break;
            }
        }

        if all_committed {
            {
                let mut coordinator = self.coordinator.write().unwrap();
                if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                    tx.status = TransactionStatus::Committed;
                    tx.updated_at = Instant::now();
                }
            }

            let mut participants_guard = self.participants.write().unwrap();
            let participant_ids: Vec<String> = participants_guard.keys().cloned().collect();
            for pid in participant_ids {
                if let Some(p) = participants_guard.get_mut(&pid) {
                    p.state = ParticipantState::Committed;
                }
            }

            Ok(())
        } else {
            self.abort_transaction(transaction_id)
        }
    }

    pub fn abort_transaction(&self, transaction_id: &str) -> Result<(), TwoPCError> {
        {
            let mut coordinator = self.coordinator.write().unwrap();

            if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                tx.status = TransactionStatus::Aborting;
                tx.updated_at = Instant::now();
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        }

        let participants = {
            let coordinator = self.coordinator.read().unwrap();
            if let Some(tx) = coordinator.transactions.get(transaction_id) {
                tx.participants.clone()
            } else {
                return Err(TwoPCError::TransactionNotFound);
            }
        };

        for participant_id in &participants {
            self.simulate_abort(participant_id)?;
        }

        {
            let mut coordinator = self.coordinator.write().unwrap();
            if let Some(tx) = coordinator.transactions.get_mut(transaction_id) {
                tx.status = TransactionStatus::Aborted;
                tx.updated_at = Instant::now();
            }
        }

        Ok(())
    }

    pub fn get_status(&self, transaction_id: &str) -> Option<TransactionStatus> {
        let coordinator = self.coordinator.read().unwrap();
        coordinator
            .transactions
            .get(transaction_id)
            .map(|t| t.status.clone())
    }

    pub fn get_transaction(&self, transaction_id: &str) -> Option<TwoPCTransaction> {
        let coordinator = self.coordinator.read().unwrap();
        coordinator.transactions.get(transaction_id).cloned()
    }
}

#[derive(Debug)]
pub enum TwoPCError {
    TransactionNotFound,
    Timeout,
    ParticipantFailed,
    CoordinatorFailed,
    InvalidState,
}

impl std::fmt::Display for TwoPCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TwoPCError::TransactionNotFound => write!(f, "Transaction not found"),
            TwoPCError::Timeout => write!(f, "Operation timed out"),
            TwoPCError::ParticipantFailed => write!(f, "Participant failed"),
            TwoPCError::CoordinatorFailed => write!(f, "Coordinator failed"),
            TwoPCError::InvalidState => write!(f, "Invalid transaction state"),
        }
    }
}

impl std::error::Error for TwoPCError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_pc_new() {
        let two_pc = TwoPhaseCommit::new(TwoPCConfig::default());
        two_pc.register_participant("node1", "http://localhost:8081");
        two_pc.register_participant("node2", "http://localhost:8082");
    }

    #[test]
    fn test_begin_transaction() {
        let two_pc = TwoPhaseCommit::new(TwoPCConfig::default());
        two_pc.register_participant("node1", "http://localhost:8081");

        let tx_id = two_pc.begin_transaction(vec!["node1".to_string()]).unwrap();
        assert!(!tx_id.is_empty());
    }

    #[test]
    fn test_prepare_commit() {
        let two_pc = TwoPhaseCommit::new(TwoPCConfig::default());
        two_pc.register_participant("node1", "http://localhost:8081");

        let tx_id = two_pc.begin_transaction(vec!["node1".to_string()]).unwrap();

        let op = Operation {
            op_type: OperationType::Insert,
            table: "users".to_string(),
            key: "1".to_string(),
            value: Some(b"data".to_vec()),
        };

        two_pc.add_operation(&tx_id, op).unwrap();

        let prepared = two_pc.prepare(&tx_id).unwrap();
        assert!(prepared);

        two_pc.commit(&tx_id).unwrap();

        let status = two_pc.get_status(&tx_id).unwrap();
        assert!(matches!(status, TransactionStatus::Committed));
    }

    #[test]
    fn test_abort() {
        let two_pc = TwoPhaseCommit::new(TwoPCConfig::default());
        two_pc.register_participant("node1", "http://localhost:8081");

        let tx_id = two_pc.begin_transaction(vec!["node1".to_string()]).unwrap();

        two_pc.abort_transaction(&tx_id).unwrap();

        let status = two_pc.get_status(&tx_id).unwrap();
        assert!(matches!(status, TransactionStatus::Aborted));
    }
}
