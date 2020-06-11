use crate::*;
use std::ffi::*;
use std::path::*;
use std::fmt;
use std::convert::TryFrom;
use serde::*;
use anyhow::Result;
use std::process::*;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Script {
    pub(crate) file: PathId,
}

impl fmt::Display for PathId {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        self.0.display().fmt(w)
    }
}

enum Args<'a> {
    PathBuf(&'a PathBuf),
    TimeOut(String),
}
impl<'a> AsRef<OsStr> for Args<'a> {
    fn as_ref(&self) -> &OsStr {
        match self {
            Args::PathBuf(p) => p.as_ref(),
            Args::TimeOut(t) => t.as_ref(),
        }
    }
}
impl<'a> fmt::Display for Args<'a> {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Args::PathBuf(p) => p.display().fmt(w),
            Args::TimeOut(t) => t.fmt(w),
        }
    }
}



// impl Script {
//     pub fn file(&self) -> &OsStr {
//         &self.file.file_name().unwrap()
//     }
// }


impl TryFrom<PathBuf> for Script {
    type Error = anyhow::Error;
    fn try_from(file: PathBuf) -> Result<Self> {
        //TODO check if it's a file, and if it's executable
        Ok(Script { file: PathId(file) })
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct PathId(PathBuf);

impl AsRef<PathBuf> for PathId {
    fn as_ref(&self)-> &PathBuf {
        &self.0
    }
}

impl Solver<Annotated<Benchmark, ()>> for Script {
    type Id = PathId;
    fn id(&self) -> &Self::Id {
        &self.file
    }

    fn to_command(&self, benchmark: &Annotated<Benchmark, ()>, timeout: &Duration) -> std::process::Command {
        let mut cmd = Command::new(self.command());
        cmd.args(self.args(benchmark, &timeout));
        cmd
    }

    fn show_command(&self, benchmark: &Annotated<Benchmark, ()>, timeout: &Duration) -> String {
        let mut w = self.command().display().to_string();
        for arg in self.args(benchmark, timeout) {
            w.push(' ');
            w.push_str(&arg.to_string());
        }
        w
    }
}

impl Script {

    pub fn command<'a>(&'a self) -> &'a PathBuf {
        &self.file.0
    }

    pub fn args<'a>(&'a self, benchmark: &'a Annotated<Benchmark, ()>, timeout: &'a Duration) -> impl IntoIterator<Item = impl AsRef<OsStr> + fmt::Display + 'a> + 'a {
        use std::iter::once;
        once(Args::PathBuf(&benchmark.file))
            .chain(once(Args::TimeOut(format!("{}", timeout.as_secs()))))
    }


}


impl FromDir for Script {
    fn from_dir<P>(file: P) -> Result<Self> 
        where P: AsRef<Path>,
    {
        Ok(Script { file: PathId(file.as_ref().canonicalize().context("failed to canonicalize")?.into()) })
    }
}

// impl FromDir for Script {
//     fn from_dir<P>(basedir: P) -> Result<Self> 
//         where P: AsRef<Path>,
//     {
//         let basedir: &Path = basedir.as_ref();
//         //TODO check if it's a file, and if it's executable
//         if !basedir.is_dir() {
//             bail!("solver is not a directory")
//         }
//         let launchers = basedir.join("launchers");
//         if !launchers.exists() || !launchers.is_dir() {
//             bail!("solver does not contain a launchers directory")
//         }
//         let launchers = process_results(read_dir(&launchers)?, |launchers| {
//             launchers
//                 .map(|l| Script { script: l.path() })
//                 .collect()
//         })?;
//
//         Ok(Script { basedir: basedir.to_owned(), launchers })
//     }
// }
