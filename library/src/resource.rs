pub trait ResourceController: Send + Sync {
    fn setup(&self) -> std::io::Result<()>;
}

/// A resource controller that does nothing.
pub struct NoOpController {}

impl ResourceController for NoOpController {
    fn setup(&self) -> std::io::Result<()> {
        Ok(())
    }
}
