use anyhow::anyhow;
use async_trait::async_trait;
use opensovd_core::{
    Component, Data, DataError, DataFilter, DataProvider, DiscoveryError, DiscoveryProvider,
    EntityCollection, EntityRef, Metadata,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::timeout;
use zenoh::Session;


pub struct ZenohConfig {
    /// The network address of the Zenoh router (e.g., "tcp/127.0.0.1:7447")
    pub endpoint: String,
    /// The selector for discovery (default: "**" for everything)
    pub discovery_selector: String,
    /// Which part of the path is the robot name? (0 = first part)
    pub robot_name_index: usize,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp/localhost:7447".to_string(),
            discovery_selector: "**".to_string(),
            robot_name_index: 0,
        }
    }
}

pub struct ZenohProvider {
    session: Session,
    config: ZenohConfig,
}

impl ZenohProvider {
    /// Creates a new ZenohProvider and establishes the connection to the router.
    pub async fn new(config: ZenohConfig) -> anyhow::Result<Self> {
        let mut zenoh_config = zenoh::Config::default();
        
        // Set mode to client
        zenoh_config.insert_json5("mode", r#""client""#).map_err(|e| anyhow!("{e}"))?;
        
        // Configure endpoints
        let endpoints_json = format!(r#"["{}"]"#, config.endpoint);
        zenoh_config.insert_json5("connect/endpoints", &endpoints_json).map_err(|e| anyhow!("{e}"))?;

        // Open session
        let session = zenoh::open(zenoh_config).await.map_err(|e| anyhow!("{e}"))?;
        tracing::info!("ZenohProvider successfully connected to {}", config.endpoint);
        
        Ok(Self { session, config })
    }
}

#[async_trait]
impl DiscoveryProvider for ZenohProvider {
    /// Discovery via LIVELINESS TOKENS.
    async fn discover(
        &self,
    ) -> Result<
        Pin<Box<dyn futures::stream::Stream<Item = Result<(Vec<EntityRef>, EntityCollection), DiscoveryError>> + Send + 'static>>,
        DiscoveryError,
    > {
        let mut collection = EntityCollection::default();
        
        tracing::info!("Starting discovery via Zenoh Liveliness Tokens with selector: {}", self.config.discovery_selector);

        
        let replies = self.session.liveliness().get(&self.config.discovery_selector)
            .await
            .map_err(|e| DiscoveryError::Other(e.to_string().into()))?;

        let mut robot_map: HashMap<String, Vec<(String, String)>> = HashMap::new();

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let key = sample.key_expr().as_str();
                let parts: Vec<&str> = key.split('/').collect();
                
                
                if let Some(robot_name) = parts.get(self.config.robot_name_index) {
                    let robot_name_str = robot_name.to_string();
                    
                    let data_parts: Vec<&str> = parts.iter()
                        .enumerate()
                        .filter(|&(i, _)| i != self.config.robot_name_index)
                        .map(|(_, &s)| s)
                        .collect();
                    
                    let data_id = data_parts.join("_");
                    
                    if !data_id.is_empty() {
                        robot_map.entry(robot_name_str)
                            .or_default()
                            .push((data_id.clone(), data_id));
                    }
                }
            }
        }

        for (robot_name, points) in robot_map {
            tracing::info!("SOVD component created: '{}' (via Liveliness Token)", robot_name);
            let mut component = Component::new(robot_name.clone(), format!("Zenoh Robot {robot_name}"));
            
            component = component.with_data_provider(ZenohDataProvider {
                session: self.session.clone(),
                key_prefix: robot_name,
                data_points: points,
            });
            collection.components.push(component);
        }

        let stream = futures::stream::once(std::future::ready(Ok((vec![], collection))));
        Ok(Box::pin(stream))
    }
}


pub struct ZenohDataProvider {
    session: Session,
    key_prefix: String,
    data_points: Vec<(String, String)>,
}

#[async_trait]
impl DataProvider for ZenohDataProvider {
    async fn list(&self, _filter: DataFilter) -> Result<Vec<Metadata>, DataError> {
        Ok(self.data_points.iter().map(|(id, name)| Metadata {
            id: id.clone(),
            name: name.clone(),
            category: "zenoh-pubsub".to_string(),
            is_readable: true,
            is_writable: true,
            translation_id: None,
            groups: vec![],
            tags: vec![],
            schema: None,
        }).collect())
    }

    async fn read(&self, data_id: &str, _include_schema: bool) -> Result<Data, DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);
   
        let subscriber = self.session.declare_subscriber(&key)
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;

        tracing::debug!("SOVD Read: Waiting for publication for {}", key);

        // Timeout after 5 seconds
        match timeout(Duration::from_secs(5), subscriber.recv_async()).await {
            Ok(Ok(sample)) => {
                let body = String::from_utf8_lossy(&sample.payload().to_bytes()).into_owned();
                let data: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({"raw_value": body}));
                Ok(Data { data, schema: None })
            }
            Ok(Err(e)) => Err(DataError::Internal(format!("Subscriber Error: {}", e))),
            Err(_) => Err(DataError::NotFound(format!("Timeout: Robot did not send data on '{}'", key))),
        }
    }

   
    async fn write(&self, data_id: &str, value: Value) -> Result<(), DataError> {
        let zenoh_id = data_id.replace('_', "/");
        let key = format!("{}/{}", self.key_prefix, zenoh_id);
        
        self.session.put(&key, value.to_string())
            .await
            .map_err(|e| DataError::Internal(e.to_string()))?;
        
        tracing::info!("SOVD Write: Value published to Zenoh key '{}'", key);
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use opensovd_core::{DataFilter, DataProvider};
    use serde_json::json;

    #[test]
    fn test_zenoh_pubsub_config_default() {
        let config = ZenohConfig::default();
        assert_eq!(config.endpoint, "tcp/localhost:7447");
        assert_eq!(config.discovery_selector, "**");
        assert_eq!(config.robot_name_index, 0);
    }

    #[tokio::test]
    async fn test_zenoh_pubsub_provider_list_metadata() {
        let config = zenoh::Config::default();
        let session = zenoh::open(config).await.unwrap();
        
        let provider = ZenohDataProvider {
            session,
            key_prefix: "TestRobot".to_string(),
            data_points: vec![("sensor_1".to_string(), "Sensor 1".to_string())],
        };

        let metadata = provider.list(DataFilter::default()).await.unwrap();
        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, "sensor_1");
        assert!(metadata[0].is_writable);
        assert!(metadata[0].is_readable);
    }

    #[tokio::test]
    async fn test_zenoh_pubsub_provider_write_success() {
        let config = zenoh::Config::default();
        let session = zenoh::open(config).await.unwrap();
        let provider = ZenohDataProvider { session, key_prefix: "R2".to_string(), data_points: vec![] };

        let result = provider.write("sensor_1", json!({"val": 42})).await;
        assert!(result.is_ok()); // simple Publish should pass w/o errors
    }
}