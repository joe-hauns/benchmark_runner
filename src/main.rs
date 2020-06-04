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
    fn map(&self, _: &BenchRunResult) -> Result<Self::Mapped> {
        Ok(Unit)
    }
    fn reduce(&self, _: &JobConfig,_: impl IntoIterator<Item=(BenchRunConf, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(Unit)
    }
}

fn main() -> Result<()> {
    benchmark_runner::main(NopPostprocessor)
}
