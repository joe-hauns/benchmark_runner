use super::*;
use anyhow::*;
use std::convert::TryFrom;
use std::ffi::*;
use std::fmt;
use std::io;
use std::ops::Deref;
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

// impl TryFrom<PathBuf> for Solver {
//     type Error = anyhow::Error;
//     fn try_from(file: PathBuf) -> Result<Self> {
//         Ok(Solver { file })
//     }
// }

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Benchmark {
    pub(crate) file: PathBuf,
}

impl Benchmark {
    pub fn reader(&self) -> Result<impl io::Read> {
        File::open(&self.file)
            .with_context(|| format!("failed to read benchmark {}", self.file.display()))
    }
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

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Annotated<A, B>(pub(crate) A, pub(crate) B);

impl<A, B> Deref for Annotated<A, B> {
    type Target = A;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A, B> Annotated<A, B> {
    pub fn annotation(&self) -> &B {
        &self.1
    }
}

/// Represents a benchmark configuration that can be run. It contains a solver, a benchmark and some metadata.
#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchRunConf<A> {
    pub(crate) job: Arc<JobConfig<A>>,
    pub(crate) benchmark: Arc<Annotated<Benchmark, A>>,
    pub(crate) solver: Arc<Solver>,
}
impl<A> BenchRunConf<A> {
    pub fn job(&self) -> &JobConfig<A> {
        &self.job
    }
    pub fn benchmark(&self) -> &Annotated<Benchmark, A> {
        &self.benchmark
    }
    pub fn solver(&self) -> &Solver {
        &self.solver
    }
}

impl<A> fmt::Display for BenchRunConf<A> {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        write!(w, "solver: {} benchmark: {}", self.solver, self.benchmark.0)
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobConfig<A> {
    pub(crate) solvers: Vec<Arc<Solver>>,
    pub(crate) benchmarks: Vec<Arc<Annotated<Benchmark, A>>>,
    pub(crate) timeout: Duration,
}

impl<A> JobConfig<A> {
    pub fn solvers(&self) -> &[impl AsRef<Solver>] {
        &self.solvers
    }
    pub fn benchmarks(&self) -> &[impl AsRef<Annotated<Benchmark, A>>] {
        &self.benchmarks
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub enum BenchmarkStatus {
    Success,
    Timeout,
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchRunResult<A> {
    pub(crate) run: BenchRunConf<A>,
    pub(crate) status: BenchmarkStatus,
    pub(crate) time: Duration,
    pub(crate) exit_status: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl<A> BenchRunResult<A> {
    pub fn stdout<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stdout))
    }
    pub fn stderr<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stderr))
    }
}