use crate::message::OutboundMessage;
use crate::session::Status;
use hotfix_message::message::Message;

#[async_trait::async_trait]
/// The application users of HotFIX can implement to hook into the engine.
pub trait Application: Send + Sync + 'static {
    type Outbound: OutboundMessage;

    /// Called when a message is sent to the engine to be sent to the counterparty.
    ///
    /// This is invoked before the raw message is persisted in the message store.
    async fn on_outbound_message(&self, msg: &Self::Outbound) -> OutboundDecision;
    /// Called when a message is received from the counterparty.
    ///
    /// This is invoked after the message is verified by the session layer.
    async fn on_inbound_message(&self, msg: &Message) -> InboundDecision;
    /// Called when the session is logged out.
    async fn on_logout(&mut self, reason: &str);
    /// Called when the session is logged on.
    async fn on_logon(&mut self);
    /// Called when the session state changes.
    ///
    /// This is invoked after every state transition, providing the previous
    /// and new status. The default implementation does nothing.
    async fn on_state_change(&self, from: &Status, to: &Status);
}

/// Standard FIX Business Reject Reason values (tag 380).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BusinessRejectReason {
    Other = 0,
    UnknownId = 1,
    UnknownSecurity = 2,
    UnsupportedMessageType = 3,
    ApplicationNotAvailable = 4,
    ConditionallyRequiredFieldMissing = 5,
    NotAuthorized = 6,
    DeliverToFirmNotAvailable = 7,
}

pub enum InboundDecision {
    Accept,
    Reject {
        reason: BusinessRejectReason,
        text: Option<String>,
    },
    TerminateSession,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboundDecision {
    Send,
    Drop,
    TerminateSession,
}
