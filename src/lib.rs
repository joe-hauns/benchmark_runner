mod dao;
mod dto;
mod ui;
mod service;

pub use ui::*;
pub use dto::*;
pub use dao::DaoConfig;
use service::*;

use anyhow::Result;
use anyhow::*;
use crossbeam_channel::*;
use dao::*;
use itertools::*;
use rayon::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Arguments as FormatArgs;
use std::fs::*;
use std::io;
use std::path::*;
use std::sync::*;
use std::time::*;
use clap::*;
use thiserror::Error as ThisError;
use wait_timeout::ChildExt;
use std::convert::*;

#[macro_export]
macro_rules! log_err {
    ($e:expr $(, $fmt:tt)*) => {{ match $e {
        Err(e) => {
            eprint!($($fmt),*);
            eprintln!(": {}", e);
            Err(e)
        },
        Ok(x) => Ok(x),
    } }}
}

#[macro_export]
macro_rules! log_err_ {
    ($e:expr $(, $fmt:tt)+) => { { let _ = log_err!($e $(, $fmt)+); } }
}

#[cfg(test)]
mod test;

#[derive(Clap, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
/// A simple benchmark running tool.
///
/// Gets a set of solvers and a set of benchmarks as inputs and runs each solver on each benchmark.
/// Each solver will be invoked as its own process using
///
/// $ <solver> <benchmark> <timeout>
///
/// stdout, and stderr of the process will be captured and written to the output directory.
///
/// The solver shall indicate success with a return value of zero and failure with a non-zero
/// return value. If the solver returns non-zero its stdout, and stderr will be moved to a
/// the output directory in a subdirectory suffixed by `.err`. These *.err directories may be
/// deleted whe the benchmark runner is invoked with the same output directory again.
pub struct Opts {
    /// directory that must containn must contain poroblem instance files, that will be passed to
    /// the solver as first argument.
    #[clap(
        parse(from_os_str),
        short = "b",
        long = "benchmarks",
        default_value = "benchmarks"
    )]
    pub bench_dir: PathBuf,

    /// Directory containing solvers.
    #[clap(
        parse(from_os_str),
        short = "s",
        long = "solvers",
        default_value = "solvers"
    )]
    pub solver_dir: PathBuf,

    /// timeout in seconds
    pub timeout: u64,

    /// directory to which the outputs written
    #[clap(
        parse(from_os_str),
        short = "o",
        long = "outdir",
        default_value = "benchmark_results"
    )]
    pub outdir: PathBuf,

    // /// only run post processor, not benchmarks
    // #[clap(short = "p", long = "post")]
    // pub only_post_process: bool,

    /// How many threads shall be ran in parallel? [default: number of physical cpus]
    #[clap(short = "t", long = "threads")]
    pub num_threads: Option<usize>,
}

//TODO create sercice module
#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct ServiceConfig {
    pub threads: Option<usize>,
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct ApplicationConfig<A> {
    pub service: ServiceConfig,
    pub dao: DaoConfig,
    pub job: JobConfig<A>,
}

pub fn benchmarks_from_dir<P>(bench_dir: &PathBuf, postpro: &P) -> Result<Vec<Arc<Annotated<Benchmark, P::BAnnot>>>>
where P: Postprocessor
{
    let benchmarks = from_file_or_dir(&bench_dir, |file| Ok(Benchmark { file }))?;
    let ui = Ui::new("Annotating", benchmarks.len());

    benchmarks
        .into_iter()
        .map(|b| {
            let res = postpro.annotate_benchark(&b)?;
            ui.progress();
            Ok(Arc::new(Annotated(b, res)))
        })
        .collect::<Result<_>>()
        .context("failed to annotated benchmarks")
}

fn from_file_or_dir<A, F>(d: &PathBuf, parse: F) -> Result<Vec<A>>
where
    // A: TryFrom<PathBuf, Error = anyhow::Error>,
    F: Fn(PathBuf) -> Result<A, anyhow::Error>,
{
    if d.is_dir() {
        process_results(
            read_dir(d).with_context(|| format!("failed to open directory: {}", d.display()))?,
            |files| {
                files
                    .map(|f| -> Result<A> {
                        let path = f.path().canonicalize().with_context(|| {
                            format!("failed to canonize: {}", f.path().display())
                        })?;
                        parse(path).with_context(|| {
                            format!("failed to parse path: {}", f.path().display())
                        })
                    })
                    .collect::<Result<_>>()
            },
        )
        .with_context(|| format!("failed to read directory: {}", d.display()))?
    } else {
        Ok(vec![ parse(d.clone())? ])
    }
}


fn validate_opts<P: Postprocessor>(
    postpro: &P,
    opts: Opts,
) -> Result<ApplicationConfig<P::BAnnot>> {
    let Opts {
        bench_dir,
        solver_dir,
        outdir,
        num_threads: threads,
        timeout,
    } = opts;

    Ok(ApplicationConfig {
        service: ServiceConfig { threads, },
        dao: DaoConfig { outdir, },
        job: JobConfig {
            solvers: from_file_or_dir(&solver_dir, |file| Ok(Arc::new(Solver::try_from(file)?)))?,
            benchmarks: benchmarks_from_dir(&bench_dir, postpro)?,
            timeout: Duration::from_secs(timeout),
        },
    })
}

impl TryFrom<PathBuf> for Solver {
    type Error = anyhow::Error;
    fn try_from(file: PathBuf) -> Result<Self> {
        //TODO check if it's a file, and if it's executable
        Ok(Solver { file })
    }
}

pub struct PostproIOAccess(PathBuf);

impl PostproIOAccess {
    pub fn benchmark_out(&self, s: &Benchmark) -> io::Result<impl io::Write> {
        File::create(self.0.join(s.file()))
    }
    pub fn solver_out(&self, s: &Solver) -> io::Result<impl io::Write> {
        File::create(self.0.join(s.file()))
    }
    pub fn global_out(&self) -> io::Result<impl io::Write> {
        File::create(self.0.join("summary"))
    }
}

pub trait Summerizable {
    fn write_summary<W>(&self, out: W) -> Result<()>
    where
        W: io::Write;
}

pub trait Postprocessor {
    type Mapped: Send + Sync;
    type Reduced: Serialize + DeserializeOwned + Summerizable;
    /// Benchmark Annotation
    type BAnnot: Serialize + DeserializeOwned + Send + Sync;

    fn annotate_benchark(&self, b: &Benchmark) -> Result<Self::BAnnot>;
    fn map(&self, r: &BenchRunResult<Self::BAnnot>) -> Result<Self::Mapped>;
    fn reduce(
        &self,
        job: &JobConfig<Self::BAnnot>,
        iter: impl IntoIterator<Item = (BenchRunConf<Self::BAnnot>, Self::Mapped)>,
    ) -> Result<Self::Reduced>;
}
fn set_thread_cnt(n: usize) -> Result<()> {
    let r = rayon::ThreadPoolBuilder::new()
        .num_threads(n)
        .build_global();

    if cfg!(test) {
        /* ignore error since tests are multithreaded */
        let _ = r;
    } else {
        /* raise error in main method */
        r?;
    }
    Ok(())
}

#[derive(ThisError, Debug)]
#[error("received termination signal")]
pub struct TermSignal;

unsafe impl Send for TermSignal {}

use lazy_static::*;

lazy_static! {
    static ref TERMINATE: RwLock<bool> = RwLock::new(false);
    static ref TERM_SEND: Mutex<Option<Vec<Sender<TermSignal>>>> = Mutex::new(Some(Vec::new()));
}

fn shall_terminate() -> bool {
    *TERMINATE.read().unwrap()
}

#[allow(unused)]
fn term_receiver() -> Result<Receiver<TermSignal>, TermSignal> {
    // let (tx, rx) = channel();
    let (tx, rx) = bounded(1);
    let mut lock = TERM_SEND
        .lock()
        .expect("failed to register termination receiver");
    match &mut *lock {
        Some(ref mut rcvs) => {
            rcvs.push(tx);
            Ok(rx)
        }
        None => Err(TermSignal),
    }
}
#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("{0}")]
    TermSignal(#[from] TermSignal),
}

fn run_with_opts<P>(post: P, opts: Opts) -> Result<P::Reduced, Error>
where
    P: Postprocessor + Sync,
    <P as Postprocessor>::BAnnot: Clone,
{
    let conf = validate_opts(&post, opts)?;
    run_with_conf(post, conf)
}

fn run_with_conf<P>(post: P, conf: ApplicationConfig<P::BAnnot>) -> Result<P::Reduced, Error>
where
    P: Postprocessor + Sync,
    <P as Postprocessor>::BAnnot: Clone,
{
    let ApplicationConfig {
        job,
        dao,
        service,
    } = conf;

    let dao = dao::create(dao)?;
    let service = service::create(service)?;
    service.run(job, &dao, &post)

}

pub fn main_with_opts<P>(post: P, opts: Opts) -> Result<()>
where
    P: Postprocessor + Sync,
    <P as Postprocessor>::BAnnot: Clone,
{
    match run_with_opts(post, opts) {
        Ok(_) | Err(Error::TermSignal(TermSignal)) => Ok(()),
        Err(Error::Anyhow(e)) => Err(e),
    }
}



pub fn main_with_conf<P>(post: P, conf: ApplicationConfig<P::BAnnot>) -> Result<()>
where
    P: Postprocessor + Sync,
    <P as Postprocessor>::BAnnot: Clone,
{
    match run_with_conf(post, conf) {
        Ok(_) | Err(Error::TermSignal(TermSignal)) => Ok(()),
        Err(Error::Anyhow(e)) => Err(e),
    }
}


