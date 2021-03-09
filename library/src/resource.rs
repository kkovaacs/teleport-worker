use cgroups_rs::cpu::CpuController;
use cgroups_rs::memory::MemController;
use cgroups_rs::{Cgroup, CgroupPid};

use std::time::Duration;

pub trait ResourceController: Send + Sync {
    /// Set up resource controls for command
    fn setup(&self) -> std::io::Result<()>;

    /// Cleans up resource controls for a child process
    fn cleanup(&self, pid: u32) -> std::io::Result<()>;
}

/// A resource controller that does nothing.
pub struct NoOpController {}

impl ResourceController for NoOpController {
    fn setup(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn cleanup(&self, _pid: u32) -> std::io::Result<()> {
        Ok(())
    }
}

/// A cgroups-based resource controller
///
/// Creates a new cgroup for each subprocess. The name of the cgroup is based on the PID of
/// the child process -- the setup() method is called after fork() but before exec(), so the
/// PID setup() runs with is the PID of the command we'll be running.
///
/// Resource limits are arbitrary here -- just for the sake of demonstrating that they can be
/// set.
pub struct CGroupsController {}

impl ResourceController for CGroupsController {
    fn setup(&self) -> std::io::Result<()> {
        let pid = std::process::id();

        // create cgroup
        let hierarchy = cgroups_rs::hierarchies::auto();
        let cg = Cgroup::new(hierarchy, pid_to_name(pid));

        if let Some(memory_controller) = cg.controller_of::<MemController>() {
            // arbitrary limit: this should be configurable in a non-prototype implementation
            const MEMORY_LIMIT: i64 = 1024 * 1024 * 10;
            memory_controller
                .set_limit(MEMORY_LIMIT)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        }

        if let Some(cpu_controller) = cg.controller_of::<CpuController>() {
            // arbitrary limit: CPU shares should be configurable for each job in a non-prototype
            // implementation
            cpu_controller
                .set_shares(100)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            cpu_controller
                .set_cfs_period(Duration::from_millis(100).as_micros() as u64)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            cpu_controller
                .set_cfs_quota(Duration::from_millis(10).as_micros() as i64)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        }

        cg.add_task(CgroupPid::from(pid as u64))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    fn cleanup(&self, pid: u32) -> std::io::Result<()> {
        // delete cgroup
        let hierarchy = cgroups_rs::hierarchies::auto();
        let cg = Cgroup::load(hierarchy, pid_to_name(pid));
        cg.delete()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

fn pid_to_name(pid: u32) -> String {
    format!("worker-{}", pid)
}
