use crate::item::Item;
use aws_config::SdkConfig;
use aws_sdk_sqs::error::SdkError;
use aws_sdk_sqs::operation::delete_message::DeleteMessageError;
use aws_sdk_sqs::operation::receive_message::ReceiveMessageError;
use aws_sdk_sqs::types::{Message, MessageSystemAttributeName};
use aws_sdk_sqs::Client;
use std::collections::HashMap;
use std::result;
use std::time::Duration;
use thiserror::Error;

/// Abstraction for a remote SQS queue.
pub struct Queue {
    queue_url: String,
    visibility_timeout: i32,
    client: Client,
}

impl Queue {
    pub fn new(queue_url: String, visibility_timeout: i32, sdk_config: &SdkConfig) -> Self {
        Queue {
            queue_url,
            visibility_timeout,
            client: Client::new(sdk_config),
        }
    }

    /// Retrieve the next item from the queue. If no items are available, wait up to
    /// `wait_duration` for an item to arrive. If there are still no items, return `None`.
    pub async fn receive(&self, wait_duration: Duration) -> Result<Option<Item>> {
        let output = self.client.receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(1)
            .wait_time_seconds(wait_duration.as_secs() as i32)
            .visibility_timeout(self.visibility_timeout)
            .message_attribute_names("All")
            .message_system_attribute_names(MessageSystemAttributeName::All)
            .send().await?;

        let mut messages = output.messages.unwrap_or_default();
        if messages.is_empty() {
            return Ok(None);
        }

        let item: Item = messages.remove(0).try_into()?;
        Ok(Some(item))
    }

    /// Acknowledge that a retrieved item has been processed. This will ensure that it is
    /// permanently removed from the queue and not re-attempted later.
    pub async fn acknowledge(&self, item: &Item) -> Result<()> {
        let receipt_handle = item.metadata.get("receipt_handle")
            .ok_or(Error::MissingReceiptHandle)?;

        self.client.delete_message()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .send().await?;

        Ok(())
    }
}

impl TryFrom<Message> for Item {
    type Error = Error;

    fn try_from(value: Message) -> result::Result<Self, Self::Error> {
        let mut item = Item {
            id: value.message_id.ok_or(Error::MissingMessageId)?,
            data: Vec::new(),
            metadata: HashMap::new(),
        };

        let receipt_handle = value.receipt_handle.ok_or(Error::MissingReceiptHandle)?;
        item.metadata.insert("receipt_handle".to_string(), receipt_handle);

        if value.body.is_some() {
            item.data = value.body.unwrap().into_bytes();
        }

        if value.message_attributes.is_some() {
            for (key, val) in value.message_attributes.unwrap().iter() {
                if let Some(val) = &val.string_value {
                    log::debug!("[task {}] MessageAttribute[{}] = {}", &item.id, key, val);
                    item.metadata.insert(key.clone(), val.clone());
                }
            }
        }

        if let Some(system_attributes) = value.attributes {
            for (key, val) in system_attributes.iter() {
                log::debug!("[task {}] MessageSystemAttribute[{}] = {}", &item.id, key, val);
                item.metadata.insert(key.to_string(), val.clone());
            }
        }

        Ok(item)
    }
}


//
// Error handling
//

#[derive(Debug, Error)]
pub enum Error {
    #[error("SQS ReceiveMessage API call failed")]
    SqsReceiveMessageError(#[from] ReceiveMessageError),
    #[error("SQS DeleteMessage API call failed")]
    SqsDeleteMessageError(#[from] DeleteMessageError),
    #[error("invalid message model received: missing MessageId")]
    MissingMessageId,
    #[error("invalid message model received: missing ReceiptHandle")]
    MissingReceiptHandle,
}

impl From<SdkError<ReceiveMessageError>> for Error {
    fn from(value: SdkError<ReceiveMessageError>) -> Self {
        Error::SqsReceiveMessageError(value.into_service_error())
    }
}

impl From<SdkError<DeleteMessageError>> for Error {
    fn from(value: SdkError<DeleteMessageError>) -> Self {
        Error::SqsDeleteMessageError(value.into_service_error())
    }
}

pub type Result<T> = result::Result<T, Error>;
