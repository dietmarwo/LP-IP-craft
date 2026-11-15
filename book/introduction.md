# LP-IP-craft documentation

LP-IP-craft is a native Rust solver for linear and mixed-integer linear
programs, with Python bindings and a job worker. This site renders the pages
a user needs:

- [Project overview](README.md): what the solver does, how to install it and
  what has been measured.
- [Native API](docs/api.md): models, options, statuses and independent checks.
- [Application tutorials](tutorials/README.md): nine worked models, each
  checked against an independent reference.
- [AI context](ai-context.md): modeling rules, options and checks for an agent
  that formulates a problem.
- [Shared job API](docs/job-api.md): LP and MIP jobs through a shell or MCP.

Plans, implementation logs, benchmark records and raw evidence are not
rendered here. Links to them lead to the
[repository on GitHub](https://github.com/dietmarwo/LP-IP-craft), where the
same Markdown files are the canonical source of every page on this site.

LP-IP-craft is one of
[18 related repositories](https://github.com/dietmarwo/fcmaes-rust/blob/main/docs/repositories.md).
To choose between it and the other solvers, start at the
[shared solver-selection guide](https://github.com/dietmarwo/optimization-jobs/blob/main/ai-context.md).
