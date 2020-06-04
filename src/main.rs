use anyhow::*;
use benchmark_runner::*;
use std::io;
use serde::*;


struct NopPostprocessor;
#[derive(Serialize,Deserialize)]
struct Unit;
impl Summerizable for Unit {
    fn write_summary<W: io::Write>(&self, _: W) -> Result<()> {Ok(())}
}

impl Postprocessor for NopPostprocessor {
    type Mapped = Unit;
    type Reduced = Unit;
    type BenchmarkAnnotations = ();
    fn annotate_benchark(&self, _: &Benchmark) -> Result<Self::BenchmarkAnnotations> { Ok(()) }
    fn map(&self, _: &BenchRunResult<Self::BenchmarkAnnotations>) -> Result<Self::Mapped> {
        Ok(Unit)
    }
    fn reduce(&self, _: &JobConfig<Self::BenchmarkAnnotations>,_: impl IntoIterator<Item=(BenchRunConf<Self::BenchmarkAnnotations>, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(Unit)
    }
}

fn main() -> Result<()> {
    benchmark_runner::main(NopPostprocessor)
}
