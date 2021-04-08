use super::*;
use anyhow::Result;
use crate::interface::Ident;
use std::fs;
use log::*;

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
        //TODO make this clean
        macro_rules! id_to_path {
            ($x: expr) =>  { {
                let id = PathBuf::from(format!("{}", $x));
                id.file_name().unwrap().to_owned()
            }}
        }
        PathBuf::from(&self.outdir)
            .join(id_to_path!(Ident::id(run.solver.as_ref())))
            .join(format!("{}", run.timeout.as_secs()))
            .join(id_to_path!(run.benchmark.id()))
    }

    fn meta_json<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where
        P: Benchmarker
    {
        self.outdir(run).join("meta.json")
    }

    fn pwd_dir<P>(&self, run: &BenchRunConf<P>) -> PathBuf
    where
        P: Benchmarker
    {
        self.outdir(run).join("pwd")
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
        info!("removing result {} (reason: {})", run, reason);
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
            write_vec(create_file(&reason_file)?, format!("{}", reason).as_ref())
            // create_file(reason_file).and_then(|mut reason_file| write!(reason_file, "{}", reason).co?)
        {
            eprintln!("failed to write reason for file removal: {}", e);
            eprintln!("reason was: {}", reason);
        }
        Ok(())
    }

    fn store_result(&self, run: &BenchRunResult<P>) -> Result<()> {
        info!("storing result {:?}", run);
        let BenchRunResult {
            run,
            status,
            time,
            exit_status,
            stdout,
            stderr,
            files,
        } = run;

        let outdir = self.outdir(run);
        create_dir_all(&outdir)?;

        write_json(
            create_file(self.meta_json(run))?,
            &BenchRunResultMeta {
                run,
                status,
                time,
                exit_status,
            },
        )?;
        write_vec(create_file(&self.stdout_txt(run))?, stdout)?;
        write_vec(create_file(&self.stderr_txt(run))?, stderr)?;
        let pwd = self.pwd_dir(&run);
        for f in files {
            let path = pwd.join(&f.name);
            std::fs::create_dir_all(path.parent().unwrap())?;
            write_vec(create_file(&path)?, &f.bytes)?;
        }
        Ok(())
    }

    fn read_result(&self, run: &BenchRunConf<P>) -> Result<Option<BenchRunResult<P>>> {
        info!("reading result {}", run);
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
        let pwd = self.pwd_dir(&run);
        let files = 
            if pwd.exists() {
                read_file_conts(&pwd)?
            } else {
                vec![] // for backwards compability
            };

        Ok(Some(BenchRunResult {
            run,
            status,
            time,
            exit_status,
            stdout,
            stderr,
            files ,
        }))
    }
}

pub(crate) fn read_file_conts(dir: impl AsRef<Path>) -> Result<Vec<FileConts>> {
    let dir = dir.as_ref();
    let res = read_dir_rec(dir, |path|  
        Ok(FileConts {
            bytes: crate::dao::read_vec(&path)?,
            name: path.strip_prefix(&dir).unwrap().to_owned(),
        }));
    res 
}

// TODO move all these function to own module `fs`
pub fn read_dir_rec<P, F, A, E>(dir: P, mut f: F) -> Result<Vec<A>, E>
    where P: AsRef<Path>,
          F: FnMut(PathBuf) -> Result<A, E>,
          E: From<std::io::Error>,
{
    let mut vec = Vec::new();
    fn read_dir_rec<P, F, A, E>(vec: &mut Vec<A>, dir: P, func: &mut F) -> Result<(), E>
        where P: AsRef<Path>,
              F: FnMut(PathBuf) -> Result<A, E>,
              E: From<std::io::Error>,
    {
        for f in std::fs::read_dir(dir)? {
            let f = f?;
            let path = f.path();
            if path.is_dir() {
                read_dir_rec(vec, path, func)?;
            } else {
                vec.push(func(path)?);
            }
        }
        Ok(())
    }
    read_dir_rec(&mut vec, dir, &mut f)?;
    Ok(vec)
}

// TODO move all these function to own module `fs`
pub fn read_vec(path: &PathBuf) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("failed to read file: {}", path.display()))
}

fn write_vec(mut file: fs::File, vec: &[u8]) -> Result<()> {
    use std::io::Write;
    // fs::write(path, vec).with_context(|| format!("failed to write file: {}", path.display()))
    file.write_all(vec).context("failed to write file")
}

pub fn write_json<A: Serialize>(file: fs::File, value: &A) -> Result<()> {
    // let file =
    //     create_file(&path).with_context(|| format!("failed to create {}", path.display()))?;
    Ok(serde_json::to_writer_pretty(file, &value)
        .context("failed to write json")?)
}

// pub fn write_json<A: Serialize>(path: PathBuf, value: &A) -> Result<()> {
//     let file =
//         create_file(&path).with_context(|| format!("failed to create {}", path.display()))?;
//     Ok(serde_json::to_writer_pretty(file, &value)
//         .with_context(|| format!("failed to write json to {}", path.display()))?)
// }

pub fn read_json<A: DeserializeOwned, P: AsRef<Path>>(f: P) -> Result<A> {
    let f = f.as_ref();
    Ok(serde_json::from_reader(
        open_file(&f).with_context(|| format!("failed to open json '{}'", f.display()))?,
    )
    .with_context(|| format!("failed to read json '{}'", f.display()))?)
}

pub fn create_file<P>(p: P) -> Result<fs::File> 
    where P: AsRef<Path>
{
    let p = p.as_ref();
    fs::File::create(p)
        .with_context(||format!("failed to create file '{}'", p.display()))
}

pub fn open_file<P>(p: P) -> Result<fs::File> 
    where P: AsRef<Path>
{
    let p = p.as_ref();
    fs::File::open(p)
        .with_context(||format!("failed to open file '{}'", p.display()))
}


pub fn remove_dir_all<P>(p: P) -> Result<()> 
    where P: AsRef<Path> 
{
    let p = p.as_ref();
    fs::remove_dir_all(p)
        .with_context(||format!("failed to remove dir '{}'", p.display()))
}

pub fn create_dir_all<P>(p: P) -> Result<()> 
    where P: AsRef<Path> 
{
    let p = p.as_ref();
    fs::create_dir_all(p)
        .with_context(||format!("failed to create dirs '{}'", p.display()))
}

pub fn read_dir<'a, P>(path: &'a P) -> Result<impl Iterator<Item = Result<DirEntry>> + 'a>
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

// pub fn read_dir<P>(p: P) -> Result<fs::ReadDir> 
//     where P: AsRef<Path> 
// {
//     let p = p.as_ref();
//     fs::read_dir(p)
//         .with_context(||format!("failed to read dir '{}'", p.display()))
// }

fn rename<P, Q>(p: P, q: Q) -> Result<()> 
    where P: AsRef<Path> ,
          Q: AsRef<Path> ,
{
    let p = p.as_ref();
    let q = q.as_ref();
    fs::rename(p, q)
        .with_context(||format!("failed to rename '{}' to '{}'", p.display(), q.display()))
}

