use std::collections::VecDeque;
use std::sync::Mutex;

pub struct ObjectPool<T> {
    pool: Mutex<VecDeque<T>>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
    max_size: usize,
}

impl<T: Clone + Send + 'static> ObjectPool<T> {
    pub fn new<F>(factory: F, max_size: usize) -> ObjectPool<T>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        ObjectPool {
            pool: Mutex::new(VecDeque::with_capacity(max_size)),
            factory: Box::new(factory),
            max_size,
        }
    }

    pub fn acquire(&self) -> T {
        let mut pool = self.pool.lock().unwrap();

        if let Some(obj) = pool.pop_front() {
            return obj;
        }

        (self.factory)()
    }

    pub fn release(&self, obj: T) {
        let mut pool = self.pool.lock().unwrap();

        if pool.len() < self.max_size {
            pool.push_back(obj);
        }
    }

    pub fn with<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        let obj = self.acquire();
        let result = f(&obj);
        self.release(obj);
        result
    }

    pub fn size(&self) -> usize {
        self.pool.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.pool.lock().unwrap().clear();
    }

    pub fn prefill(&self, count: usize) {
        let mut pool = self.pool.lock().unwrap();
        for _ in 0..count.min(self.max_size) {
            pool.push_back((self.factory)());
        }
    }
}

pub struct PoolConfig {
    pub initial_size: usize,
    pub max_size: usize,
    pub min_size: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            initial_size: 10,
            max_size: 1000,
            min_size: 2,
        }
    }
}

impl PoolConfig {
    pub fn with_max(max_size: usize) -> Self {
        PoolConfig {
            max_size,
            ..Default::default()
        }
    }

    pub fn with_initial(initial_size: usize, max_size: usize) -> Self {
        PoolConfig {
            initial_size,
            max_size,
            min_size: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_pool_basic() {
        let pool = ObjectPool::new(|| vec![0, 0, 0], 10);

        let v1 = pool.acquire();
        assert_eq!(v1.len(), 3);

        pool.release(v1);

        let v2 = pool.acquire();
        assert_eq!(v2.len(), 3);
    }

    #[test]
    fn test_object_pool_with() {
        let pool = ObjectPool::new(|| 42, 10);

        let result = pool.with(|v| v * 2);
        assert_eq!(result, 84);
    }

    #[test]
    fn test_object_pool_size() {
        let pool = ObjectPool::new(|| 0, 10);

        assert_eq!(pool.size(), 0);

        let v = pool.acquire();
        pool.release(v);

        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn test_object_pool_prefill() {
        let pool = ObjectPool::new(|| 0, 10);

        pool.prefill(5);
        assert_eq!(pool.size(), 5);
    }

    #[test]
    fn test_object_pool_max_size() {
        let pool = ObjectPool::new(|| 0, 2);

        let v1 = pool.acquire();
        let v2 = pool.acquire();
        let v3 = pool.acquire();

        pool.release(v1);
        pool.release(v2);
        pool.release(v3);

        assert_eq!(pool.size(), 2);
    }

    #[test]
    fn test_pool_config() {
        let config = PoolConfig::with_initial(5, 100);
        assert_eq!(config.initial_size, 5);
        assert_eq!(config.max_size, 100);
    }
}
