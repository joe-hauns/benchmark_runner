use super::*;
use std::fmt;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct PathId(PathBuf);

impl AsRef<PathBuf> for PathId {
    fn as_ref(&self)-> &PathBuf {
        &self.0
    }
}

impl Ident for PathId {
    type Id = Self;
    fn id(&self) -> &Self::Id {
        &self
    }
}


impl Benchmark for PathId { }

impl FromDir for PathId {
    fn from_dir<P: AsRef<Path>>(file: P) -> Result<PathId> {
        Ok(PathId(file.as_ref().canonicalize().context("failed to canonicalize")?.into()))
    }
}

impl fmt::Display for PathId {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        write!(w, "{}", self.0.display())
    }
}
