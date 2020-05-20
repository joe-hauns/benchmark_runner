// TODO use anyhow
use itertools::*;
use rayon::prelude::*;
use std::ffi::*;
use std::fmt;
use std::fs;
use std::fs::*;
use std::io;
use std::path::*;
use std::process::*;
use std::sync::mpsc::*;
use std::sync::*;
use structopt::*;
use anyhow::*;

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

#[derive(StructOpt, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
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

    /// Directory containing solvers. These solvers must be executables that will be invoked by
    /// $ <bin> <benchmark> <timeout_secs>
    #[structopt(
        parse(from_os_str),
        short = "s",
        long = "solvers",
        default_value = "solvers"
    )]
    solver_dir: PathBuf,

    /// timeout in seconds
    timeout: usize,

    /// directory where the outputs of the runs shall be written to
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

    /// How many threads shall be ran in parallel? Default: number of physical cpus
    #[structopt(short = "t", long = "threads")]
    num_threads: Option<usize>,
}

trait TryFrom<P> {
    fn try_from(p: P) -> Result<Self>
    where
        Self: Sized;
}

impl Opts {
    fn lines_to_files<A>(d: &PathBuf) -> Result<Vec<Arc<A>>>
    where
        A: TryFrom<PathBuf>,
    {
        read_dir(d).with_context(||format!("failed to open directory: {}", d.display()))?
            .map_results(|e| e.path())
            .map_results(|e| A::try_from(e).map(Arc::new))
            .collect::<Result<_, _>>()
            .with_context(||format!("failed to read directory: {}", d.display()))?
    }
    fn validate(self) -> Result<Config> {
        Ok(Config {
            solvers: Self::lines_to_files(&self.solver_dir)?,
            benchmarks: Self::lines_to_files(&self.bench_dir)?,
            timeout: self.timeout,
            opts: self,
        })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Benchmark {
    file: PathBuf,
}

impl Benchmark {
    #[cfg(test)]
    fn new(file: PathBuf) -> Self {
        Benchmark { file }
    }
}

impl fmt::Display for BenchConf {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        write!(w, "solver: {} benchmark: {}", self.solver, self.benchmark)
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
    fn try_from(file: PathBuf) -> Result<Self> {
        Ok(Benchmark { file })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Solver {
    bin: PathBuf,
}

impl Solver {
    #[cfg(test)]
    fn new(bin: PathBuf) -> Self {
        Solver { bin }
    }
}

impl fmt::Display for Solver {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        self.bin.display().fmt(w)
    }
}

impl Solver {
    pub fn file(&self) -> &OsStr {
        &self.bin.file_name().unwrap()
    }
}

impl TryFrom<PathBuf> for Solver {
    fn try_from(bin: PathBuf) -> Result<Self> {
        Ok(Solver { bin })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct BenchConf {
    config: Arc<Config>,
    benchmark: Arc<Benchmark>,
    solver: Arc<Solver>,
}
impl BenchConf {
    fn new(config: Arc<Config>, benchmark: Arc<Benchmark>, solver: Arc<Solver>) -> Self {
        BenchConf {
            config,
            benchmark,
            solver,
        }
    }
    fn outdir(&self) -> PathBuf {
        let mut path = PathBuf::from(&self.config.opts.outdir);
        path.push(self.solver.file());
        path.push(format!("{}", self.config.timeout));
        path.push(self.benchmark.file());
        path
    }

    fn cmd(&self) -> PathBuf {
        self.outdir().join("cmd")
    }
    fn stdout(&self) -> PathBuf {
        self.outdir().join("stdout")
    }
    fn stderr(&self) -> PathBuf {
        self.outdir().join("stderr")
    }

    fn remove_files(&self, ui: &Ui, reason: impl fmt::Display) -> Result<()> {
        let dir = self.outdir();
        let err_dir = dir.with_extension("err");
        if err_dir.exists() {
            remove_dir_all(&err_dir)
                .with_context(||format!("failed to remove directory: {}", err_dir.display()))?;
        }
        rename(&dir, &err_dir).with_context(||format!("failed move dir {} -> {}", dir.display(), err_dir.display()))?;
        ui.println(format!(
            "{}: moving result to {} (may be deleted in another run)",
            reason,
            err_dir.display()
        ));
        let reasons = err_dir.join("reason.txt");
        fs::write(&reasons, reason.to_string())
            .with_context(||format!("failed to write {}", reasons.display()))?;
        Ok(())
    }
}

use derivative::*;

#[derive(Derivative)]
#[derivative(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct Config {
    solvers: Vec<Arc<Solver>>,
    benchmarks: Vec<Arc<Benchmark>>,
    opts: Opts,
    timeout: usize,
    // #[derivative(PartialEq = "ignore")]
    // #[derivative(Hash = "ignore")]
    // #[derivative(Hash = "ignore")]
    // #[derivative(Debug = "ignore")]
    // #[derivative(Hash = "ignore")]
    // #[derivative(Ord = "ignore")]
    // #[derivative(PartialOrd = "ignore")]
    // #[derivative(PartialEq = "ignore")]
    // recv_term: spmc::Receiver<TermSignal>,
}

impl Config {
    fn postpro_dir(&self, post: &impl Postprocessor) -> io::Result<PathBuf> {
        let dir = self.opts.outdir.join("post_proc");
        create_dir_all(&dir)?;
        Ok(dir.join(self.timeout.to_string()).join(post.id()))
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
        // println!("{}", m);
        self.bar.println(m.to_string()); //TODO check me
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

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchmarkConfig<'a>(&'a Config);

impl<'a> BenchmarkConfig<'a> {
    pub fn solvers(&self) -> &[Arc<Solver>] {
        &self.0.solvers
    }
    pub fn benchmarks(&self) -> &[Arc<Benchmark>] {
        &self.0.benchmarks
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct BenchmarkResult {
    run: BenchConf,
}

impl BenchmarkResult {
    pub fn benchmark(&self) -> &Benchmark {
        &self.run.benchmark
    }
    pub fn solver(&self) -> &Solver {
        &self.run.solver
    }

    pub fn stdout(&self) -> io::Result<impl io::Read> {
        fs::File::open(self.run.stdout())
    }

    pub fn stderr(&self) -> io::Result<impl io::Read> {
        fs::File::open(self.run.stderr())
    }


    pub fn config<'a>(&'a self) -> BenchmarkConfig<'a> {
        BenchmarkConfig(&self.run.config)
    }

}

impl BenchmarkResult {
    fn from_file(conf: BenchConf) -> Result<Self> {
        let check_file = |f: &PathBuf| -> io::Result<()> {
            if f.exists() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("file not found: {}", f.display()),
                ))
            }
        };
        check_file(&conf.stdout())?;
        check_file(&conf.stderr())?;
        Ok(BenchmarkResult { run: conf })
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

pub trait Postprocessor {
    type Processed: Send + Sync;
    type Reduced;
    fn map(&self, r: &BenchmarkResult) -> Result<Self::Processed>;
    fn reduce(&self, iter: impl IntoIterator<Item=Self::Processed>) -> Result<Self::Reduced>;
    fn id(&self) -> &str;
    fn write_reduced(&self, results: Self::Reduced, conf: BenchmarkConfig, io: PostproIOAccess) -> Result<()>;
}

pub fn main(post: impl Postprocessor + Sync) -> Result<()> {
    main_with_opts(post, Opts::from_args())
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

struct TermSignal;

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
fn term_receiver() -> Receiver<TermSignal> {
    let (tx, rx) = channel();
    unimplemented!(); // TODO
    rx
}

fn setup_ctrlc() {
    log_err_!(
        ctrlc::set_handler(move || {
            println!("received termination signal");
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

fn main_with_opts<P>(post: P, opts: Opts) -> Result<()> 
    where P: Postprocessor + Sync
{
    macro_rules! handle_term_signal {
        ($x:expr) => {
            match $x {
                Ok(x) => x,
                Err(TermSignal) => return Ok(()),
            }
        };
    }
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

    let (mut done, todo): (Vec<_>, Vec<_>) = {
        let bs = &config.benchmarks[..];
        let cs = &config.solvers[..];
        let config = &config;
        bs.par_iter()
            .flat_map(move |benchmark| {
                cs.par_iter()
                    .cloned()
                    .map(move |solver| BenchConf::new(config.clone(), benchmark.clone(), solver))
            })
            .partition_map(|c| match BenchmarkResult::from_file(c.clone()) {
                Ok(res) => Either::Left(res),
                Err(_) => Either::Right(c),
            })
    };

    if !config.opts.only_post_process {
        let ui = Ui::new("Benchmarking", todo.len());
        done.par_extend(todo[..].into_par_iter().filter_map(|conf| {
            if shall_terminate() {
                None
            } else {
                let result = match run(&ui, &conf) {
                    Ok(x) => Some(x),
                    Err(e) => {
                        log_err_!(conf.remove_files(&ui, format!("failed to run {}: {}", conf, e)), "failed to remove output files");
                        None
                    }
                };
                ui.progress();
                result
            }
        }));
    }

    if shall_terminate() {
        return Ok(());
    }

    let mapped: Vec<P::Processed> = {
        let ui = Ui::new("Mapping", done.len());

        handle_term_signal!(
        done.par_iter()
            .filter_map(|x| Some({
                let res = match post.map(&x) {
                    Ok(x) => x,
                    Err(e) => {
                        if let Err(e) = x.run.remove_files(&ui, format!("failed to prostprocess: {}", e)) {
                            ui.println(format!("failed to delete result: {}", e));
                        }
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
            .collect::<std::result::Result<Vec<_>, _>>())
    };


    let reduced = post.reduce(mapped)?;


    if shall_terminate() {
        return Ok(());
    }

    let dir = config.postpro_dir(&post)?;
    fs::create_dir_all(&dir)?;
    println!("writing to output dir: {}", dir.display());
    post.write_reduced(reduced, BenchmarkConfig(&config), PostproIOAccess(dir))?;
    Ok(())
}

fn run(_ui: &Ui, conf: &BenchConf) -> Result<BenchmarkResult> {
    let solver = &conf.solver;
    let benchmark = &conf.benchmark;
    let dir = conf.outdir();

    create_dir_all(&dir)?;

    macro_rules! cmd {
        ($bin:expr $(, $args:expr)*) => {{
            let mut msg = format!("{}", $bin.display());
            // println!();
            // print!("{}", $bin.display());
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
            fs::write(conf.cmd(), format!("{}\n", msg))?;

            let mut cmd = Command::new($bin);
            $(cmd.arg($args);)*
            cmd.stdout(File::create(conf.stdout())?);
            cmd.stderr(File::create(conf.stderr())?);
            cmd.output()
        }}
    }

    let result = cmd!(
        &solver.bin,
        &benchmark.file,
        format!("{}", conf.config.timeout)
    )?;

    if result.status.success() {
        BenchmarkResult::from_file(conf.clone())
    } else {
        let msg = format!("unexpected exit status (code: {:?})", result.status.code(),);
        Err(io::Error::new(io::ErrorKind::Other, msg))?
    }
}
