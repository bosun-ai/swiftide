//! Internal utility functions and macros for anything agent

/// Simple macro to consistently call hooks and clean up the code
#[macro_export]
macro_rules! invoke_hooks {
    ($hook_type:ident, $self_expr:expr $(, $arg:expr)* ) => {{
        // Iterate through every hook matching `HookTypes::$hook_type`
        for hook in $self_expr.hooks_by_type(HookTypes::$hook_type) {
            // Downcast to the correct closure variant
            if let Hook::$hook_type(hook_fn) = hook {
                // Create a tracing span for instrumentation
                let span = tracing::info_span!(
                    "hook",
                    "otel.name" = format!("hook.{:?}", HookTypes::$hook_type)
                );
                tracing::info!("Calling {} hook", HookTypes::$hook_type);

                // Call the hook, instrument, and log on failure
                if let Err(err) = hook_fn($self_expr $(, $arg)*)
                    .instrument(span.or_current())
                    .await
                {
                    tracing::error!(
                        "Error in {hooktype} hook: {err}",
                        hooktype = HookTypes::$hook_type,
                    );
                }
            }
        }
    }};
}
