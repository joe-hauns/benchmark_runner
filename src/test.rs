use crate::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::collections::*;

struct TestPostpro;

impl TestPostpro {
    fn new(
    // benchmarks: Vec<Benchmark>,
    // solvers: Vec<Solver>,
        ) -> Self {
        TestPostpro {
            // benchmarks,solvers,
        }
    }
}

impl<A> Summerizable for TestReduced<A> {
    fn write_summary<W: io::Write>(&self, _: W) -> Result<()> {Ok(())}
}

#[derive(Deserialize, Serialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct TestReduced<A>(JobConfig<()>, Vec<(BenchRunConf<A>, BenchRunResult<A>)>);


impl Postprocessor for TestPostpro {
    type Mapped = BenchRunResult<Self::BenchmarkAnnotations>;
    type Reduced = TestReduced<Self::BenchmarkAnnotations>;
    type BenchmarkAnnotations = ();
    fn annotate_benchark(&self, _: &Benchmark) -> Result<Self::BenchmarkAnnotations> { Ok(()) }

    fn map(&self, r: &BenchRunResult<Self::BenchmarkAnnotations>) -> Result<BenchRunResult> {
        Ok(r.clone())
    }

    fn reduce(&self, conf: &JobConfig<Self::BenchmarkAnnotations>, iter: impl IntoIterator<Item=(BenchRunConf<Self::BenchmarkAnnotations>, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(TestReduced(conf.clone(), iter.into_iter().collect()))
    }

    type BenchmarkAnnotations = ();
    fn annotate_benchark(&self, b: &Benchmark) -> Result<Self::BenchmarkAnnotations> { Ok(()) }
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
            fs::write(&s, format!(r#" #!/bin/bash 
                                      echo {} $*    "#,s.display())).unwrap();
            fs::set_permissions(&s, Permissions::from_mode(0o777)).unwrap();
        }

        let benchmarks: Vec<_> = benchmarks.into_iter().map(|p|p.canonicalize().unwrap()).collect();
        let solvers: Vec<_>  = solvers.into_iter().map(|p|p.canonicalize().unwrap()).collect();

        let proc = main_with_opts(TestPostpro ::new(
            // benchmarks.iter().map(|p|Benchmark::new(p.clone())).collect(),
            // solvers.iter().map(|p|Solver::new(p.clone())).collect(),
        ), opts).unwrap();

        // let proc_benchmarks = proc.iter().map(|x|x.benchmark());
        // let proc_solvers    = proc.iter().map(|x|x.solver());

        assert_eq!(benchmarks.len() * solvers.len(), proc.1.len());
        itertools::assert_equal(
                benchmarks.iter().sorted(),
                proc.0.benchmarks().iter().map(|x|&x.as_ref().file).sorted()
            );
        itertools::assert_equal(
                solvers.iter().sorted(),
                proc.0.solvers().iter().map(|x|&x.as_ref().file).sorted()
            );

        for (run, result) in proc.1.iter() {
            assert_eq!(&proc.0, run.job.as_ref());
            assert_eq!(run, &result.run);
        }


        for b in benchmarks.iter() {
            for s in solvers.iter() {
                let filtered = proc.1.iter()
                    .filter(|(run,_res)| &run.benchmark.file == b && &run.solver.file == s)
                    .collect::<Vec<_>>();
                if filtered.len() != 1 {
                    println!("benchmark: {}", b.display());
                    println!("solver:    {}", s.display());
                    println!("found:     {:#?}", filtered);
                    panic!();
                }
                let (_, res) = filtered[0].clone();
                //TODO better testing
                let BenchRunResult {
                    run: _,
                    time: _,
                    stdout: _,
                    stderr: _,
                    status,
                    exit_status,
                } = res;
                assert!(exit_status == Some(0));
                assert!(status == BenchmarkStatus::Success);
            }
        }
        true
    }
