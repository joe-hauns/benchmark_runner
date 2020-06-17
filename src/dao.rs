use super::*;
use anyhow::Result;
use crate::interface::Ident;
use std::fs;

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct DaoConfig {
    pub outdir: PathBuf
}

pub(crate) fn create<P>(conf: DaoConfig) -> Result<impl Dao<P>>
where P: Benchmarker,
{
    //TODO get rid of this clone
    let outdir = conf.outdir;
    create_dir_all(&outdir)?;
    Ok(DaoImpl { outdir })
}

pub(crate) trait Dao<P> 
where P: Benchmarker
{
    fn store_result(&self, run: &BenchRunResult<P>) -> Result<()>;
    fn read_result(&self, run: &BenchRunConf<P>) -> Result<Option<BenchRunResult<P>>>;
    fn remove_result<R: std::fmt::Display>(&self, run: &BenchRunConf<P>, reason: R) -> Result<()>;
}


#[derive(Serialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
// #[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct BenchRunResultMeta<'a, P> 
where P: Benchmarker,
{
    #[serde(bound(serialize = "BenchRunConf<P>: Serialize"))]
    run: &'a BenchRunConf<P>,
    status: &'a BenchmarkStatus,
    time: &'a Duration,
    exit_status: &'a Option<i32>,
}

// impl<'a,P> serde::Serialize for BenchRunResultMeta<'a, P> 
//     where P: Benchmarker,
// {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         use serde::ser::*;
//         let mut map = serializer.serialize_map(Some(4))?;
//         map.serialize_entry("run", self.run)?;
//         map.serialize_entry("status", self.status)?;
//         map.serialize_entry("time", self.time)?;
//         map.serialize_entry("exit_status", self.exit_status)?;
//         map.end()
//     }
// }

#[derive(Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct BenchRunResultMetaOwned<P> 
where P: Benchmarker,
{
    #[serde(bound(deserialize = "BenchRunConf<P>: DeserializeOwned"))]
    run: BenchRunConf<P>,
    status: BenchmarkStatus,
    time: Duration,
    exit_status: Option<i32>,
}
//TODO ensure thread safety

pub struct DaoImpl {
    outdir: PathBuf,
}

impl DaoImpl {
    fn outdir<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where P: Benchmarker,
          // <P as Benchmarker>::Solver: Ident,
          // <P as Benchmarker>::Benchmark: Ident,
    {
        let mut path = PathBuf::from(&self.outdir);
        path.push(format!("{}", Ident::id(run.solver.as_ref())));
        path.push(format!("{}", run.job.timeout.as_secs()));
        // path.push(run.benchmark.file.file_name().unwrap());
        path.push(format!("{}", run.benchmark.id()));
        path
    }
    fn meta_json<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where
        P: Benchmarker
    {
        self.outdir(run).join("meta.json")
    }

    fn stdout_txt<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where
        P: Benchmarker
    {
        self.outdir(run).join("stdout.txt")
    }

    fn stderr_txt<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where
        P: Benchmarker
    {
        self.outdir(run).join("stderr.txt")
    }
}

impl<P> Dao<P> for DaoImpl
where
    P: Benchmarker
{
    fn remove_result<R: std::fmt::Display>(&self, run: &BenchRunConf<P>, reason: R) -> Result<()> {
        let dir = self.outdir(run);
        let err_dir = dir.with_extension("err");
        if err_dir.exists() {
            remove_dir_all(&err_dir)?;
        }
        rename(&dir, &err_dir)?;
        // ui.println(&reason);
        // ui.println(format!(
        //     "moving result to {} (may be deleted in another run)",
        //     err_dir.display()
        // ));
        let reason_file = err_dir.join("error_reason.txt");
        if let Err(e) =
            write_vec(&reason_file, format!("{}", reason).as_ref())
            // create_file(reason_file).and_then(|mut reason_file| write!(reason_file, "{}", reason).co?)
        {
            eprintln!("failed to write reason for file removal: {}", e);
            eprintln!("reason was: {}", reason);
        }
        Ok(())
    }
    fn store_result(&self, run: &BenchRunResult<P>) -> Result<()> {
        let BenchRunResult {
            run,
            status,
            time,
            exit_status,
            stdout,
            stderr,
        } = run;

        let outdir = self.outdir(run);
        create_dir_all(&outdir)?;

        write_json(
            self.meta_json(run),
            &BenchRunResultMeta {
                run,
                status,
                time,
                exit_status,
            },
        )?;
        write_vec(&self.stdout_txt(run), stdout)?;
        write_vec(&self.stderr_txt(run), stderr)?;
        Ok(())
    }

    fn read_result(&self, run: &BenchRunConf<P>) -> Result<Option<BenchRunResult<P>>> {
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
    fs::read(path).with_context(|| format!("failed to read file: {}", path.display()))
}

fn write_vec(path: &PathBuf, vec: &[u8]) -> Result<()> {
    fs::write(path, vec).with_context(|| format!("failed to write file: {}", path.display()))
}

pub fn write_json<A: Serialize>(path: PathBuf, value: &A) -> Result<()> {
    let file =
        create_file(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(serde_json::to_writer_pretty(file, &value)
        .with_context(|| format!("failed to write json to {}", path.display()))?)
}

pub fn read_json<A: DeserializeOwned, P: AsRef<Path>>(f: P) -> Result<A> {
    Ok(serde_json::from_reader(
        create_file(&f).with_context(|| format!("failed to open {}", f.as_ref().display()))?,
    )
    .with_context(|| format!("failed to read {}", f.as_ref().display()))?)
}

fn create_file<P>(p: P) -> Result<fs::File> 
    where P: AsRef<Path>
{
    let p = p.as_ref();
    fs::File::create(p)
        .with_context(||format!("failed to create file {}", p.display()))
}


fn remove_dir_all<P>(p: P) -> Result<()> 
    where P: AsRef<Path> 
{
    let p = p.as_ref();
    fs::remove_dir_all(p)
        .with_context(||format!("failed to remove dir {}", p.display()))
}

fn create_dir_all<P>(p: P) -> Result<()> 
    where P: AsRef<Path> 
{
    let p = p.as_ref();
    fs::create_dir_all(p)
        .with_context(||format!("failed to create dirs {}", p.display()))
}

fn rename<P, Q>(p: P, q: Q) -> Result<()> 
    where P: AsRef<Path> ,
          Q: AsRef<Path> ,
{
    let p = p.as_ref();
    let q = q.as_ref();
    fs::rename(p, q)
        .with_context(||format!("failed to rename {} to {}", p.display(), q.display()))
}

