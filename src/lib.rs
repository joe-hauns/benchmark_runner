mod dao;
mod interface;
mod dto;
mod ui;
mod service;

pub use interface::*;
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
use std::fs;
use std::fs::DirEntry;
use std::io;
use std::path::*;
use std::sync::*;
use std::time::*;
use clap::*;
use thiserror::Error as ThisError;
use wait_timeout::ChildExt;
use std::convert::*;
pub use dao::{read_json, write_json};

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
pub struct ApplicationConfig<P> 
    where P: Benchmarker
{
    pub service: ServiceConfig,
    pub dao: DaoConfig,
    pub job: JobConfig<P>,
}


fn validate_opts<P: Benchmarker>(
    opts: Opts,
) -> Result<ApplicationConfig<P>> 
    where P: Benchmarker,
          P::Solver: FromDir,
          P::Benchmark: FromDir,
{
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
            solvers: FromDir::from_dir(&solver_dir)?,
            benchmarks: FromDir::from_dir(&bench_dir)?,
            timeout: Duration::from_secs(timeout),
        },
    })
}


impl<A> FromDir for Arc<A> 
    where A: FromDir
{
    fn from_dir<P>(dir: P) -> Result<Self> 
        where P: AsRef<Path>,
    {
        Ok(Arc::new(FromDir::from_dir(dir)?))
    }
}


impl<A> FromDir for Vec<A> 
    where A: FromDir
{
    fn from_dir<P>(dir: P) -> Result<Self> 
        where P: AsRef<Path>,
    {
        process_results(read_dir(&dir)?, |d| {
            d.map(|d: DirEntry| A::from_dir(d.path())).collect::<Result<Vec<A>>>()
        })?
    }
}

fn read_dir<'a, P>(path: &'a P) -> Result<impl Iterator<Item = Result<DirEntry>> + 'a>
where
    P: AsRef<Path> + 'a,
{
    Ok(fs::read_dir(&path)
        .with_context(|| format!("failed to read dir: {}", path.as_ref().display()))?
        .map(move |x| {
            x.with_context(move || {
                format!("failed to read path entry: {}", path.as_ref().display())
            })
        }))
}

pub trait FromDir {
    fn from_dir<P>(dir: P) -> Result<Self>
    where
        P: AsRef<Path>,
        Self: Sized;
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
    P: Benchmarker + Sync,
    P::Solver: FromDir,
    P::Benchmark: FromDir,
{
    let conf = validate_opts::<P>(opts)?;
    run_with_conf(post, conf)
}

fn run_with_conf<P>(post: P, conf: ApplicationConfig<P>) -> Result<P::Reduced, Error>
where
    P: Benchmarker + Sync,
    P::Benchmark: FromDir,
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
    P: Benchmarker + Sync,
    P::Solver: FromDir,
    P::Benchmark: FromDir,
{
    match run_with_opts(post, opts) {
        Ok(_) | Err(Error::TermSignal(TermSignal)) => Ok(()),
        Err(Error::Anyhow(e)) => Err(e),
    }
}



pub fn main_with_conf<P>(post: P, conf: ApplicationConfig<P>) -> Result<()>
where
    P: Benchmarker + Sync,
    P::Benchmark: FromDir,
{
    match run_with_conf(post, conf) {
        Ok(_) | Err(Error::TermSignal(TermSignal)) => Ok(()),
        Err(Error::Anyhow(e)) => Err(e),
    }
}


