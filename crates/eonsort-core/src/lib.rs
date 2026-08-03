pub mod copy;
pub mod dateparse;
pub mod error;
pub mod model;
pub mod plan;
pub mod providers;
pub mod scan;
pub mod verify;

pub use error::{Error, Result};
pub use model::{
    destination_for, duplicate_variant, validate_folder_pattern, PlanEntry, PlanHeader, PlanRecord,
    SkippedEntry, DEFAULT_FOLDER_PATTERN, PLAN_VERSION,
};
pub use plan::{default_plan_name, read_plan, Plan, PlanWriter};
pub use providers::{DetectOptions, Provider, Strategy};
pub use scan::{scan, ScanOptions, ScanPhase, ScanProgress};
pub use verify::{verify, VerifyOptions, VerifyProgress, VerifyReport};
