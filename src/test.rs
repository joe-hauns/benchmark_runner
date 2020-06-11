use crate::*;
use std::fs;
use std::fs::*;
use std::ffi::*;
use std::os::unix::fs::PermissionsExt;
use std::collections::*;
use crate::interface::solvers::Script;

struct TestPostpro;

impl TestPostpro {
    fn new() -> Self {
        TestPostpro { }
    }
}

impl<P> Summerizable for TestReduced<P> 
    where P: Postprocessor
{
    fn write_summary<W: io::Write>(&self, _: W) -> Result<()> {Ok(())}
}

#[derive(Deserialize, Serialize, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct TestReduced<P>(
    #[serde(bound(serialize = "P: Postprocessor", deserialize = "P: Postprocessor"))]
    JobConfig<P>, 
    #[serde(bound(serialize = "P: Postprocessor", deserialize = "P: Postprocessor"))]
    Vec<(BenchRunConf<P>, BenchRunResult<P>)>)
    where P: Postprocessor
     ;


impl Postprocessor for TestPostpro {
    type Solver = Script;
    type Mapped = BenchRunResult<Self>;
    type Reduced = TestReduced<Self>;
    type BAnnot = ();
    fn annotate_benchark(&self, _: &Benchmark) -> Result<Self::BAnnot> { Ok(()) }

    fn map(&self, r: &BenchRunResult<Self>) -> Result<Self::Mapped> {
        Ok(r.clone())
    }

    fn reduce(&self, conf: &JobConfig<Self>, iter: impl IntoIterator<Item=(BenchRunConf<Self>, Self::Mapped)>) -> Result<Self::Reduced> {
        Ok(TestReduced(conf.clone(), iter.into_iter().collect()))
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
            .map(|x| format!("benchmark{}", x))
            .collect::<Vec<_>>();

        let solvers = solvers.into_iter()
            .map(|x| format!("solver{}", x))
            .collect::<Vec<_>>();

        let bench_dir = tempfile::tempdir().unwrap();
        let solver_dir = tempfile::tempdir().unwrap();
        let out_dir = tempfile::tempdir().unwrap();
        let timeout = 1;

        let opts = Opts {
            bench_dir: bench_dir.path().to_owned(),
            solver_dir: solver_dir.path().to_owned(),
            outdir: out_dir.path().to_owned(),
            // only_post_process: false,
            timeout,
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

        let script = |s: &PathBuf| -> String { 
            format!(r#" 
              #!/bin/bash 
              echo err {solver} $* >> /dev/stderr
              echo {solver} $*
              "#, solver=s.parent().unwrap().canonicalize().unwrap().join(s.file_name().unwrap()).display())
        };

        let script_err = |solver: &PathBuf, benchmark: &PathBuf, timeout: u64| -> String { 
            format!("err {} {} {}\n", solver.display(), benchmark.display(), timeout)
        };

        let script_out = |solver: &PathBuf, benchmark: &PathBuf, timeout: u64| -> String { 
            format!("{} {} {}\n", solver.display(), benchmark.display(), timeout)
        };

        for s in solvers.iter() {
            fs::write(&s, script(&s)).unwrap();
            fs::set_permissions(&s, Permissions::from_mode(0o777)).unwrap();
        }

        let benchmarks: Vec<_> = benchmarks.into_iter().map(|p|p.canonicalize().unwrap()).collect();
        let solvers: Vec<_>  = solvers.into_iter().map(|p|p.canonicalize().unwrap()).collect();

        let proc = run_with_opts(TestPostpro ::new(), opts).unwrap();

        assert_eq!(benchmarks.len() * solvers.len(), proc.1.len());
        itertools::assert_equal(
                benchmarks.iter().sorted(),
                proc.0.benchmarks().iter().map(|x|&x.as_ref().file).sorted()
            );
        itertools::assert_equal(
                solvers.iter().sorted(),
                proc.0.solvers().iter().map(|s| {
                    // let s: &<TestPostpro as Postprocessor>::Solver  = s.as_ref();
                    let s: &<test::TestPostpro as interface::Postprocessor>::Solver = s.as_ref();
                    s.id().as_ref()
                }).sorted()
            );

        for (run, result) in proc.1.iter() {
            assert_eq!(&proc.0, run.job.as_ref());
            assert_eq!(run, &result.run);
        }


        for b in benchmarks.iter() {
            for s in solvers.iter() {
                let filtered = proc.1.iter()
                    .filter(|(run,_res)| &run.benchmark.file == b && run.solver.id().as_ref() == s)
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
                    stdout,
                    stderr,
                    status,
                    exit_status,
                } = res;
                assert_eq!(String::from_utf8(stdout.clone()).unwrap(), script_out(&s, &b, timeout));
                assert_eq!(String::from_utf8(stderr.clone()).unwrap(), script_err(&s, &b, timeout));
                assert_eq!(exit_status, Some(0));
                assert_eq!(status, BenchmarkStatus::Success);
            }
        }
        true
    }
