//! A task node that holds the draft until a human approves it.

use std::{
    convert::Infallible,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use swiftide::{
    reexports::async_trait::async_trait,
    tasks::{DynNodeId, TaskNode},
};

use crate::{progress::Progress, render::ArchitectureDraft};

/// What the approval gate decided for a draft.
#[derive(Debug, Clone)]
pub enum Review {
    Approved(ArchitectureDraft),
    AwaitingApproval,
}

// ANCHOR: approval
/// Lets a draft through once it is approved, and otherwise keeps it for review.
///
/// The graph pauses the task when the gate answers [`Review::AwaitingApproval`]. A paused branch
/// re-runs its node on `resume()`, so after [`ApprovalGate::approve`] the same draft passes
/// through. The gate is cheap to evaluate, which is why the pause sits here and not on the
/// expensive render node.
#[derive(Debug, Clone)]
pub struct ApprovalGate {
    state: Arc<Mutex<GateState>>,
    progress: Progress,
}

#[derive(Debug, Default)]
struct GateState {
    approved: bool,
    pending: Option<ArchitectureDraft>,
}

impl ApprovalGate {
    pub fn new(pre_approved: bool, progress: Progress) -> Self {
        let state = GateState {
            approved: pre_approved,
            pending: None,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            progress,
        }
    }

    /// The draft waiting for a decision, if any.
    pub fn pending_draft(&self) -> Option<ArchitectureDraft> {
        self.state().pending.clone()
    }

    pub fn approve(&self) {
        self.state().approved = true;
    }

    fn state(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl TaskNode for ApprovalGate {
    type Input = ArchitectureDraft;
    type Output = Review;
    type Error = Infallible;

    async fn evaluate(
        &self,
        _node_id: &DynNodeId<Self>,
        draft: &ArchitectureDraft,
    ) -> Result<Review, Infallible> {
        let mut state = self.state();
        if state.approved {
            state.pending = None;
            self.progress.step("review", "draft approved");
            return Ok(Review::Approved(draft.clone()));
        }

        state.pending = Some(draft.clone());
        self.progress.step(
            "review",
            format_args!(
                "draft ready ({} bytes), waiting for approval",
                draft.as_str().len()
            ),
        );
        Ok(Review::AwaitingApproval)
    }
}
// ANCHOR_END: approval
