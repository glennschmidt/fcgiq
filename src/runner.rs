use crate::config::{FieldMappings, FieldSource};
use crate::item::Item;
use crate::pool::{HttpResponse, Pool};
use crate::queue::Queue;
use anyhow::{anyhow, Context};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::sleep;
use tokio::{select, spawn};
use tokio_util::sync::CancellationToken;

/// Asynchronously watches a task queue, and attempts to dispatch tasks as they arrive.
pub struct Runner {
    inner: Arc<_Runner>,
    join_handle: JoinHandle<()>,
}

impl Runner {
    pub fn start(max_tasks: usize, pool: Arc<Pool>, queue: Arc<Queue>, mapping_config: FieldMappings) -> Self {
        let inner = Arc::new(_Runner {
            max_tasks, pool, queue,
            mapping_config: Arc::new(mapping_config),
            cancellation: CancellationToken::new(),
        });

        Self {
            inner: Arc::clone(&inner),
            join_handle: spawn(run(Arc::clone(&inner))),
        }
    }

    pub async fn stop(self) {
        self.inner.cancellation.cancel();
        _ = self.join_handle.await;
    }
}

struct _Runner {
    max_tasks: usize,
    pool: Arc<Pool>,
    queue: Arc<Queue>,
    mapping_config: Arc<FieldMappings>,
    cancellation: CancellationToken,
}

impl _Runner {
    async fn run(&self) {
        let mut tasks = JoinSet::new();
        loop {
            log::debug!("{} of {} workers are busy; polling for new tasks", tasks.len(), self.max_tasks);

            //Poll for items on the queue. Block until one of these events:
            // 1. An item becomes available, or 20 seconds have elapsed and still no items are available
            // 2. The runner receives a stop request
            select! {
                poll_result = self.queue.receive(Duration::from_secs(20)) => {
                    match poll_result {
                        Ok(item) => {
                            if let Some(item) = item {
                                //Spawn a task to handle this item
                                log::debug!("dispatching task {}", &item.id);
                                tasks.spawn(
                                    consume_item(item, Arc::clone(&self.pool), Arc::clone(&self.queue), Arc::clone(&self.mapping_config))
                                );
                            }
                        }
                        Err(error) => {
                            log::error!("An error occurred fetching from the queue (will retry in 5s): {:#}", anyhow!(error));
                            sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
                _ = self.cancellation.cancelled() => {}
            }

            //If all our workers are now busy, block until a task finishes
            while tasks.len() >= self.max_tasks {
                log::debug!("all workers are busy, not polling for new tasks");
                tasks.join_next().await;
            }

            //Clear any finished tasks out of the JoinSet
            while let Some(_) = tasks.try_join_next() {}

            //See if we have received a stop request
            if self.cancellation.is_cancelled() {
                if !tasks.is_empty() {
                    log::info!("Waiting for {} tasks to finish...", tasks.len());
                    tasks.join_all().await;
                    log::info!("All tasks complete.");
                }
                break;
            }
        }
    }
}

async fn run(runner: Arc<_Runner>) {
    runner.run().await
}

async fn consume_item(item: Item, pool: Arc<Pool>, queue: Arc<Queue>, mapping_config: Arc<FieldMappings>) {
    let item_id = item.id.clone();

    //Dispatch the task to the FastCGI pool
    let result = async move {
        let mut env = HashMap::new();
        for (key, field_mapping) in mapping_config.iter() {
            let val = match field_mapping.source {
                FieldSource::BodyJson => item.get_string_from_data_json_object(&field_mapping.field),
                FieldSource::Metadata => item.metadata.get(&field_mapping.field).cloned(),
            };
            if let Some(val) = val {
                log::debug!("[task {}] env override: {}={}", &item.id, key, &val);
                env.insert(key.to_owned(), val);
            }
        }

        let result = pool.dispatch(&item.data, env).await?;

        if let Some(stderr) = result.stderr_string() {
            if !stderr.is_empty() {
                log::warn!("[task {}] {}", &item.id, stderr);
            }
        }

        if let Some(stdout) = result.stdout_string() {
            log::debug!("[task {}] stdout: {}", &item.id, stdout);
        }

        let http_response: HttpResponse = result.try_into()?;
        if !http_response.status().is_success() {
            return Err(anyhow::anyhow!("script returned status code {}", http_response.status()));
        }

        if let Ok(body_string) = String::from_utf8(http_response.into_body()) {
            log::info!("[task {}] task complete: {}", &item.id, body_string);
        } else {
            log::info!("[task {}] task complete", &item.id);
        }

        Ok(item)
    }.await;

    //If the task was successful, remove it from the queue. Otherwise, log the failure.
    match result.context("task failed") {
        Ok(item) => {
            let delete_result = queue.acknowledge(&item).await
                .context("failed to remove task from queue");
            if let Err(e) = delete_result {
                log::error!("[task {}] {:#}", item_id, e);
            }
        },
        Err(e) => {
            log::error!("[task {}] {:#}", item_id, e);
        }
    };
}