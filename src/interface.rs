pub mod solvers;
pub mod ids;

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

pub trait Benchmarker {
    type Solver: Solver<Self>;
    type Benchmark: Benchmark;

    type Mapped: Send + Sync;
    fn map(&self, r: &BenchRunResult<Self>) -> Result<Self::Mapped>;

    type Reduced: Serialize + DeserializeOwned + Summerizable + Sized;
    fn reduce(
        &self,
        job: &JobConfig<Self>,
        iter: impl IntoIterator<Item = (BenchRunResult<Self>, Self::Mapped)>,
    ) -> Result<Self::Reduced>;

}

pub trait Ident {
    type Id: std::fmt::Display;
    fn id(&self) -> &Self::Id;
}

pub trait Benchmark:  Ident + Clone + Debug+ Hash+ Ord+ PartialOrd + Eq + PartialEq + Serialize + DeserializeOwned + Sized + Send + Sync {
    // type Id: std::fmt::Display;
    // fn id(&self) -> &Self::Id;
    // fn to_command(&self, benchmark: &B, timeout: &Duration) -> std::process::Command;
    // fn show_command(&self, benchmark: &B, timeout: &Duration) -> String;
}

pub trait Solver<P>: Ident + Clone + Debug+ Hash+ Ord+ PartialOrd + Eq + PartialEq + Serialize + DeserializeOwned + Sized + Send + Sync 
    where P: Benchmarker + ?Sized
{
    // type Id: std::fmt::Display;
    // fn id(&self) -> &Self::Id;
    fn to_command(&self, benchmark: &P::Benchmark, timeout: &Duration) -> std::process::Command;
    fn show_command(&self, benchmark: &P::Benchmark, timeout: &Duration) -> String;
}
