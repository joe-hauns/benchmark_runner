use super::*;
use std::result::Result;
use super::Error;

struct ServiceImpl {
    conf: ServiceConfig,
}

//TODO get rid of this clone
pub(crate) fn create(conf: ServiceConfig) -> anyhow::Result<impl Service> {
    Ok(ServiceImpl { conf })
}

pub(crate) trait Service {
    fn run<D, P>(&self, job: JobConfig<P::BAnnot>, dao: &D, post: &P) -> Result<P::Reduced, Error>
    where
        D: Dao<P::BAnnot> + Sync,
        P: Postprocessor + Sync,
        <P as Postprocessor>::BAnnot: Clone;
}

impl Service for ServiceImpl {
    fn run<D, P>(&self, job: JobConfig<P::BAnnot>, dao: &D, post: &P) -> Result<P::Reduced, Error>
    where
        D: Dao<P::BAnnot> + Sync,
        P: Postprocessor + Sync,
        <P as Postprocessor>::BAnnot: Clone,
    {
        let job = &Arc::new(job);

        // println!("output dir: {}", config.dao.outdir.display());
        println!("cpus: {}", num_cpus::get_physical());

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
                        job: job.clone(),
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

        let remove_files =
            |ui: &Ui, conf: &BenchRunConf<P::BAnnot>, reason: FormatArgs| match dao
                .remove_result(&conf, reason)
            {
                Ok(()) => ui.println(format_args!("removed output files for {}", conf)),
                Err(e) => ui.println(format_args!("failed to remove output files: {:#}", e)),
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
            return Err(Error::TermSignal(TermSignal));
        }

        let mapped: Vec<_> = {
            // TODO postprocess directly after benchmarking to get online warnings when runs are failing
            let ui = Ui::new("Postprocessing", done.len());

            done.par_iter()
                .filter_map(|x| {
                    Some({
                        let res = match post.map(&x) {
                            Ok(mapped) => (x.run.clone(), mapped),
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
