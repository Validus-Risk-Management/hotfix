use crate::common::test_messages::TestMessage;
use hotfix::Application;

pub struct MockApplication {}

#[async_trait::async_trait]
impl Application<TestMessage> for MockApplication {
    async fn on_outbound_message(&self, _msg: &TestMessage) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_inbound_message(&self, _msg: TestMessage) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_logout(&mut self, _reason: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_logon(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
