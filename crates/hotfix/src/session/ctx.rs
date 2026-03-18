use hotfix_message::MessageBuilder;
use hotfix_message::message::Config as MessageConfig;

use crate::config::SessionConfig;

pub(crate) struct SessionCtx<S> {
    pub config: SessionConfig,
    pub store: S,
    pub message_builder: MessageBuilder,
    pub message_config: MessageConfig,
}
