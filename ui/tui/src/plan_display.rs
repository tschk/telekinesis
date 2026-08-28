//! Whole-turn plan gate that auto-approves but forwards the proposal to the
//! UI, so the agent keeps its own internal goals and plans without host
//! interruptions. The y/n gate lives in rx4's `ChannelPlanApprover`; hosts
//! that want a hard gate use that instead.

use rx4::permissions::{PlanApprover, PlanDecision, PlanProposal};

use crate::app::PendingPlanApproval;

pub type AutoPlanReceiver = tokio::sync::mpsc::UnboundedReceiver<PendingPlanApproval>;

pub struct AutoPlanDisplay {
    tx: tokio::sync::mpsc::UnboundedSender<PendingPlanApproval>,
}

impl AutoPlanDisplay {
    pub fn pair() -> (Self, AutoPlanReceiver) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

#[async_trait::async_trait]
impl PlanApprover for AutoPlanDisplay {
    async fn approve_plan(&self, proposal: &PlanProposal) -> PlanDecision {
        let (respond, _rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send((proposal.clone(), respond));
        PlanDecision::Approve
    }
}
