use itertools::*;
use rayon::prelude::*;
use std::ffi::*;
use std::fmt;
use std::fs::*;
use std::io;
use std::path::*;
use std::process::*;
use std::sync::Arc;
use structopt::*;
type Result<A> = std::result::Result<A, Box<dyn std::error::Error>>;

#[derive(StructOpt, Clone,Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opts {
    /// input directory. Must contain a directory named `solvers` and a directory called `benchmarks`.
    ///
    /// <indir>/solvers must contain executables that will be invoked with `<bin> <benchmark> <timeout_secs>`
    #[structopt(parse(from_os_str), short = "i", long = "input", default_value = ".")]
    indir: PathBuf,

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
            solvers: Self::lines_to_files(&self.indir.join("solvers"))?,
            benchmarks: Self::lines_to_files(&self.indir.join("benchmarks"))?,
            timeout: self.timeout,
            opts: self,
        })
    }
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Benchmark {
    file: PathBuf,
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
    fn postpro_file(&self, post: &impl Postprocessor) -> io::Result<PathBuf> {
        let dir = self.opts.outdir.join("post_proc");
        create_dir_all(&dir)?;
        Ok(dir.join(post.id()))
    }
}
use indicatif::*;

struct Ui {
    bar: ProgressBar,
}

impl Ui {
    fn new(job: &str, cnt: usize) -> Self {
        let bar = ProgressBar::new(cnt as u64);
        bar.set_message(job);
        Ui { bar }
    }

    fn println(&self, m: impl std::fmt::Display) {
        self.bar.println(m.to_string());
    }

    fn progress(&self) {
        self.bar.inc(1);
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

pub trait Postprocessor {
    fn process(&self, r: &BenchmarkResult) -> Result<()>;
    fn id(&self) -> &str;
    fn write_results<W>(self, w: W) -> Result<()> 
        where W: io::Write;
}

pub fn main(post: impl Postprocessor + Sync) -> Result<()> {
    let config = Arc::new(Opts::from_args().validate()?);

    println!("output dir: {}", config.opts.outdir.display());
    println!("cpus: {}", num_cpus::get_physical());

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get_physical())
        .build_global()?;

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
        let ui = Ui::new("Postprocessing", done.len());
        done.into_par_iter().for_each(|x| match post.process(&x) {
            Ok(()) => (),
            Err(e) => {
                ui.println(format!("failed to prostprocess: {}", e));
                    if let Err(e) = x.run.remove_files(&ui) {
                        ui.println(format!("failed to delete result: {}", e));
                    }
            },
        });
    }

    let file = config.postpro_file(&post)?;
    println!("writing to output file: {}", file.display());
    post.write_results(File::create(file)?)?;
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
            ui.println(msg);
            // println!();
            Command::new($bin)$(.arg($args))*
        }}
    }

    let result = cmd!(
        &solver.bin,
        &benchmark.file,
        format!("{}", conf.config.timeout)
    )
    .stdout(File::create(conf.stdout())?)
    .stderr(File::create(conf.stderr())?)
    .output()?;

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
