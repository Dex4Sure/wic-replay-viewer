use std::collections::HashMap;
use std::panic;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug)]
pub(crate) struct FileOperationGuard(pub(crate) Arc<AtomicBool>);

impl Drop for FileOperationGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) fn begin_file_operation(
    busy: &Arc<AtomicBool>,
    conflict: &str,
) -> Result<FileOperationGuard, String> {
    busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| conflict.to_owned())?;
    Ok(FileOperationGuard(Arc::clone(busy)))
}

pub(crate) struct DetailLoadCoordinator<T> {
    in_flight: Mutex<HashMap<PathBuf, Arc<PendingDetailLoad<T>>>>,
}

impl<T> Default for DetailLoadCoordinator<T> {
    fn default() -> Self {
        Self {
            in_flight: Mutex::new(HashMap::new()),
        }
    }
}

struct PendingDetailLoad<T> {
    result: Mutex<Option<Result<T, String>>>,
    ready: Condvar,
}

impl<T> Default for PendingDetailLoad<T> {
    fn default() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }
}

impl<T: Clone> DetailLoadCoordinator<T> {
    pub(crate) fn load<F>(&self, path: PathBuf, loader: F) -> Result<T, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        let (pending, leader) = {
            let mut in_flight = self
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match in_flight.get(&path) {
                Some(pending) => (Arc::clone(pending), false),
                None => {
                    let pending = Arc::new(PendingDetailLoad::default());
                    in_flight.insert(path.clone(), Arc::clone(&pending));
                    (pending, true)
                }
            }
        };

        if leader {
            let result = panic::catch_unwind(panic::AssertUnwindSafe(loader))
                .unwrap_or_else(|_| Err("Replay detail loader panicked".to_owned()));
            {
                let mut stored = pending
                    .result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *stored = Some(result.clone());
                pending.ready.notify_all();
            }
            if let Ok(mut in_flight) = self.in_flight.lock()
                && in_flight
                    .get(&path)
                    .is_some_and(|current| Arc::ptr_eq(current, &pending))
            {
                in_flight.remove(&path);
            }
            return result;
        }

        let mut stored = pending
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while stored.is_none() {
            stored = pending
                .ready
                .wait(stored)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        stored
            .as_ref()
            .cloned()
            .expect("detail result is present after waiting")
    }
}

pub(crate) fn canonical_directories(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    let mut directories = Vec::new();
    for path in paths {
        let candidate = PathBuf::from(path);
        let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
            format!("Cannot open replay folder {}: {error}", candidate.display())
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "Dropped path is not a folder: {}",
                canonical.display()
            ));
        }
        if !directories.contains(&canonical) {
            directories.push(canonical);
        }
    }
    Ok(directories)
}

pub(crate) fn paths_as_strings(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn file_operation_guard_excludes_and_releases_on_success_error_and_panic() {
        let busy = Arc::new(AtomicBool::new(false));
        {
            let _guard = begin_file_operation(&busy, "busy").expect("first guard");
            assert_eq!(begin_file_operation(&busy, "busy").unwrap_err(), "busy");
        }
        assert!(!busy.load(Ordering::Acquire));

        let result: Result<(), String> = (|| {
            let _guard = begin_file_operation(&busy, "busy")?;
            Err("operation failed".to_owned())
        })();
        assert_eq!(result.unwrap_err(), "operation failed");
        assert!(!busy.load(Ordering::Acquire));

        let _ = panic::catch_unwind({
            let busy = Arc::clone(&busy);
            move || {
                let _guard = begin_file_operation(&busy, "busy").expect("panic guard");
                panic!("operation panicked");
            }
        });
        assert!(!busy.load(Ordering::Acquire));
    }

    #[test]
    fn canonical_directory_validation_deduplicates_and_preserves_order() {
        let directory = tempdir().expect("temp dir");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::create_dir_all(&first).expect("first directory");
        std::fs::create_dir_all(&second).expect("second directory");
        let file = directory.path().join("replay.wicdemo");
        std::fs::write(&file, b"fixture").expect("file");

        let paths = canonical_directories(vec![
            first.to_string_lossy().into_owned(),
            first.join(".").to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ])
        .expect("directories");
        assert_eq!(
            paths,
            vec![
                first.canonicalize().unwrap(),
                second.canonicalize().unwrap()
            ]
        );
        assert_eq!(paths_as_strings(paths).len(), 2);
        assert!(
            canonical_directories(vec![file.to_string_lossy().into_owned()])
                .unwrap_err()
                .contains("not a folder")
        );
        assert!(
            canonical_directories(vec![
                directory
                    .path()
                    .join("missing")
                    .to_string_lossy()
                    .into_owned()
            ])
            .unwrap_err()
            .contains("Cannot open replay folder")
        );
    }

    #[test]
    fn detail_loads_coalesce_success_failure_and_panic_then_retry() {
        const REQUESTS: usize = 8;
        let coordinator = Arc::new(DetailLoadCoordinator::<usize>::default());
        let barrier = Arc::new(Barrier::new(REQUESTS));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..REQUESTS {
            let coordinator = Arc::clone(&coordinator);
            let barrier = Arc::clone(&barrier);
            let calls = Arc::clone(&calls);
            threads.push(thread::spawn(move || {
                barrier.wait();
                coordinator.load(PathBuf::from("shared"), || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(25));
                    Ok(18)
                })
            }));
        }
        for handle in threads {
            assert_eq!(handle.join().unwrap().unwrap(), 18);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        assert_eq!(
            coordinator
                .load(PathBuf::from("shared"), || Err("failure".to_owned()))
                .unwrap_err(),
            "failure"
        );
        assert_eq!(
            coordinator
                .load(PathBuf::from("panic"), || -> Result<usize, String> {
                    panic!("boom")
                })
                .unwrap_err(),
            "Replay detail loader panicked"
        );
        assert_eq!(
            coordinator.load(PathBuf::from("panic"), || Ok(7)).unwrap(),
            7
        );
    }

    #[test]
    fn distinct_detail_paths_do_not_coalesce() {
        let coordinator = DetailLoadCoordinator::<usize>::default();
        assert_eq!(coordinator.load(PathBuf::from("a"), || Ok(1)).unwrap(), 1);
        assert_eq!(coordinator.load(PathBuf::from("b"), || Ok(2)).unwrap(), 2);
    }
}
