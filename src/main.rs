use benchmark_runner::*;
use std::io;
use serde::*;
use clap::*;
use anyhow::Result;


struct NopPostprocessor;
#[derive(Serialize,Deserialize)]
struct Unit;
impl Summerizable for Unit {
    fn write_summary<W: io::Write>(&self, _: W) -> Result<()> {Ok(())}
}

impl Postprocessor for NopPostprocessor {
    type Mapped = Unit;
    type Reduced = Unit;
    type BAnnot = ();
    fn annotate_benchark(&self, _: &Benchmark) -> Result<Self::BAnnot> { Ok(()) }
    fn map(&self, _: &BenchRunResult<Self::BAnnot>) -> Result<Self::Mapped> {
        Ok(Unit)
    }
    fn reduce(&self, _: &JobConfig<Self::BAnnot>,_: impl IntoIterator<Item=(BenchRunConf<Self::BAnnot>, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(Unit)
    }
}

fn main() -> Result<()> {
    benchmark_runner::main_with_opts(NopPostprocessor, Opts::parse())
}
