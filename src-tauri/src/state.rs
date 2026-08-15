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
    pub model_cancel: Arc<AtomicBool>,
    pub depth_cancel: Arc<AtomicBool>,
    pub upright_cancel: Arc<AtomicBool>,
    pub diffuse_cancel: Arc<AtomicBool>,
    pub downloading: Mutex<Option<String>>,
    pub fetching_depth: Mutex<bool>,
    pub fetching_upright: Mutex<bool>,
    pub fetching_diffuse: Mutex<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Mutex::new(Session::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            model_cancel: Arc::new(AtomicBool::new(false)),
            depth_cancel: Arc::new(AtomicBool::new(false)),
            upright_cancel: Arc::new(AtomicBool::new(false)),
            diffuse_cancel: Arc::new(AtomicBool::new(false)),
            downloading: Mutex::new(None),
            fetching_depth: Mutex::new(false),
            fetching_upright: Mutex::new(false),
            fetching_diffuse: Mutex::new(false),
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
