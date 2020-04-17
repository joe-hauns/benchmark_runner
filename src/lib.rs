use itertools::*;
use rayon::prelude::*;
use std::ffi::*;
use std::fmt;
use std::fs::*;
use std::fs;
use std::io;
use std::path::*;
use std::process::*;
use std::sync::*;
use structopt::*;
type Result<A> = DynResult<A>;
type DynResult<A> = std::result::Result<A, Box<dyn std::error::Error>>;

// macro_rules! warn {
//     ($fmt:expr $(,$x:tt)*) => {{ let _ = eprintln!($fmt $(,$x)*); }}
// }

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
    #[structopt(parse(from_os_str), short = "s", long = "solvers", default_value = "solvers")]
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
    fn try_from(p: P) -> io::Result<Self>
    where
        Self: Sized;
}

impl Opts {
    fn lines_to_files<A>(d: &PathBuf) -> io::Result<Vec<Arc<A>>>
    where
        A: TryFrom<PathBuf>,
    {
        read_dir(d)?
            .map_results(|e| e.path())
            .map_results(|e| A::try_from(e).map(Arc::new))
            .collect::<io::Result<_>>()?
    }
    fn validate(self) -> io::Result<Config> {
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
        Benchmark { file, }
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
    fn try_from(file: PathBuf) -> io::Result<Self> {
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
        Solver { bin, }
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
    fn try_from(bin: PathBuf) -> io::Result<Self> {
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

    fn remove_files(&self, ui: &Ui) -> Result<()> {
        let dir = self.outdir();
        let err_dir = dir.with_extension("err");
        if err_dir.exists() {
            remove_dir_all(&err_dir)?;
        }
        rename(&dir, &err_dir)?;
        ui.println(format!("moving result to {} (may be deleted in another run)", err_dir.display()));
        Ok(())

    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct Config {
    solvers: Vec<Arc<Solver>>,
    benchmarks: Vec<Arc<Benchmark>>,
    opts: Opts,
    timeout: usize,
}

impl Config {
    fn postpro_dir(&self, post: &impl Postprocessor) -> io::Result<PathBuf> {
        let dir = self.opts.outdir.join("post_proc");
        create_dir_all(&dir)?;
        Ok(dir.join(self.timeout.to_string()).join(post.id()))
    }
}
use indicatif::*;
use timer::Timer;

struct Ui {
    config: Arc<Config>,
    timer: Arc<Mutex<Timer>>,
    prog: MultiProgress,
    bar: ProgressBar,
}

struct Job {
    bar: ProgressBar,
    _guard: timer::Guard,
}


impl Drop for Job {
    fn drop(&mut self) {
        self.bar.finish();
    }
}

impl Ui {
    fn new(job: &str, cnt: usize, config: Arc<Config>) -> Self {
        let prog = MultiProgress::with_draw_target(ProgressDrawTarget::stdout_with_hz(1));
        let bar = prog.add(ProgressBar::new(cnt as u64)) ;
        bar.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .progress_chars("##-"));
        bar.set_message(job);
        Ui { bar, prog, config, timer: Arc::new(Mutex::new(Timer::new())), }
    }

    fn println(&self, m: impl std::fmt::Display) {
        println!("{}", m);
        // self.bar.println(m.to_string()); //TODO check me
    }

    fn progress(&self) {
        self.bar.inc(1);
    }

    fn add_job(&self, msg: &str) -> Job {
        let timeout = self.config.timeout as _;
        let bar = self.prog.add(ProgressBar::new(timeout));
        bar.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {msg}")
                .progress_chars("##-")
            );
        bar.set_message(msg);
        let mut counter = 0;
        Job { 
            _guard: {
                let bar = bar.clone();
                self.timer.lock().unwrap()
                    .schedule_repeating(chrono::Duration::seconds(1), move || {
                    counter += 1;
                    if counter == timeout {
                        bar.set_style(
                            ProgressStyle::default_bar()
                                .template("[{elapsed_precise}] {bar:40.cyan/blue} {msg}")
                                .progress_chars("##-")
                            );
                    } else if counter < timeout {
                        bar.inc(1);
                    }


                }) 
            },
            bar,
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.bar.finish();
        if let Err(e) = self.prog.join_and_clear() {
            eprintln!("failed to join multiprogress: {}", e);
        }
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
        File::open(self.run.stdout())
    }

    pub fn stderr(&self) -> io::Result<impl io::Read> {
        File::open(self.run.stderr())
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
    fn process(&self, r: &BenchmarkResult) -> Result<()>;
    fn id(&self) -> &str;
    fn write_results(self, io: PostproIOAccess) -> Result<()>;
}

pub fn main(post: impl Postprocessor + Sync) -> DynResult<()> {
    main_with_opts(post, Opts::from_args())
}

fn set_thread_cnt(n: usize) -> DynResult<()> {
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

fn main_with_opts(post: impl Postprocessor + Sync, opts: Opts) -> DynResult<()> {
    let config = Arc::new(opts.validate()?);

    println!("output dir: {}", config.opts.outdir.display());
    println!("cpus: {}", num_cpus::get_physical());

    set_thread_cnt(config.opts.num_threads 
            .unwrap_or_else(||num_cpus::get_physical()))?;

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
        let ui = Ui::new("Benchmarking", todo.len(), config.clone());
        done.par_extend(todo[..].par_iter().filter_map(|conf| {
            let result = match run(&ui, &conf) {
                Ok(x) => Some(x),
                Err(e) => {
                    ui.println(format!("failed to run {}: {}", conf, e));
                    None
                }
            };
            ui.progress();
            result
        }));
    }

    {
        let ui = Ui::new("Postprocessing", done.len(), config.clone());
        done.into_par_iter().for_each(|x| match post.process(&x) {
            Ok(()) => (),
            Err(e) => {
                ui.println(format!("failed to prostprocess: {}", e));
                    if let Err(e) = x.run.remove_files(&ui) {
                        ui.println(format!("failed to delete result: {}", e));
                    }
                    ui.progress();
            },
        });
    }

    let dir = config.postpro_dir(&post)?;
    fs::create_dir_all(&dir)?;
    println!("writing to output dir: {}", dir.display());
    post.write_results(PostproIOAccess(dir))?;
    Ok(())
}

fn run(ui: &Ui, conf: &BenchConf) -> Result<BenchmarkResult> {
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
            // ui.println(msg);
            fs::write(conf.cmd(), format!("{}\n", msg))?;
            let _ = ui.add_job(&msg);
            Command::new($bin)$(.arg($args))*
                .stdout(File::create(conf.stdout())?)
                .stderr(File::create(conf.stderr())?)
                .output()?
        }}
    }

    let result = cmd!(
        &solver.bin,
        &benchmark.file,
        format!("{}", conf.config.timeout));

    if result.status.success() {
        BenchmarkResult::from_file(conf.clone())
    } else {
        conf.remove_files(ui)?;
        let msg = format!(
            "unexpected exit status (code: {:?})",
            result.status.code(),
        );
        Err(io::Error::new(io::ErrorKind::Other, msg))?

    }
}
