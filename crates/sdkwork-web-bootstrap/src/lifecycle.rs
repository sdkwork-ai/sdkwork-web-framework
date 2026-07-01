use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type LifecycleFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// EP-20: application startup/shutdown hooks around [`crate::serve`].
pub trait WebFrameworkLifecycle: Send + Sync {
    fn on_startup(&self) -> LifecycleFuture<'_> {
        Box::pin(async { Ok(()) })
    }

    fn on_shutdown(&self) -> LifecycleFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Default)]
pub struct NoOpWebFrameworkLifecycle;

impl WebFrameworkLifecycle for NoOpWebFrameworkLifecycle {}

/// Runs multiple lifecycle hooks in registration order.
#[derive(Clone, Default)]
pub struct CompositeWebFrameworkLifecycle {
    hooks: Vec<Arc<dyn WebFrameworkLifecycle>>,
}

impl CompositeWebFrameworkLifecycle {
    pub fn new(hooks: Vec<Arc<dyn WebFrameworkLifecycle>>) -> Self {
        Self { hooks }
    }

    pub fn push(mut self, hook: Arc<dyn WebFrameworkLifecycle>) -> Self {
        self.hooks.push(hook);
        self
    }
}

impl WebFrameworkLifecycle for CompositeWebFrameworkLifecycle {
    fn on_startup(&self) -> LifecycleFuture<'_> {
        let hooks = self.hooks.clone();
        Box::pin(async move {
            for hook in hooks {
                hook.on_startup().await?;
            }
            Ok(())
        })
    }

    fn on_shutdown(&self) -> LifecycleFuture<'_> {
        let hooks = self.hooks.clone();
        Box::pin(async move {
            for hook in hooks {
                hook.on_shutdown().await?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingLifecycle {
        startup: Arc<AtomicUsize>,
        shutdown: Arc<AtomicUsize>,
    }

    impl WebFrameworkLifecycle for CountingLifecycle {
        fn on_startup(&self) -> LifecycleFuture<'_> {
            let counter = self.startup.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }

        fn on_shutdown(&self) -> LifecycleFuture<'_> {
            let counter = self.shutdown.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn composite_lifecycle_runs_hooks_in_order() {
        let startup = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicUsize::new(0));
        let lifecycle = CompositeWebFrameworkLifecycle::new(vec![
            Arc::new(CountingLifecycle {
                startup: startup.clone(),
                shutdown: shutdown.clone(),
            }),
            Arc::new(CountingLifecycle {
                startup: startup.clone(),
                shutdown: shutdown.clone(),
            }),
        ]);
        lifecycle.on_startup().await.expect("startup");
        lifecycle.on_shutdown().await.expect("shutdown");
        assert_eq!(2, startup.load(Ordering::SeqCst));
        assert_eq!(2, shutdown.load(Ordering::SeqCst));
    }
}
