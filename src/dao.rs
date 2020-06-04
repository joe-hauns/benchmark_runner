use super::*;
use std::io::*;
use anyhow::Result;

pub(crate) fn create(opts: &Opts) -> Result<impl Dao> {
    let outdir = opts.outdir.clone();
    create_dir_all(&outdir)
        .with_context(|| format!("failed to create outdir: {}", outdir.display()))?;
    Ok(DaoImpl {
        outdir,
    })
}

pub(crate) trait Dao {
    fn store_result(&self, run: &BenchRunResult) -> Result<()>;
    fn read_result(&self, run: &BenchRunConf) -> Result<Option<BenchRunResult>>;
    fn remove_result<R: std::fmt::Display>(&self, run: &BenchRunConf, reason: R) -> Result<()>;
}

#[derive(Serialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct BenchRunResultMeta<'a> {
    run: &'a BenchRunConf,
    status: &'a BenchmarkStatus,
    time: &'a Duration,
    exit_status: &'a Option<i32>,
}

#[derive(Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct BenchRunResultMetaOwned {
    run: BenchRunConf,
    status: BenchmarkStatus,
    time: Duration,
    exit_status: Option<i32>,
}
    //TODO ensure thread safety

pub struct DaoImpl {
    outdir: PathBuf,
}

impl DaoImpl {
    fn outdir(&self, run: &BenchRunConf) -> PathBuf {
        let mut path = PathBuf::from(&self.outdir);
        path.push(run.solver.file.file_name().unwrap());
        path.push(format!("{}", run.job.timeout.as_secs()));
        path.push(run.benchmark.file.file_name().unwrap());
        path
    }
    fn meta_json(&self, run: &BenchRunConf) -> PathBuf {
        self.outdir(run).join("meta.json")
    }
    fn stdout_txt(&self, run: &BenchRunConf) -> PathBuf {
        self.outdir(run).join("stdout.txt")
    }
    fn stderr_txt(&self, run: &BenchRunConf) -> PathBuf {
        self.outdir(run).join("stderr.txt")
    }
}

impl Dao for DaoImpl {
    fn remove_result<R: std::fmt::Display>(&self, run: &BenchRunConf, reason: R) -> Result<()> {
        let dir = self.outdir(run);
        let err_dir = dir.with_extension("err");
        if err_dir.exists() {
            remove_dir_all(&err_dir)
                .with_context(|| format!("failed to remove directory: {}", err_dir.display()))?;
        }
        rename(&dir, &err_dir)
            .with_context(|| format!("failed move dir {} -> {}", dir.display(), err_dir.display()) )?;
        // ui.println(&reason);
        // ui.println(format!(
        //     "moving result to {} (may be deleted in another run)",
        //     err_dir.display()
        // ));
        let reason_file = err_dir.join("error_reason.txt");
        if let Err(e) = File::create(reason_file)
                .and_then(|mut reason_file| write!(reason_file, "{}", reason)) {
            eprintln!("failed to write reason for file removal: {}", e);
            eprintln!("reason was: {}", reason);
        }
        Ok(())

    }
    fn store_result(&self, run: &BenchRunResult) -> Result<()> {
        let BenchRunResult {
            run,
            status,
            time,
            exit_status,
            stdout,
            stderr,
        } = run;

        let outdir = self.outdir(run);
        create_dir_all(&outdir)
            .with_context(|| format!("failed to craete dir: {}", outdir.display()))?;

        write_json(
            self.meta_json(run),
            &BenchRunResultMeta {
                run,
                status,
                time,
                exit_status,
            },
        )?;
        write_vec(&self.stdout_txt(run), stderr)?;
        write_vec(&self.stderr_txt(run), stdout)?;
        Ok(())
    }

    fn read_result(&self, run: &BenchRunConf) -> Result<Option<BenchRunResult>> {
        let outdir = self.outdir(run);
        if !outdir.exists() {
            return Ok(None);
        }
        let BenchRunResultMetaOwned {
            run,
            status,
            time,
            exit_status,
        } = read_json(self.meta_json(run))?;

        let stdout = read_vec(&self.stdout_txt(&run))?;
        let stderr = read_vec(&self.stderr_txt(&run))?;

        Ok(Some(BenchRunResult {
            run,
            status,
            time,
            exit_status,
            stdout,
            stderr,
        }))
    }
}

fn read_vec(path: &PathBuf) -> Result<Vec<u8>> {
    read(path)
        .with_context(||format!("failed to read file: {}", path.display()))
}

fn write_vec(path: &PathBuf, vec: &Vec<u8>) -> Result<()> {
    write(path, vec)
        .with_context(||format!("failed to write file: {}", path.display()))
}

fn write_json<A: Serialize>(path: PathBuf, value: &A) -> Result<()> {
    let file =
        File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(serde_json::to_writer_pretty(file, &value)
        .with_context(|| format!("failed to write json to {}", path.display()))?)
}

fn read_json<A: DeserializeOwned>(f: PathBuf) -> Result<A> {
    Ok(serde_json::from_reader(
        File::open(&f).with_context(|| format!("failed to open {}", f.display()))?,
    )
    .with_context(|| format!("failed to read {}", f.display()))?)
}
