use super::*;
use anyhow::*;
use std::convert::TryFrom;
use std::ffi::*;
use std::fmt;
use std::path::*;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Solver {
    pub(crate) file: PathBuf,
}

impl fmt::Display for Solver {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        self.file.display().fmt(w)
    }
}

impl Solver {
    pub fn file(&self) -> &OsStr {
        &self.file.file_name().unwrap()
    }
}

impl TryFrom<PathBuf> for Solver {
    type Error = anyhow::Error;
    fn try_from(file: PathBuf) -> Result<Self> {
        Ok(Solver { file })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Benchmark {
    pub(crate) file: PathBuf,
}

impl fmt::Display for Benchmark {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        self.file.display().fmt(w)
    }
}

impl Benchmark {
    pub fn file(&self) -> &OsStr {
        &self.file.file_name().unwrap()
    }
}

impl TryFrom<PathBuf> for Benchmark {
    type Error = anyhow::Error;
    fn try_from(file: PathBuf) -> Result<Self> {
        Ok(Benchmark { file })
    }
}

/// Represents a benchmark configuration that can be run. It contains a solver, a benchmark and some metadata.
#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchRunConf {
    pub(crate) job: Arc<JobConfig>,
    pub(crate) benchmark: Arc<Benchmark>,
    pub(crate) solver: Arc<Solver>,
}

impl fmt::Display for BenchRunConf {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        write!(w, "solver: {} benchmark: {}", self.solver, self.benchmark)
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobConfig {
    pub(crate) solvers: Vec<Arc<Solver>>,
    pub(crate) benchmarks: Vec<Arc<Benchmark>>,
    pub(crate) timeout: Duration,
}

impl JobConfig {
    pub fn solvers(&self) -> &[impl AsRef<Solver>] {&self.solvers}
    pub fn benchmarks(&self) -> &[impl AsRef<Benchmark>] {&self.benchmarks}
    pub fn timeout(&self) -> Duration {self.timeout}
}


#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub enum BenchmarkStatus {
    Success,
    Timeout,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchRunResult {
    pub(crate) run: BenchRunConf,
    pub(crate) status: BenchmarkStatus,
    pub(crate) time: Duration,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) exit_status: Option<i32>,
}
