mod log;
mod log_reader;
mod record;
mod supervisor;

///
/// This library implements running subprocesses with the output redirected to log files, plus a
/// log reader that can be used to read process output in a structured way.
///
pub use log::Log;
pub use supervisor::Supervisor;
