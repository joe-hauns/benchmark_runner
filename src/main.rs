use itertools::*;
use std::ffi::*;
use std::fs::*;
use std::fmt;
use std::process::*;
use std::io;
use std::io::*;
use std::path::*;
use rayon::prelude::*;
use structopt::*;

#[derive(Debug, StructOpt)]
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
    #[structopt(parse(from_os_str), short = "o", long = "outdir", default_value = "benchmark_results")]
    outdir: PathBuf, }
trait TryFrom<P> {
    fn try_from(p: P) -> io::Result<Self> where Self: Sized;
}

impl Opts {
    fn lines_to_files<A>(d: &PathBuf) -> io::Result<Vec<A>> 
            where A: TryFrom<PathBuf>
    {
        read_dir(d)?
            .map_results(|e|e.path())
            .map_results(A::try_from)
            .collect::<Result<_>>()?

    }
  
    fn validate(self) -> io::Result<Config> {
        Ok(Config {
            solvers: Self::lines_to_files(&self.indir.join("solvers"))?,
            benchmarks: Self::lines_to_files(&self.indir.join("benchmarks"))?,
            timeout: self.timeout,
            outdir: self.outdir,
        })
    }
}

#[derive(Debug)]
struct Benchmark {
    file: PathBuf,
}

impl<'a> fmt::Display for BenchConf<'a> {
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
    fn id(&self) -> &PathBuf { &self.file/*.file_name().unwrap()*/ }
}

impl TryFrom<PathBuf> for Benchmark {
    fn try_from(file: PathBuf) -> io::Result<Self> {
        Ok(Benchmark{ file })
    }
}

#[derive(Debug)]
struct Solver {
    bin: PathBuf,
}

impl fmt::Display for Solver {
     fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
         self.bin.display().fmt(w)
     }
}

impl Solver {
    fn id(&self) -> &OsStr { &self.bin.file_name().unwrap() }
}

impl TryFrom<PathBuf> for Solver {
    fn try_from(bin: PathBuf) -> io::Result<Self> {
        Ok(Solver{ bin })
    }
}

struct BenchConf<'a> {
    benchmark: &'a Benchmark,
    solver: &'a Solver,
}

#[derive(Debug)]
struct Config {
    solvers: Vec<Solver>,
    benchmarks: Vec<Benchmark>,
    outdir:  PathBuf,
    timeout: usize,
}

impl Config {
    fn outdir(&self, conf: &BenchConf) -> PathBuf {
        let mut path = PathBuf::from(&self.outdir);
        path.push(conf.solver.id());
        path.push(format!("{}", self.timeout));
        path.push(conf.benchmark.id());
        path
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
        Ui {
            bar
        }
    }

    fn println(&self, m: impl std::fmt::Display) {
        self.bar.println(m.to_string());
    }

    fn progress(&self) {
        self.bar.inc(1);
    }

}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {

    let opts = Opts::from_args().validate()?;

    println!("output dir: {}", opts.outdir.display());
    println!("cpus: {}", num_cpus::get_physical());

    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get_physical())
        .build_global()?;

    let todo = opts.benchmarks[..].par_iter()
        .flat_map(|benchmark| opts.solvers[..].par_iter()
            .map(move |solver| BenchConf {benchmark,solver}))
        .progress()
        .filter(|c|!opts.outdir(c).exists())
        .collect::<Vec<_>>();

    let ui = Ui::new("Benchmarking", todo.len());

    &todo[..].par_iter()
        .for_each(|conf| {
            match run(&ui, &opts, &conf) {
                Ok(()) => (),
                Err(e) => ui.println(format!("run failed: {}", e)),
            }
            ui.progress();
    });
    Ok(())
}


fn run(ui: &Ui, opts: &Config, conf: &BenchConf) -> Result<()> {

    let solver = &conf.solver;
    let benchmark = &conf.benchmark;

    let dir = opts.outdir(conf);

    if dir.exists() {
        // println!("skipping {} {}", solver, benchmark);
        Ok(())
    } else {
        create_dir_all(&dir)?;

        let ofile = |n| {
            let f = dir.join(n);
            File::create(f)
        };

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

        let result = cmd!(&solver.bin, &benchmark.file, format!("{}", opts.timeout))
            .stdout(ofile("stdout")?)
            .stderr(ofile("stderr")?)
            .output()?;

        if result.status.success() {
            Ok(())
        } else {
            let err_dir = dir.with_extension("err");
            if err_dir.exists() {
                remove_dir_all(&err_dir)?;
            }
            rename(&dir, &err_dir)?;
            let msg = format!("unexpected exit status (code: {:?}, output in dir: {})", result.status.code(), err_dir.display());
            Err(io::Error::new(io::ErrorKind::Other, msg))
        }

    }
}

