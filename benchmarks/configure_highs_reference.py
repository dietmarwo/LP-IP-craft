"""Benchmark-only controls; never adds HiGHS to the production workspace."""
from pathlib import Path
import argparse,hashlib,json
p=argparse.ArgumentParser();p.add_argument('repo',type=Path);a=p.parse_args()
f=a.repo/'crates/highs/src/lib.rs';s=f.read_text();before=hashlib.sha256(s.encode()).hexdigest()
if '// LPIP_REFERENCE_CONTROLS' not in s:
 needle='        set(&mut native, &mut report, "threads", 1_i32)?;'
 insert='''
        // LPIP_REFERENCE_CONTROLS: isolated benchmark options, not production.
        if let Ok(presolve)=std::env::var("LPIP_PRESOLVE") {
            set(&mut native,&mut report,"presolve",presolve.as_str())?;
        }
        if !model.is_mip() {
            if let Ok(solver)=std::env::var("LPIP_HIGHS_SOLVER") {
                set(&mut native,&mut report,"solver",solver.as_str())?;
                if solver=="simplex" {
                    set(&mut native,&mut report,"simplex_strategy",1_i32)?;
                    set(&mut native,&mut report,"parallel","off")?;
                }
            }
        }
'''
 assert needle in s;s=s.replace(needle,needle+insert)
 needle='        result.iterations = u64::try_from(solved.simplex_iteration_count()).unwrap_or(0)'
 assert needle in s
 s=s.replace(needle,'''        result.backend.effective_options.extend([
            ("diagnostics.simplex_iterations".into(),solved.simplex_iteration_count().to_string()),
            ("diagnostics.ipm_iterations".into(),solved.ipm_iteration_count().to_string()),
        ]);
'''+needle)
 f.write_text(s)
print(json.dumps({'source':str(f),'before_sha256':before,'after_sha256':hashlib.sha256(f.read_bytes()).hexdigest()}))
