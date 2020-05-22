use crate::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::collections::*;

struct TestPostpro {
    benchmarks: Vec<Benchmark>,
    solvers: Vec<Solver>,
}

impl TestPostpro {
    fn new(
    benchmarks: Vec<Benchmark>,
    solvers: Vec<Solver>,
        ) -> Self {
        TestPostpro {
            benchmarks,solvers,
        }
    }
}

impl Postprocessor for TestPostpro {
    type Mapped = BenchmarkResult;
    type Reduced = Vec<BenchmarkResult>;
    // fn id(&self) -> &str { "test_postpro" }

    fn map(&self, r: &BenchmarkResult) -> Result<BenchmarkResult> {
        Ok(r.clone())
    }

    fn reduce(&self, iter: impl IntoIterator<Item=Self::Mapped>) -> Result<Self::Reduced> {
        Ok(iter.into_iter().collect())
    }

    fn write_reduced(&self, results: Self::Reduced, conf: BenchmarkConfig, _io: PostproIOAccess) -> Result<()>{

        let proc = results;
        assert_eq!(self.benchmarks.len() * self.solvers.len(), proc.len());
        println!("{}", proc.len());
        for b in self.benchmarks.iter() {
            for s in self.solvers.iter() {
                assert!(proc.iter().any(|r| r.benchmark() == b && r.solver() == s), "\nb: {}\ns: {}\nproc: {:#?}", s, b, proc.iter().map(|r|format!("{:?}", (r.benchmark(), r.solver()))).collect::<Vec<_>>());
            }
        }
        assert_eq!(self.benchmarks.len(), conf.benchmarks().len());
        assert_eq!(self.solvers.len(), conf.solvers().len());
        Ok(())
    }
}

#[test]
fn test_all_ran() {
    println!("one");
    assert!(prop(
            vec![0,1,2].into_iter().collect(), 
            vec![].into_iter().collect()));

    println!("two");
    assert!(prop(
            vec![].into_iter().collect(), 
            vec![173, 73, 73].into_iter().collect()));

    println!("three");
    assert!(prop(
            vec![0,1,2].into_iter().collect(), 
            vec![173, 73, 73].into_iter().collect()));
    println!("end");

}

// use quickcheck::quickcheck;
// quickcheck! {
    fn prop(benchmarks: BTreeSet<usize>, solvers: BTreeSet<usize>) -> bool {

        let benchmarks = benchmarks.into_iter()
            .map(|x| format!("{}", x))
            .collect::<Vec<_>>();

        let solvers = solvers.into_iter()
            .map(|x| format!("{}", x))
            .collect::<Vec<_>>();

        let bench_dir = tempfile::tempdir().unwrap();
        let solver_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();

        let opts = Opts {
            bench_dir: bench_dir.path().to_owned(),
            solver_dir: solver_dir.path().to_owned(),
            outdir: out_dir.path().to_owned(),
            only_post_process: false,
            timeout: 1,
            num_threads: None,
        };

        let benchmarks: Vec<_> = benchmarks.into_iter()
            .map(|b|bench_dir.path().join(b))
            .collect();

        let solvers: Vec<_> = solvers.into_iter()
            .map(|s|solver_dir.path().join(s))
            .collect();

        for b in benchmarks.iter() {
            fs::write(&b, format!("{}", b.display())).unwrap();
        }

        for s in solvers.iter() {
            fs::write(&s, format!("#!/bin/bash\necho {}",s.display())).unwrap();
            fs::set_permissions(&s, Permissions::from_mode(0o777)).unwrap();
        }

        main_with_opts(TestPostpro ::new(
            benchmarks.into_iter().map(|p|p.canonicalize().unwrap()).map(Benchmark::new).collect(),
            solvers.into_iter().map(|p|p.canonicalize().unwrap()).map(Solver::new).collect(),
        ), opts).unwrap();

        true
    }
// }
