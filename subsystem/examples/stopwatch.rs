use subsystem::{CallRequest, ShutdownRequest};
use std::time::{Instant, Duration};
use logging::log;

struct Stopwatch(Instant);

impl Stopwatch {
    async fn start(mut call_rq: CallRequest<Self>, mut shutdown_rq: ShutdownRequest) {
        let mut stopwatch = Self::new(Instant::now());
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                () = shutdown_rq.recv() => break,
                call = call_rq.recv() => call(&mut stopwatch),
                _ = interval.tick() => stopwatch.report("Running"),
            }
        }
        stopwatch.report("Elapsed");
    }

    fn new(start: Instant) -> Self {
        Self(start)
    }

    fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.0)
    }

    fn report(&self, msg: &str) {
        log::error!("{} {:?}", msg, self.elapsed());
    }
}

#[tokio::main]
async fn main() {
    logging::init_logging::<&std::path::Path>(None);

    let app = subsystem::Manager::new("toplevel");
    app.start("watch", Stopwatch::start);
    app.main().await
}

