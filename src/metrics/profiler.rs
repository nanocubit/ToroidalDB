use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct QueryProfile {
    pub query_id: String,
    pub query: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub duration: Option<Duration>,
    pub phases: Vec<PhaseProfile>,
    pub metrics: QueryMetrics,
    pub status: QueryStatus,
}

#[derive(Clone)]
pub struct PhaseProfile {
    pub name: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub duration: Option<Duration>,
}

#[derive(Clone, Default)]
pub struct QueryMetrics {
    pub cpu_time_ns: u64,
    pub memory_used_bytes: usize,
    pub rows_scanned: u64,
    pub rows_returned: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub disk_reads: u64,
    pub disk_writes: u64,
    pub network_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryStatus {
    Running,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

pub struct QueryProfiler {
    profiles: Arc<RwLock<VecDeque<QueryProfile>>>>,
    active_profiles: Arc<RwLock<HashMap<String, QueryProfile>>>,
    max_stored_profiles: usize,
}

impl QueryProfiler {
    pub fn new(max_stored_profiles: usize) -> Self {
        QueryProfiler {
            profiles: Arc::new(RwLock::new(VecDeque::with_capacity(max_stored_profiles))),
            active_profiles: Arc::new(RwLock::new(HashMap::new())),
            max_stored_profiles,
        }
    }

    pub fn start_query(&self, query_id: &str, query: &str) {
        let profile = QueryProfile {
            query_id: query_id.to_string(),
            query: query.to_string(),
            start_time: Instant::now(),
            end_time: None,
            duration: None,
            phases: Vec::new(),
            metrics: QueryMetrics::default(),
            status: QueryStatus::Running,
        };

        let mut active = self.active_profiles.write().unwrap();
        active.insert(query_id.to_string(), profile);
    }

    pub fn end_query(&self, query_id: &str, status: QueryStatus) {
        let mut active = self.active_profiles.write().unwrap();
        
        if let Some(mut profile) = active.remove(query_id) {
            profile.end_time = Some(Instant::now());
            profile.duration = Some(profile.end_time.unwrap().duration_since(profile.start_time));
            profile.status = status;

            let mut profiles = self.profiles.write().unwrap();
            
            if profiles.len() >= self.max_stored_profiles {
                profiles.pop_front();
            }
            
            profiles.push_back(profile);
        }
    }

    pub fn start_phase(&self, query_id: &str, phase_name: &str) {
        let mut active = self.active_profiles.write().unwrap();
        
        if let Some(profile) = active.get_mut(query_id) {
            profile.phases.push(PhaseProfile {
                name: phase_name.to_string(),
                start_time: Instant::now(),
                end_time: None,
                duration: None,
            });
        }
    }

    pub fn end_phase(&self, query_id: &str, phase_name: &str) {
        let mut active = self.active_profiles.write().unwrap();
        
        if let Some(profile) = active.get_mut(query_id) {
            if let Some(phase) = profile.phases.iter_mut().find(|p| p.name == phase_name) {
                phase.end_time = Some(Instant::now());
                phase.duration = Some(phase.end_time.unwrap().duration_since(phase.start_time));
            }
        }
    }

    pub fn update_metrics(&self, query_id: &str, updater: impl FnOnce(&mut QueryMetrics)) {
        let mut active = self.active_profiles.write().unwrap();
        
        if let Some(profile) = active.get_mut(query_id) {
            updater(&mut profile.metrics);
        }
    }

    pub fn get_profile(&self, query_id: &str) -> Option<QueryProfile> {
        let active = self.active_profiles.read().unwrap();
        active.get(query_id).cloned()
    }

    pub fn get_recent_profiles(&self, limit: usize) -> Vec<QueryProfile> {
        let profiles = self.profiles.read().unwrap();
        profiles.iter().rev().take(limit).cloned().collect()
    }

    pub fn get_slow_queries(&self, threshold_ms: u64) -> Vec<QueryProfile> {
        let profiles = self.profiles.read().unwrap();
        let threshold = Duration::from_millis(threshold_ms);
        
        profiles
            .iter()
            .filter(|p| {
                if let Some(d) = p.duration {
                    d > threshold
                } else {
                    false
                }
            })
            .cloned()
            .collect()
    }

    pub fn get_failed_queries(&self, limit: usize) -> Vec<QueryProfile> {
        let profiles = self.profiles.read().unwrap();
        profiles
            .iter()
            .filter(|p| matches!(p.status, QueryStatus::Failed))
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get_statistics(&self) -> ProfilerStatistics {
        let profiles = self.profiles.read().unwrap();
        
        let mut total_queries = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut total_duration = Duration::ZERO;
        let mut avg_duration = Duration::ZERO;
        
        if !profiles.is_empty() {
            total_queries = profiles.len();
            
            for p in profiles.iter() {
                if let Some(d) = p.duration {
                    total_duration += d;
                }
                
                match p.status {
                    QueryStatus::Completed => completed += 1,
                    QueryStatus::Failed => failed += 1,
                    _ => {}
                }
            }
            
            avg_duration = total_duration / total_queries as u32;
        }

        ProfilerStatistics {
            total_queries,
            completed_queries: completed,
            failed_queries: failed,
            total_duration,
            average_duration: avg_duration,
        }
    }

    pub fn clear(&self) {
        let mut profiles = self.profiles.write().unwrap();
        profiles.clear();
        
        let mut active = self.active_profiles.write().unwrap();
        active.clear();
    }
}

#[derive(Clone, Debug)]
pub struct ProfilerStatistics {
    pub total_queries: usize,
    pub completed_queries: usize,
    pub failed_queries: usize,
    pub total_duration: Duration,
    pub average_duration: Duration,
}

impl Default for QueryProfiler {
    fn default() -> Self {
        Self::new(1000)
    }
}

pub struct QueryProfilerMiddleware {
    profiler: Arc<QueryProfiler>,
}

impl QueryProfilerMiddleware {
    pub fn new(profiler: Arc<QueryProfiler>) -> Self {
        QueryProfilerMiddleware { profiler }
    }

    pub fn profile_query<F, R>(&self, query_id: &str, query: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.profiler.start_query(query_id, query);
        
        let result = f();
        
        self.profiler.end_query(query_id, QueryStatus::Completed);
        
        result
    }

    pub fn get_profiler(&self) -> Arc<QueryProfiler> {
        self.profiler.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_end_query() {
        let profiler = QueryProfiler::new(100);
        
        profiler.start_query("q1", "SELECT * FROM users");
        profiler.end_query("q1", QueryStatus::Completed);
        
        let profiles = profiler.get_recent_profiles(10);
        assert_eq!(profiles.len(), 1);
    }

    #[test]
    fn test_phase_tracking() {
        let profiler = QueryProfiler::new(100);
        
        profiler.start_query("q1", "SELECT * FROM users");
        profiler.start_phase("q1", "parse");
        profiler.end_phase("q1", "parse");
        profiler.end_query("q1", QueryStatus::Completed);
        
        let profile = profiler.get_profile("q1").unwrap();
        assert_eq!(profile.phases.len(), 1);
    }

    #[test]
    fn test_slow_queries() {
        let profiler = QueryProfiler::new(100);
        
        profiler.start_query("q1", "SELECT * FROM users");
        std::thread::sleep(Duration::from_millis(50));
        profiler.end_query("q1", QueryStatus::Completed);
        
        let slow = profiler.get_slow_queries(10);
        assert!(slow.len() >= 1);
    }

    #[test]
    fn test_statistics() {
        let profiler = QueryProfiler::new(100);
        
        profiler.start_query("q1", "SELECT 1");
        profiler.end_query("q1", QueryStatus::Completed);
        
        let stats = profiler.get_statistics();
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.completed_queries, 1);
    }

    #[test]
    fn test_middleware() {
        let profiler = Arc::new(QueryProfiler::new(100));
        let middleware = QueryProfilerMiddleware::new(profiler.clone());
        
        let result = middleware.profile_query("q1", "SELECT 1", || {
            std::thread::sleep(Duration::from_millis(10));
            42
        });
        
        assert_eq!(result, 42);
        
        let stats = profiler.get_statistics();
        assert_eq!(stats.total_queries, 1);
    }
}
