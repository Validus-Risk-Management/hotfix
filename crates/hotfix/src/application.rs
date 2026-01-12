#[async_trait::async_trait]
/// The application users of HotFIX can implement to hook into the engine.
pub trait Application<Inbound, Outbound>: Send + Sync + 'static {
    /// Called when a message is sent to the engine to be sent to the counterparty.
    ///
    /// This is invoked before the raw message is persisted in the message store.
    async fn on_outbound_message(&self, msg: &Outbound) -> OutboundDecision;
    /// Called when a message is received from the counterparty.
    ///
    /// This is invoked after the message is verified and parsed into a typed message.
    async fn on_inbound_message(&self, msg: Inbound) -> InboundDecision;
    /// Called when the session is logged out.
    async fn on_logout(&mut self, reason: &str);
    /// Called when the session is logged on.
    async fn on_logon(&mut self);
}

pub enum InboundDecision {
    Accept,
    TerminateSession,
}

pub enum OutboundDecision {
    Send,
    Drop,
    TerminateSession,
}
