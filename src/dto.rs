use super::*;
use anyhow::*;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::process::*;
use std::ops::Deref;
use std::path::*;
use std::sync::Arc;
use std::fs::File;
use derivative::*;

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

// impl Benchmark {
//     pub fn file(&self) -> &OsStr {
//         &self.file.file_name().unwrap()
//     }
// }

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
    pub fn annotated(&self) -> &A {
        &self.0
    }
    pub fn annotation(&self) -> &B {
        &self.1
    }
}

/// Represents a benchmark configuration that can be run. It contains a solver, a benchmark and some metadata.
#[derive(Serialize, Deserialize, Derivative)]
#[derivative( Clone(bound=""), Debug(bound=""), Hash(bound=""), Ord(bound=""), PartialOrd(bound=""), Eq(bound=""), PartialEq(bound="") )]
pub struct BenchRunConf<P> 
    where P: Postprocessor + ?Sized
{
    // #[serde(bound(deserialize = "JobConfig<P>: DeserializeOwned", serialize = "JobConfig<P>: Serialize"))]
    #[serde(bound(serialize = "P: Postprocessor", deserialize = "P: Postprocessor"))]
    pub(crate) job: Arc<JobConfig<P>>,
    pub(crate) benchmark: Arc<Annotated<Benchmark, P::BAnnot>>,
    pub(crate) solver: Arc<P::Solver>,
}
impl<P> BenchRunConf<P>
    where P: Postprocessor + ?Sized
{
    pub fn job(&self) -> &JobConfig<P> {
        &self.job
    }
    pub fn benchmark(&self) -> &Annotated<Benchmark, P::BAnnot> {
        self.benchmark.as_ref()
    }
    pub fn solver(&self) -> &P::Solver {
        self.solver.as_ref()
    }

    pub fn to_command<'a>(&self) -> Command {
        self.solver().to_command(self.benchmark(), &self.job.timeout)
    }
    //
    // pub fn args<'a>(&'a self) -> impl IntoIterator<Item = impl AsRef<OsStr> + fmt::Display + 'a> + 'a {
    //     use std::iter::once;
    //     once(Args::PathBuf(&self.benchmark().file))
    //         .chain(once(Args::TimeOut(format!("{}", self.job.timeout.as_secs()))))
    // }

}

impl<P> fmt::Display for BenchRunConf<P> 
    where P: Postprocessor
{
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        self.solver().show_command(self.benchmark(), &self.job.timeout).fmt(w)
        // write!(w, "{}", self.command().display())?;
        // for arg in self.args() {
        //     write!(w, " {}", arg)?;
        // }
        // Ok(())
    }
}

#[derive(Serialize, Deserialize, Derivative)]
#[derivative( Clone(bound=""), Debug(bound=""), Hash(bound=""), Ord(bound=""), PartialOrd(bound=""), Eq(bound=""), PartialEq(bound="") )]
pub struct JobConfig<P> 
    where P: Postprocessor  + ?Sized

    // where P: Postprocessor + Clone + std::fmt::Debug+ std::hash::Hash+ Ord+ PartialOrd + Eq + PartialEq + Serialize + DeserializeOwned+ ?Sized
{
    pub solvers: Vec<Arc<P::Solver>>,
    pub benchmarks: Vec<Arc<Annotated<Benchmark, P::BAnnot>>>,
    pub timeout: Duration,
}


impl<P> JobConfig<P> 
    where P: Postprocessor + ?Sized
{
    // pub fn solvers(&self) -> &[impl AsRef<P::Solver>] {
    pub fn solvers(&self) -> &[Arc<P::Solver>] {
        &self.solvers
    }
    pub fn benchmarks(&self) -> &[Arc<Annotated<Benchmark, P::BAnnot>>] {
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

#[derive(Serialize, Deserialize, Derivative)]
#[derivative( Clone(bound=""), Debug(bound=""), Hash(bound=""), Ord(bound=""), PartialOrd(bound=""), Eq(bound=""), PartialEq(bound="") )]
pub struct BenchRunResult<P> 
    where P: Postprocessor + ?Sized
{
    #[serde(bound(serialize = "P: Postprocessor", deserialize = "P: Postprocessor"))]
    pub(crate) run: BenchRunConf<P>,
    pub(crate) status: BenchmarkStatus,
    pub(crate) time: Duration,
    pub(crate) exit_status: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl<P> BenchRunResult<P> 
    where P: Postprocessor + ?Sized
{
    pub fn stdout<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stdout))
    }
    pub fn stderr<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stderr))
    }
}
