use super::*;
use std::result::Result;
use super::Error;
use log::*;
use atty::Stream;

struct ServiceImpl {
    conf: ServiceConfig,
}

//TODO get rid of this clone
pub(crate) fn create(conf: ServiceConfig) -> anyhow::Result<impl Service> {
    Ok(ServiceImpl { conf })
}

pub(crate) trait Service {
    fn run<D, P>(&self, job: JobConfig<P>, dao: &D, post: &P) -> Result<P::Reduced, Error>
    where
        D: Dao<P> + Sync,
        P: Benchmarker + Sync;
        // <P as Benchmarker>::BAnnot: Clone;
    fn run_single<P>(&self, ident: &BenchRunConf<P>) -> Result<BenchRunResult<P>, Error>
    where
        P: Benchmarker + Sync;
}

impl Service for ServiceImpl {
    fn run_single<P>(&self, conf: &BenchRunConf<P>) -> Result<BenchRunResult<P>, Error>
    where
        P: Benchmarker + Sync,
        // <P as Benchmarker>::BAnnot: Clone,
    {
        tprintln!("Running: {}...", conf.display_command());
        let out = run_command(&conf);
        tprintln!("Finished.");
        out
    }

    fn run<D, P>(&self, job: JobConfig<P>, dao: &D, post: &P) -> Result<P::Reduced, Error>
    where
        D: Dao<P> + Sync,
        P: Benchmarker + Sync,
        // <P as Benchmarker>::BAnnot: Clone,
    {
        let job = &Arc::new(job);

        setup_ctrlc();
        log_err_!(
            set_thread_cnt(
                self.conf
                    .threads
                    .unwrap_or_else(|| num_cpus::get_physical())
            ),
            "failed to set number of threads"
        );

        let (mut done, todo): (Vec<_>, Vec<_>) = {
            let bs = &job.benchmarks[..];
            let cs = &job.solvers[..];
            let ui = Ui::new("Reading old results", bs.len() * cs.len());
            // let config = &config;
            bs.par_iter()
                .flat_map(move |benchmark| {
                    cs.par_iter().map(move |solver| BenchRunConf {
                        timeout: job.timeout,
                        benchmark: benchmark.clone(),
                        solver: solver.clone(),
                    })
                })
                .filter(|_| !shall_terminate())
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

        let remove_files = |ui: &Ui, conf: &BenchRunConf<P>, reason: FormatArgs| {
            eprintln!("error: {}", reason);
            match dao.remove_result(&conf, reason)
            {
                Ok(()) => ui.println(format_args!("removed output files for {} {}", conf.solver().id(), conf.benchmark().id())),
                Err(e) => ui.println(format_args!("failed to remove output files: {:#}", e)),
            }
        };

        {
            let ui = Ui::new("Benchmarking", todo.len());
            done.par_extend(todo[..].into_par_iter().filter_map(|conf| {
                if shall_terminate() {
                    None
                } else {
                    let result = match run_command(&conf) {
                        Ok(x) => {
                            if let Err(e) = dao.store_result(&x) {
                                eprintln!("failed to store result: {:#}", e);
                            }
                            Some(x)
                        }
                        Err(Error::TermSignal(TermSignal)) => None,
                        Err(e) => {
                            remove_files(&ui, &conf, format_args!("failed to run {}: {:#}", conf.display_command(), e));
                            None
                        }
                    };
                    ui.progress();
                    result
                }
            }));
        }

        if shall_terminate() {
            return Err(Error::TermSignal(TermSignal));
        }

        let mapped: Vec<_> = {
            // TODO postprocess directly after benchmarking to get online warnings when runs are failing
            let ui = Ui::new("Postprocessing", done.len());

            done.par_iter()
                .filter_map(|x| {
                    Some({
                        let res = match post.map(&x) {
                            Ok(mapped) => (x.clone(), mapped),
                            Err(e) => {
                                remove_files(
                                    &ui,
                                    &x.run,
                                    format_args!("failed to prostprocess: {:#}", e),
                                );
                                return None;
                            }
                        };
                        ui.progress();
                        if shall_terminate() {
                            Err(TermSignal)
                        } else {
                            Ok(res)
                        }
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        //TODO store this via dto
        let reduced = post.reduce(&job, mapped)?;

        if shall_terminate() {
            return Err(Error::TermSignal(TermSignal));
        }

        // let dir = config.postpro_dir()?;
        // fs::create_dir_all(&dir)
        //     .with_context(|| format!("failed to create directory: {}", dir.display()))?;
        // println!("writing to output dir: {}", dir.display());

        // TODO serialize summary to file
        reduced.write_summary(std::io::stdout().lock())?;
        Ok(reduced)

    }
}

fn run_command<P>(run: &BenchRunConf<P>) -> Result<BenchRunResult<P>, Error>
where
    P: Benchmarker,
{
    info!("running: {}", run.display_command());

    // let mut cmd = Command::new(run.command());
    // cmd.args(run.args());
    let mut cmd = run.to_command();
    let temp_dir = tempfile::tempdir().context("failed to create temp dir")?;
    let temp_dir = temp_dir.path();
    let stdout = temp_dir.join("stdout.txt");
    let stderr = temp_dir.join("stderr.txt");
    cmd.stdout(crate::dao::create_file(&stdout)?);
    cmd.stderr(crate::dao::create_file(&stderr)?);
    let mut child =cmd.spawn().context("failed to launch child process")?;


    use std::time::*;

    let start = Instant::now();
    // TODO make poll timeout relative to timeout of benchmark
    let poll = Duration::from_millis(500);
    loop {
        let status = child
            .wait_timeout(poll)
            .context("failed to wait for child process")?;

        let with_bench_status = | exit_status: Option<i32>,
                                 status: BenchmarkStatus|
         -> Result<BenchRunResult<P>, Error> {
            
            let stdout = crate::dao::read_vec(&stdout)?;
            let stderr = crate::dao::read_vec(&stderr)?;

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
                    with_bench_status(status.code(), BenchmarkStatus::Success)
                }
            }
            None => {
                if shall_terminate() {
                    child.kill().context("failed to kill child process")?;
                    return Err(TermSignal)?;
                }
                if start.elapsed() > run.timeout.mul_f64(1.2) {
                    child.kill().context("failed to kill child process")?;
                    return with_bench_status(None, BenchmarkStatus::Timeout);
                }
            }
        }
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

fn set_thread_cnt(n: usize) -> anyhow::Result<()> {
    if atty::is(Stream::Stdout) {
        println!("using {} threads", num_cpus::get_physical());
    }
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
