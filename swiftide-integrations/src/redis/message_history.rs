use anyhow::{Context as _, Result};
use async_trait::async_trait;
use swiftide_core::{MessageHistory, chat_completion::ChatMessage, indexing::Chunk};

use super::Redis;

#[async_trait]
impl<T: Chunk> MessageHistory for Redis<T> {
    async fn history(&self) -> Result<Vec<ChatMessage>> {
        if let Some(mut cm) = self.lazy_connect().await {
            let messages: Vec<String> = redis::cmd("LRANGE")
                .arg(&self.message_history_key)
                .arg(0)
                .arg(-1)
                .query_async(&mut cm)
                .await
                .context("Error fetching message history")?;
            let chat_messages: Result<Vec<ChatMessage>> = messages
                .into_iter()
                .map(|msg| serde_json::from_str(&msg).context("Error deserializing message"))
                .collect();
            chat_messages
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }

    async fn push_owned(&self, item: ChatMessage) -> Result<()> {
        if let Some(mut cm) = self.lazy_connect().await {
            redis::cmd("RPUSH")
                .arg(&self.message_history_key)
                .arg(serde_json::to_string(&item)?)
                .query_async::<()>(&mut cm)
                .await
                .context("Error pushing to message history")?;
            Ok(())
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }

    async fn extend_owned(&self, items: Vec<ChatMessage>) -> Result<()> {
        if let Some(mut cm) = self.lazy_connect().await {
            // If it does not exist yet, we can just push the items
            let _ = redis::cmd("DEL")
                .arg(&self.message_history_key)
                .query_async::<()>(&mut cm)
                .await;

            redis::cmd("RPUSH")
                .arg(&self.message_history_key)
                .arg(
                    items
                        .iter()
                        .map(serde_json::to_string)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .query_async::<()>(&mut cm)
                .await
                .context("Error pushing to message history")?;
            Ok(())
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }

    async fn overwrite(&self, items: Vec<ChatMessage>) -> Result<()> {
        if let Some(mut cm) = self.lazy_connect().await {
            // If it does not exist yet, we can just push the items
            let _ = redis::cmd("DEL")
                .arg(&self.message_history_key)
                .query_async::<()>(&mut cm)
                .await;

            if items.is_empty() {
                // If we are overwriting with an empty history, we can just return
                return Ok(());
            }

            redis::cmd("RPUSH")
                .arg(&self.message_history_key)
                .arg(
                    items
                        .iter()
                        .map(serde_json::to_string)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .query_async::<()>(&mut cm)
                .await
                .context("Error pushing to message history")?;
            Ok(())
        } else {
            anyhow::bail!("Failed to connect to Redis")
        }
    }
}

#[cfg(test)]
mod tests {
    use testcontainers::{ContainerAsync, GenericImage, runners::AsyncRunner as _};

    use super::*;

    async fn start_redis() -> (String, ContainerAsync<GenericImage>) {
        let redis_container = testcontainers::GenericImage::new("redis", "7.2.4")
            .with_exposed_port(6379.into())
            .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
                "Ready to accept connections",
            ))
            .start()
            .await
            .expect("Redis started");

        let host = redis_container.get_host().await.unwrap();
        let port = redis_container.get_host_port_ipv4(6379).await.unwrap();

        let url = format!("redis://{host}:{port}/");

        (url, redis_container)
    }

    #[tokio::test]
    async fn test_no_messages_yet() {
        let (url, _container) = start_redis().await;
        let redis = Redis::try_from_url(url, "tests").unwrap();

        let messages = redis.history().await.unwrap();
        assert!(
            messages.is_empty(),
            "Expected history to be empty for new Redis key"
        );
    }

    #[tokio::test]
    async fn test_adding_and_next_completions() {
        let (url, _container) = start_redis().await;
        let redis = Redis::try_from_url(url, "tests").unwrap();

        let m1 = ChatMessage::System("System test".to_string());
        let m2 = ChatMessage::User("User test".to_string());

        redis.push_owned(m1.clone()).await.unwrap();
        redis.push_owned(m2.clone()).await.unwrap();

        let hist = redis.history().await.unwrap();
        assert_eq!(
            hist,
            vec![m1.clone(), m2.clone()],
            "History should match what's pushed"
        );

        let hist2 = redis.history().await.unwrap();
        assert_eq!(
            hist2,
            vec![m1, m2],
            "History should be unchanged on repeated call"
        );
    }

    #[tokio::test]
    async fn test_overwrite_history() {
        let (url, _container) = start_redis().await;
        let redis = Redis::try_from_url(url, "tests").unwrap();

        // Check that overwrite on empty also works
        redis.overwrite(vec![]).await.unwrap();

        let m1 = ChatMessage::System("First".to_string());
        let m2 = ChatMessage::User("Second".to_string());
        redis.push_owned(m1.clone()).await.unwrap();
        redis.push_owned(m2.clone()).await.unwrap();

        let m3 = ChatMessage::new_assistant(Some("Overwritten".to_string()), None);
        redis.overwrite(vec![m3.clone()]).await.unwrap();

        let hist = redis.history().await.unwrap();
        assert_eq!(
            hist,
            vec![m3],
            "History should only contain the overwritten message"
        );
    }

    #[tokio::test]
    async fn test_extend() {
        let (url, _container) = start_redis().await;
        let redis = Redis::try_from_url(url, "tests").unwrap();

        let m1 = ChatMessage::System("First".to_string());
        let m2 = ChatMessage::User("Second".to_string());
        redis
            .extend_owned(vec![m1.clone(), m2.clone()])
            .await
            .unwrap();

        let hist = redis.history().await.unwrap();
        assert_eq!(
            hist,
            vec![m1, m2],
            "History should only contain the overwritten message"
        );
    }
}
