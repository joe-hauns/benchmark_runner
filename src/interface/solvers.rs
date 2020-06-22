use crate::*;
use std::ffi::*;
use std::path::*;
use std::fmt;
use std::convert::TryFrom;
use serde::*;
use anyhow::Result;
use std::process::*;
use crate::interface::ids::*;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Script {
    pub(crate) id: String,
    pub(crate) file: PathId,
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


impl TryFrom<PathBuf> for Script {
    type Error = anyhow::Error;
    fn try_from(file: PathBuf) -> Result<Self> {
        //TODO check if it's a file, and if it's executable
        FromDir::from_dir(file)
        // Ok(Script { file: FromDir::from_dir(file)? })
    }
}


impl Ident for Script {
    type Id = String;
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl<P> Solver<P> for Script 
    where P: Benchmarker<Benchmark=PathId>,
{
    fn to_command(&self, benchmark: &PathId, timeout: &Duration) -> std::process::Command {
        let mut cmd = Command::new(self.command().as_ref());
        cmd.args(self.args(benchmark, &timeout));
        cmd
    }

    fn show_command(&self, benchmark: &PathId, timeout: &Duration) -> String {
        let mut w = self.command().as_ref().display().to_string();
        for arg in self.args(benchmark, timeout) {
            w.push(' ');
            w.push_str(&arg.to_string());
        }
        w
    }
}

impl Script {

    pub fn command<'a>(&'a self) -> &'a PathId {
        &self.file
    }

    pub fn args<'a>(&'a self, benchmark: &'a PathId, timeout: &'a Duration) -> impl IntoIterator<Item = impl AsRef<OsStr> + fmt::Display + 'a> + 'a {
        use std::iter::once;
        once(Args::PathBuf(&benchmark.as_ref()))
            .chain(once(Args::TimeOut(format!("{}", timeout.as_secs()))))
    }


}


impl FromDir for Script {
    fn from_dir<P>(file: P) -> Result<Self> 
        where P: AsRef<Path>,
    {
        let file = file.as_ref();
        match file.file_name().and_then(|x|x.to_str()) {
            Some(id) => Ok(Script { id: id.to_owned(),  file: FromDir::from_dir(file)? }),
            None => bail!("script has no file name, or conains invalid characters")
        }

    }
}
