mod dto;
mod dao;

use dao::*;
use itertools::*;
use rayon::prelude::*;
use std::ffi::*;
use std::fs;
use std::fs::*;
use std::io;
use std::path::*;
use std::fmt::Arguments as FormatArgs;
use crossbeam_channel::*;
use std::sync::*;
use structopt::*;
use wait_timeout::ChildExt;
use serde::{Serialize, Deserialize};
use serde::de::DeserializeOwned;
use anyhow::*;
use anyhow::Result;
use std::time::*;
pub use dto::*;
use std::convert::TryFrom;
use thiserror::Error as ThisError;

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

macro_rules! log_err_ {
    ($e:expr $(, $fmt:tt)+) => { { let _ = log_err!($e $(, $fmt)+); } }
}

#[cfg(test)]
mod test;

#[derive(StructOpt, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
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
struct Opts {

    /// directory that must containn must contain poroblem instance files, that will be passed to
    /// the solver as first argument.
    #[structopt(
        parse(from_os_str),
        short = "b",
        long = "benchmarks",
        default_value = "benchmarks"
    )]
    bench_dir: PathBuf,

    /// Directory containing solvers. 
    #[structopt(
        parse(from_os_str),
        short = "s",
        long = "solvers",
        default_value = "solvers"
    )]
    solver_dir: PathBuf,

    /// timeout in seconds
    timeout: u64,

    /// directory to which the outputs written 
    #[structopt(
        parse(from_os_str),
        short = "o",
        long = "outdir",
        default_value = "benchmark_results"
    )]
    outdir: PathBuf,

    /// only run post processor, not benchmarks
    #[structopt(short = "p", long = "post")]
    only_post_process: bool,

    /// How many threads shall be ran in parallel? [default: number of physical cpus]
    #[structopt(short = "t", long = "threads")]
    num_threads: Option<usize>,
}

// trait TryFrom<P> {
//     fn try_from(p: P) -> Result<Self>
//     where
//         Self: Sized;
// }

impl Opts {
    fn from_files_in_dir<A>(d: &PathBuf) -> Result<Vec<Arc<A>>>
    where
        A: TryFrom<PathBuf, Error = anyhow::Error>,
        // <A as TryFrom<PathBuf>>::Error: Send + Sync + std::error::Error + 'static,
        // Result<Arc<A>, <A as TryFrom<PathBuf>>::Error>: anyhow::Context<A, <A as TryFrom<PathBuf>>::Error>,
    {
        process_results( read_dir(d).with_context(||format!("failed to open directory: {}", d.display()))?,
            |files| files 
                .map(|f| -> Result<Arc<A>> {
                    let path = f.path().canonicalize()
                        .with_context(||format!("failed to canonize: {}", f.path().display()))?; 
                    let parsed = A::try_from(path)
                        .with_context(|| format!("failed to parse path: {}", f.path().display()))?;
                    Ok(Arc::new(parsed))
                })
                .collect::<Result<_>>()
                )
            .with_context(||format!("failed to read directory: {}", d.display()))?
    }
    fn validate(self) -> Result<ApplicationConfig> {
        Ok(ApplicationConfig {
            job_conf: Arc::new(JobConfig {
                solvers: Self::from_files_in_dir(&self.solver_dir)?,
                benchmarks: Self::from_files_in_dir(&self.bench_dir)?,
                timeout: Duration::from_secs(self.timeout),
            }),
            opts: self,
        })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct ApplicationConfig {
    opts: Opts,
    job_conf: Arc<JobConfig>,
}

impl ApplicationConfig {
    fn postpro_dir(&self) -> Result<PathBuf> {
        let dir = self.opts.outdir.join("timeout").join("post_proc");
        create_dir_all(&dir)
            .with_context(||format!( "failed to create postpro dir: {}", dir.display() ))?;
        Ok(dir)
    }
}

use indicatif::*;

pub struct Ui {
    bar: ProgressBar,
}

impl Ui {
    pub fn new(job: &str, cnt: usize) -> Self {
        let bar = ProgressBar::new(cnt as u64);
        bar.set_style(ProgressStyle::default_bar()
            .template("{spinner} {msg} [{elapsed_precise}] [{wide_bar:.green/fg}] {pos:>7}/{len:7} (left: {eta_precise})")
            .progress_chars("=> "));
        bar.set_message(job);
        bar.enable_steady_tick(100);
        Ui { bar }
    }

    pub fn println(&self, m: impl std::fmt::Display) {
        self.bar.println(m.to_string()); 
    }

    pub fn progress(&self) {
        self.bar.inc(1);
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.bar.finish();
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
        where W: io::Write;
}

pub trait Postprocessor {
    type Mapped: Send + Sync;
    type Reduced: Serialize + DeserializeOwned + Summerizable;
    fn map(&self, r: &BenchRunResult) -> Result<Self::Mapped>;
    fn reduce(&self, job_conf: &JobConfig, iter: impl IntoIterator<Item=(BenchRunConf, Self::Mapped)>) -> Result<Self::Reduced>;
}

pub fn main(post: impl Postprocessor + Sync) -> Result<()> {
    match main_with_opts(post, Opts::from_args()) {
        Ok(_) | Err(Error::TermSignal(TermSignal)) => Ok(()),
        Err(Error::Anyhow(e)) => Err(e),
    }
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
    let mut lock = TERM_SEND.lock()
        .expect("failed to register termination receiver");
    match &mut *lock {
        Some(ref mut rcvs) =>  {
            rcvs.push(tx);
            Ok(rx)
        }
        None => Err(TermSignal),
    }
}

fn setup_ctrlc() {
    log_err_!(
        ctrlc::set_handler(move || {
            eprintln!("received termination signal");
            *TERMINATE.write().unwrap() = true;
            if let Some(snds) = TERM_SEND.lock().unwrap().take() {
                for snd in snds {
                    log_err_!(snd.send(TermSignal), "failed to send termination signal");
                }
            } else {
                eprintln!("termination signal was already sent");
            }
        }),
        "failed to set up ctrl-c signal handling"
    );
}


#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("{0}")]
    TermSignal(#[from] TermSignal),
}

fn main_with_opts<P>(post: P, opts: Opts) -> Result<P::Reduced, Error> 
    where P: Postprocessor + Sync
{
    setup_ctrlc();
    let config = Arc::new(opts.validate()?);

    println!("output dir: {}", config.opts.outdir.display());
    println!("cpus: {}", num_cpus::get_physical());

    log_err_!(
        set_thread_cnt(
            config
                .opts
                .num_threads
                .unwrap_or_else(|| num_cpus::get_physical())
        ),
        "failed to set number of threads"
    );
    let dao = dao::create(&config.opts)?;

    let (mut done, todo): (Vec<_>, Vec<_>) = {
        let bs = &config.job_conf.benchmarks[..];
        let cs = &config.job_conf.solvers[..];
        let ui = Ui::new("Reading old results", bs.len() * cs.len());
        let config = &config;
        bs.par_iter()
            .flat_map(move |benchmark| {
                cs.par_iter()
                    .map(move |solver| BenchRunConf {
                        job: config.job_conf.clone(), 
                        benchmark: benchmark.clone(), 
                        solver: solver.clone(),
                    })
            })
            .partition_map(|c| {
                let result = match dao.read_result(&c) {
                    Ok(Some(res)) => Either::Left(res),
                    Ok(None) => Either::Right(c),
                    Err(e) => {
                        ui.println(format_args!("failed to read result: {:#}", e));
                        Either::Right(c)
                    }
                };
                ui.progress();
                result
            })
    };

    let remove_files = |ui: &Ui, conf: &BenchRunConf, reason: FormatArgs| {
        match dao.remove_result(&conf, reason) {
            Ok(()) => ui.println(format_args!("removed output files for {}", conf)),
            Err(e) => ui.println(format_args!("failed to remove output files: {:#}", e)),
        }
    };

    if !config.opts.only_post_process {
        let ui = Ui::new("Benchmarking", todo.len());
        done.par_extend(todo[..].into_par_iter().filter_map(|conf| {
            if shall_terminate() {
                None
            } else {
                let result = match run(&conf) {
                    Ok(x) => Some(x),
                    Err(Error::TermSignal(TermSignal)) => None,
                    Err(e) => {
                        remove_files(&ui, &conf, format_args!("failed to run {}: {:#}", conf, e));
                        None
                    }
                };
                ui.progress();
                result
            }
        }));
    }

    if shall_terminate() {
        return Err(Error::TermSignal(TermSignal))
    }

    let mapped: Vec<_> = {
        let ui = Ui::new("Mapping", done.len());

        done.par_iter()
            .filter_map(|x| Some({
                let res = match post.map(&x) {
                    Ok(mapped) => (x.run.clone(), mapped),
                    Err(e) => {
                        remove_files(&ui, &x.run, format_args!("failed to prostprocess: {:#}", e));
                        return None;
                    }
                };
                ui.progress();
                if shall_terminate() {
                    Err(TermSignal)
                } else {
                    Ok(res)
                }
            }))
            .collect::<std::result::Result<Vec<_>, _>>()?
    };


    let reduced = post.reduce(&config.job_conf, mapped)?;


    if shall_terminate() {
        return Err(Error::TermSignal(TermSignal))
    }

    let dir = config.postpro_dir()?;
    fs::create_dir_all(&dir).with_context(||format!("failed to create directory: {}", dir.display()))?;
    println!("writing to output dir: {}", dir.display());
    // TODO serialize summary to file
    reduced.write_summary(std::io::stdout().lock())?;
    Ok(reduced)
}

fn run(run: &BenchRunConf) -> Result<BenchRunResult, Error> {
    use std::io::Read;
    use std::process::*;
    let solver = &run.solver;
    let benchmark = &run.benchmark;

    macro_rules! cmd {
        ($bin:expr $(, $args:expr)*) => {{
            let mut msg = format!("{}", $bin.display());
            $({
                let args = $args;
                let a: &OsStr = args.as_ref();
                match a.to_str() {
                    Some(s) => {
                        msg.push_str(" ");
                        msg.push_str(s);
                    }
                    None => msg.push_str(" ???"),
                }
            })*

            // TODO save command to BenchRunConf
            // fs::write(run.cmd(), format!("{}\n", msg))
            //     .with_context(|| format!( "failed to write command to file: {}", run.cmd().display()))?;

            let mut cmd = Command::new($bin);
            $(cmd.arg($args);)*
            // cmd.stdout(Cursor::new(&mut stdout));
            // cmd.stderr(Cursor::new(&mut stderr));
            cmd.spawn().context("failed to launch child process")?
        }}
    }

    let mut child = cmd!(
        &solver.file,
        &benchmark.file,
        format!("{}", run.job.timeout.as_secs())
    );

    use std::time::*;

    let start = Instant::now();
    // TODO make poll timeout relative to timeout of benchmark
    let poll = Duration::from_millis(500);
    loop {
        let status = child.wait_timeout(poll)
            .context("failed to wait for child process")?;

        let with_bench_status = |child: &mut Child, exit_status: Option<i32>, status: BenchmarkStatus| -> Result<BenchRunResult, Error> {

                    macro_rules! read_buf {
                        ($buf: ident) => {
                            {
                                let mut $buf = vec![];
                                if let Some(buf) = &mut child.$buf {
                                    buf.read_to_end(&mut $buf)
                                        .with_context(||format!( "failed to read {} of {}", stringify!( $buf ), run ))?;
                                }
                                $buf
                            }
                        }
                    }

                    let stdout = read_buf!(stdout);
                    let stderr = read_buf!(stderr);

                    Ok(BenchRunResult {
                        run: run.clone(),
                        status,
                        time: start.elapsed(),
                        stdout,
                        stderr,
                        exit_status,
                    })

        };

        match status {
            Some(status) => {
                return if !status.success() && shall_terminate() {
                    Err(Error::TermSignal(TermSignal))
                } else {
                    with_bench_status(&mut child, status.code(), BenchmarkStatus::Success)
                }
            }
            None => {
                if shall_terminate() {
                    child.kill().context("failed to kill child process")?;
                    return Err(TermSignal)?;
                }
                if start.elapsed() > run.job.timeout.mul_f64(1.2) {
                    child.kill().context("failed to kill child process")?;
                    return with_bench_status(&mut child, None, BenchmarkStatus::Timeout)
                }
            }
        }
    }
}
