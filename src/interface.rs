pub mod solvers;

use super::*;
use dto::*;
use serde::*;
use std::hash::*;
use std::cmp::*;
use std::fmt::*;
use anyhow::Result;

pub trait Summerizable {
    fn write_summary<W>(&self, out: W) -> Result<()>
    where
        W: io::Write;
}

pub trait Postprocessor {
    type Solver: Solver<Annotated<Benchmark, Self::BAnnot>>;
    type Mapped: Send + Sync;
    type Reduced: Serialize + DeserializeOwned + Summerizable + Sized;
    /// Benchmark Annotation
    type BAnnot: PartialOrd + PartialEq + Eq + Hash + Ord + Debug + Clone + Serialize + DeserializeOwned + Send + Sync + Sized;

    fn annotate_benchark(&self, b: &Benchmark) -> Result<Self::BAnnot>;
    fn map(&self, r: &BenchRunResult<Self>) -> Result<Self::Mapped>;
    fn reduce(
        &self,
        job: &JobConfig<Self>,
        iter: impl IntoIterator<Item = (BenchRunConf<Self>, Self::Mapped)>,
    ) -> Result<Self::Reduced>;

}

pub trait Solver<B>:  Clone + Debug+ Hash+ Ord+ PartialOrd + Eq + PartialEq + Serialize + DeserializeOwned + Sized + Send + Sync {
    type Id: std::fmt::Display;
    fn id(&self) -> &Self::Id;
    fn to_command(&self, benchmark: &B, timeout: &Duration) -> std::process::Command;
    fn show_command(&self, benchmark: &B, timeout: &Duration) -> String;
}
