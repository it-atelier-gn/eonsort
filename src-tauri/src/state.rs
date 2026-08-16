use eonsort_core::copy::Outcome;
use eonsort_core::{Overrides, Plan};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Session {
    pub plan_path: Option<PathBuf>,
    pub plan: Option<Plan>,
    pub journal: HashMap<PathBuf, Outcome>,
    pub overrides: Overrides,
    pub rotations: eonsort_core::overrides::Rotations,
    pub excluded: std::collections::HashSet<PathBuf>,
    pub busy: Option<String>,
}

pub struct AppState {
    pub session: Mutex<Session>,
    pub cancel: Arc<AtomicBool>,
    pub upright_cancel: Arc<AtomicBool>,
    pub fetching_upright: Mutex<bool>,
    pub tag_cancel: Arc<AtomicBool>,
    pub fetching_tags: Mutex<bool>,
    pub tagging: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Mutex::new(Session::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            upright_cancel: Arc::new(AtomicBool::new(false)),
            fetching_upright: Mutex::new(false),
            tag_cancel: Arc::new(AtomicBool::new(false)),
            fetching_tags: Mutex::new(false),
            tagging: Mutex::new(false),
        }
    }
}

impl AppState {
    pub fn begin(&self, job: &str) -> Result<(), String> {
        let mut session = self.session.lock().unwrap();
        if let Some(running) = &session.busy {
            return Err(format!("{running} is still running"));
        }
        session.busy = Some(job.to_string());
        self.cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn finish(&self) {
        self.session.lock().unwrap().busy = None;
    }
}
