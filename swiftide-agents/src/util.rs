//! Internal utility functions and macros for anything agent

/// Simple macro to consistently call hooks and clean up the code
#[macro_export]
macro_rules! invoke_hooks {
    (OnStream, $self_expr:expr $(, $arg:expr)* ) => {{
        // For streaming we log less and only on the trace level
        let hooks = std::sync::Arc::clone(&$self_expr.hooks);
        for hook in hooks
            .iter()
            .filter(|hook| HookTypes::OnStream == (*hook).into())
        {
            // Downcast to the correct closure variant
            if let Hook::OnStream(hook_fn) = hook {
                // Create a tracing span for instrumentation
                let span = tracing::trace_span!(
                    "hook",
                    "otel.name" = format!("hook.{:?}", HookTypes::OnStream)
                );

                // Call the hook, instrument, and log on failure
                if let Err(err) = hook_fn($self_expr $(, $arg)*)
                    .instrument(span.or_current())
                    .await
                {
                    tracing::error!(
                        "Error in {hooktype} hook: {err}",
                        hooktype = HookTypes::OnStream,
                    );
                }
            }
        }
    }};
    ($hook_type:ident, $self_expr:expr $(, $arg:expr)* ) => {{
        // Iterate through every hook matching `HookTypes::$hook_type`
        let hooks = std::sync::Arc::clone(&$self_expr.hooks);
        for hook in hooks
            .iter()
            .filter(|hook| HookTypes::$hook_type == (*hook).into())
        {
            // Downcast to the correct closure variant
            if let Hook::$hook_type(hook_fn) = hook {
                // Create a tracing span for instrumentation
                let span = tracing::info_span!(
                    "hook",
                    "otel.name" = format!("hook.{:?}", HookTypes::$hook_type)
                );
                tracing::debug!("Calling {} hook", HookTypes::$hook_type);

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
