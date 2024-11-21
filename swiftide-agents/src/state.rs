

#[derive(Clone, Copy, Debug, Default, strum_macros::EnumDiscriminants, strum_macros::EnumIs)]
pub enum State {
    #[default]
    Pending,
    Running,
    Stopped,
}
