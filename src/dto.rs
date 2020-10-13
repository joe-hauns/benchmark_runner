use super::*;
use anyhow::*;
use std::fmt;
use std::io;
use std::process::*;
use std::sync::Arc;
use derivative::*;

impl<P> fmt::Display for BenchRunConf<P> 
    where P: Benchmarker + ?Sized
{
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        write!(w, "{} {}", self.solver.id(), self.benchmark.id())
    }
}

/// Represents a benchmark configuration that can be run. It contains a solver, a benchmark and some metadata.
#[derive(Serialize, Deserialize, Derivative)]
#[derivative( Clone(bound=""), Debug(bound=""), Hash(bound=""), Ord(bound=""), PartialOrd(bound=""), Eq(bound=""), PartialEq(bound="") )]
pub struct BenchRunConf<P> 
    where P: Benchmarker + ?Sized
{
    pub timeout: Duration,
    #[serde(bound(serialize = "P: Benchmarker", deserialize = "P: Benchmarker"))]
    pub benchmark: Arc<P::Benchmark>,
    #[serde(bound(serialize = "P: Benchmarker", deserialize = "P: Benchmarker"))]
    pub solver: Arc<P::Solver>,
}



impl<P> BenchRunConf<P>
    where P: Benchmarker + ?Sized
{
    pub fn benchmark(&self) -> &P::Benchmark {
        self.benchmark.as_ref()
    }
    pub fn solver(&self) -> &P::Solver {
        self.solver.as_ref()
    }

    pub fn to_command<'a>(&self) -> Command {
        self.solver().to_command(&self.benchmark, &self.timeout)
    }

    pub fn display_command(&self) -> impl fmt::Display {
        self.solver().show_command(&self.benchmark, &self.timeout)
    }
}

#[derive(Serialize, Deserialize, Derivative)]
#[derivative( Clone(bound=""), Debug(bound=""), Hash(bound=""), Ord(bound=""), PartialOrd(bound=""), Eq(bound=""), PartialEq(bound="") )]
pub struct JobConfig<P> 
    where P: Benchmarker  + ?Sized

{
    pub solvers: Vec<Arc<P::Solver>>,
    pub benchmarks: Vec<Arc<P::Benchmark>>,
    pub timeout: Duration,
}


impl<P> JobConfig<P> 
    where P: Benchmarker + ?Sized
{
    // pub fn solvers(&self) -> &[impl AsRef<P::Solver>] {
    pub fn solvers(&self) -> &[Arc<P::Solver>] {
        &self.solvers
    }
    pub fn benchmarks(&self) -> &[Arc<P::Benchmark>] {
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
    where P: Benchmarker + ?Sized
{
    #[serde(bound(serialize = "P: Benchmarker", deserialize = "P: Benchmarker"))]
    #[derivative(Debug(format_with="std::fmt::Display::fmt"))]
    pub(crate) run: BenchRunConf<P>,
    pub(crate) status: BenchmarkStatus,
    pub(crate) time: Duration,
    pub(crate) exit_status: Option<i32>,
    #[derivative(Debug="ignore")]
    pub(crate) stdout: Vec<u8>,
    #[derivative(Debug="ignore")]
    pub(crate) stderr: Vec<u8>,
}

use std::fmt::Debug;

// #[derive(Serialize, Deserialize, Derivative)]
// #[derivative( Clone(bound="P: Benchmarker"), Debug(bound="P: Benchmarker"), Hash(bound="P: Benchmarker"), Ord(bound="P: Benchmarker"), PartialOrd(bound="P: Benchmarker"), Eq(bound="P: Benchmarker"), PartialEq(bound="P: Benchmarker") )]
pub struct MappedBenchRunResult<P> 
    where P: Benchmarker + ?Sized
{
    // #[serde(bound(serialize = "P: Benchmarker", deserialize = "P: Benchmarker"))]
    pub raw: BenchRunResult<P>,
    // #[serde(bound(serialize = "P: Benchmarker", deserialize = "P: Benchmarker"))]
    pub mapped: P::Mapped,
}


impl<P> BenchRunResult<P> 
    where P: Benchmarker + ?Sized
{
    pub fn run(&self) -> &BenchRunConf<P> { &self.run }
    pub fn solver(&self) -> &P::Solver { &self.run().solver() }
    pub fn benchmark(&self) -> &P::Benchmark { &self.run().benchmark() }
    pub fn stdout<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stdout))
    }
    pub fn stderr<'a>(&'a self) -> Result<impl io::Read + 'a> {
        Ok(io::Cursor::new(&self.stderr))
    }
    pub fn status(&self) -> Option<i32> {self.exit_status}
    pub fn time(&self) -> Duration {self.time}
    pub fn display_command(&self) -> impl fmt::Display { self.run.display_command() }
}
