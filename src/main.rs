use anyhow::*;
use benchmark_runner::*;


struct NopPostprocessor;

impl Postprocessor for NopPostprocessor {
    type Mapped = ();
    type Reduced = ();
    fn map(&self, _: &BenchmarkResult) -> Result<Self::Mapped> {
        Ok(())
    }
    fn reduce(&self, _: impl IntoIterator<Item=Self::Mapped>) -> Result<Self::Reduced> {
        Ok(())
    }
    fn write_reduced(&self, _: Self::Reduced, _: BenchmarkConfig, _: PostproIOAccess) -> Result<()> {
        Ok(())
    }

}

fn main() -> Result<()> {
    benchmark_runner::main(NopPostprocessor)
}
