//! Internal state of the agent

#[derive(Clone, Copy, Debug, Default, strum_macros::EnumDiscriminants, strum_macros::EnumIs)]
pub(crate) enum State {
    #[default]
    Pending,
    Running,
    Stopped,
}
