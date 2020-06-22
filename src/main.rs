use benchmark_runner::*;
use std::io;
use serde::*;
use clap::*;
use anyhow::Result;


struct NopBenchmarker;
#[derive(Serialize,Deserialize)]
struct Unit;
impl Summerizable for Unit {
    fn write_summary<W: io::Write>(&self, _: W) -> Result<()> {Ok(())}
}

impl Benchmarker for NopBenchmarker {
    type Mapped = Unit;
    type Reduced = Unit;
    type Solver = benchmark_runner::solvers::Script;
    type Benchmark =  benchmark_runner::ids::PathId;
    fn map(&self, _: &BenchRunResult<Self>) -> Result<Self::Mapped> {
        Ok(Unit)
    }
    fn reduce(&self, _: &JobConfig<Self>,_: impl IntoIterator<Item=(BenchRunResult<Self>, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(Unit)
    }
}

fn main() -> Result<()> {
    benchmark_runner::main_with_opts(NopBenchmarker, Opts::parse())
}
