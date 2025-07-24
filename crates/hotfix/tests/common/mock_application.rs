use crate::common::test_messages::TestMessage;
use hotfix::Application;

pub struct MockApplication {}

#[async_trait::async_trait]
impl Application<TestMessage> for MockApplication {
    async fn on_message_from_app(&self, _msg: TestMessage) {}

    async fn on_message_to_app(&self, _msg: TestMessage) {}

    async fn on_logout(&mut self, _reason: &str) {}
}
