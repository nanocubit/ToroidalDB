use rayon::prelude::*;

pub struct ParallelQueryExecutor {
    max_workers: usize,
    chunk_size: usize,
}

impl ParallelQueryExecutor {
    pub fn new(max_workers: usize, chunk_size: usize) -> Self {
        ParallelQueryExecutor {
            max_workers,
            chunk_size,
        }
    }

    pub fn execute_parallel<T, R, F>(&self, data: &[T], func: F) -> Vec<R>
    where
        T: Send + Sync,
        R: Send,
        F: Fn(&T) -> R + Send + Sync + Clone,
    {
        data.par_iter()
            .chunks(self.chunk_size)
            .flat_map(|chunk| chunk.iter().map(|item| func(item)).collect::<Vec<_>>())
            .collect()
    }

    pub fn execute_map_reduce<T, R, M, Rm>(&self, data: &[T], map: M, reduce: Rm) -> R
    where
        T: Send + Sync,
        R: Send + Default,
        M: Fn(&T) -> R + Send + Sync + Clone,
        Rm: Fn(R, R) -> R + Send + Sync + Copy + Clone,
    {
        data.par_iter().map(map).reduce(|| R::default(), reduce)
    }
}

pub struct ParallelSorter<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: Clone + Copy + Send + Sync + Ord> Default for ParallelSorter<T> {
    fn default() -> Self {
        Self::create()
    }
}

impl<T: Clone + Copy + Send + Sync + Ord> ParallelSorter<T> {
    pub fn create() -> Self {
        ParallelSorter {
            _marker: std::marker::PhantomData,
        }
    }

    pub fn new() -> Self {
        ParallelSorter {
            _marker: std::marker::PhantomData,
        }
    }

    pub fn sort(&self, data: &mut [T]) {
        let sorted: Vec<T> = data.par_iter().cloned().collect();
        let mut sorted = sorted;
        sorted.sort();
        data.copy_from_slice(&sorted);
    }
}
